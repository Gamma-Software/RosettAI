//! Streaming command transcripts. Each invocation owns a file, so concurrent
//! hooks/watchers never interleave records or overwrite another command's log.
use chrono::Utc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

macro_rules! print {
    ($($arg:tt)*) => { crate::command_log::print("stdout", format_args!($($arg)*)) };
}
macro_rules! println {
    () => { print!("\n") };
    ($($arg:tt)*) => { print!("{}\n", format_args!($($arg)*)) };
}
macro_rules! eprint {
    ($($arg:tt)*) => { crate::command_log::print("stderr", format_args!($($arg)*)) };
}
macro_rules! eprintln {
    () => { eprint!("\n") };
    ($($arg:tt)*) => { eprint!("{}\n", format_args!($($arg)*)) };
}

type Handle = Arc<Mutex<Transcript>>;
thread_local! {
    static ACTIVE: RefCell<Option<Handle>> = const { RefCell::new(None) };
}
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Transcript {
    file: Option<File>,
    path: PathBuf,
}

impl Transcript {
    fn append(&mut self, mut event: Value) {
        event["timestamp"] = Utc::now().to_rfc3339().into();
        let Some(file) = &mut self.file else { return };
        let result = serde_json::to_vec(&event)
            .map_err(io::Error::other)
            .and_then(|mut bytes| {
                bytes.push(b'\n');
                file.write_all(&bytes)
            });
        if let Err(error) = result {
            self.file = None;
            std::eprintln!(
                "rai: cannot write command log {}: {error}",
                self.path.display()
            );
        }
    }
}

fn current() -> Option<Handle> {
    ACTIVE.with(|active| active.borrow().clone())
}

pub struct OutputScope(Option<Handle>);

impl Drop for OutputScope {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = self.0.take());
    }
}

/// Carry the transcript into a terminal progress thread without starting a
/// second command or sharing thread-local state with unrelated unit tests.
pub fn inherit_output() -> impl FnOnce() -> OutputScope {
    let inherited = current();
    move || OutputScope(ACTIVE.with(|active| active.replace(inherited)))
}

pub struct Session {
    handle: Option<Handle>,
    previous: Option<Handle>,
    started: Instant,
    finished: bool,
}

impl Session {
    pub fn start(args: &[String], project: Option<&Path>, source: &str) -> Self {
        // Unit tests call synchronization directly, without a CLI environment.
        #[cfg(test)]
        if current().is_none() {
            return Self::inactive();
        }
        match crate::user_data::logs_dir().and_then(|base| {
            Self::start_in(&base, args, project, source).map_err(|e| e.to_string())
        }) {
            Ok(session) => session,
            Err(error) => {
                std::eprintln!("rai: cannot create command log: {error}");
                Self::inactive()
            }
        }
    }

    fn inactive() -> Self {
        Self {
            handle: None,
            previous: None,
            started: Instant::now(),
            finished: false,
        }
    }

    fn start_in(
        base: &Path,
        args: &[String],
        project: Option<&Path>,
        source: &str,
    ) -> io::Result<Self> {
        let started = Instant::now();
        let project =
            project.map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
        if let Some(parent) = base.parent() {
            fs::create_dir_all(parent)?;
        }
        private_directory(base)?;
        let directory = match &project {
            Some(project) => {
                let projects = base.join("projects");
                private_directory(&projects)?;
                projects.join(project_key(project))
            }
            None => base.join("global"),
        };
        private_directory(&directory)?;
        let name = format!(
            "{}-{}-{}.jsonl",
            Utc::now().format("%Y-%m-%d_%H-%M-%S%.9f"),
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let path = directory.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        let previous = current();
        let parent = previous
            .as_ref()
            .map(|handle| handle.lock().unwrap().path.clone());
        let mut transcript = Transcript {
            file: Some(file),
            path,
        };
        transcript.append(json!({
            "event": "start", "args": args, "project": project,
            "cwd": std::env::current_dir().ok(), "pid": std::process::id(),
            "version": env!("CARGO_PKG_VERSION"), "source": source, "parent": parent
        }));
        let handle = Arc::new(Mutex::new(transcript));
        ACTIVE.with(|active| *active.borrow_mut() = Some(handle.clone()));
        Ok(Self {
            handle: Some(handle),
            previous,
            started,
            finished: false,
        })
    }

    pub fn finish(mut self, result: &Result<(), String>) {
        self.append(json!({
            "event": "finish", "exitCode": if result.is_ok() { 0 } else { 1 },
            "durationMs": self.started.elapsed().as_secs_f64() * 1000.0,
            "error": result.as_ref().err()
        }));
        self.finished = true;
    }

    fn append(&self, event: Value) {
        if let Some(handle) = &self.handle {
            handle.lock().unwrap().append(event);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.finished {
            self.append(json!({"event": "interrupted", "durationMs": self.started.elapsed().as_secs_f64() * 1000.0}));
        }
        if self.handle.is_some() {
            ACTIVE.with(|active| *active.borrow_mut() = self.previous.take());
        }
    }
}

fn project_key(project: &Path) -> String {
    let name: String = project
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let digest = Sha256::digest(project.as_os_str().as_encoded_bytes());
    format!(
        "{}-{:x}",
        if name.is_empty() { "project" } else { &name },
        digest
    )
}

fn private_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => Ok(()),
        Ok(_) => Err(io::Error::other(format!(
            "not a regular log directory: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    private_directory(path)
                }
                result => result,
            }
        }
        Err(error) => Err(error),
    }
}

fn record(handle: &Option<Handle>, stream: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if let Some(handle) = handle {
        handle.lock().unwrap().append(
            json!({"event": "output", "stream": stream, "text": String::from_utf8_lossy(bytes)}),
        );
    }
}

pub fn print(stream: &str, args: std::fmt::Arguments<'_>) {
    let text = args.to_string();
    record(&current(), stream, text.as_bytes());
    if stream == "stdout" {
        std::print!("{text}");
    } else {
        std::eprint!("{text}");
    }
}

pub struct Output<W> {
    inner: W,
    handle: Option<Handle>,
}

pub fn stdout() -> Output<io::StdoutLock<'static>> {
    Output {
        inner: io::stdout().lock(),
        handle: current(),
    }
}

impl<W: Write> Write for Output<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(bytes)?;
        record(&self.handle, "stdout", &bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: IsTerminal> Output<W> {
    pub fn is_terminal(&self) -> bool {
        self.inner.is_terminal()
    }
}

/// Preserve output from short machine-management utilities that previously
/// inherited the terminal directly, so it is included in the command result.
pub trait CommandExt {
    fn logged_status(&mut self) -> io::Result<ExitStatus>;
}

impl CommandExt for Command {
    fn logged_status(&mut self) -> io::Result<ExitStatus> {
        let output = self.stdin(Stdio::inherit()).output()?;
        record(&current(), "stdout", &output.stdout);
        record(&current(), "stderr", &output.stderr);
        io::stdout().lock().write_all(&output.stdout)?;
        io::stderr().lock().write_all(&output.stderr)?;
        Ok(output.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(path: &Path) -> Vec<Value> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn nested_sessions_restore_output_routing_and_link_to_their_parent() {
        let sandbox = tempfile::tempdir().unwrap();
        let base = sandbox.path().join("logs");
        let parent = Session::start_in(&base, &["watch".into()], None, "cli").unwrap();
        let parent_path = parent.handle.as_ref().unwrap().lock().unwrap().path.clone();
        record(&current(), "stdout", b"before\n");
        let child =
            Session::start_in(&base, &["sync".into()], Some(sandbox.path()), "watcher").unwrap();
        let child_path = child.handle.as_ref().unwrap().lock().unwrap().path.clone();
        let mut writer = Output {
            inner: Vec::new(),
            handle: current(),
        };
        writer.write_all("child ✓\n".as_bytes()).unwrap();
        child.finish(&Err("conflict".into()));
        record(&current(), "stdout", b"after\n");
        parent.finish(&Ok(()));
        assert!(current().is_none());
        let parent_events = events(&parent_path);
        let child_events = events(&child_path);
        assert_eq!(
            parent_events
                .iter()
                .filter_map(|event| event["text"].as_str())
                .collect::<String>(),
            "before\nafter\n"
        );
        assert_eq!(
            child_events[0]["parent"],
            parent_path.to_string_lossy().as_ref()
        );
        assert_eq!(child_events[1]["text"], "child ✓\n");
        assert_eq!(child_events.last().unwrap()["exitCode"], 1);
        assert_eq!(parent_events.last().unwrap()["exitCode"], 0);
    }

    #[cfg(unix)]
    #[test]
    fn linked_log_directories_are_preserved_without_writing_to_their_target() {
        let sandbox = tempfile::tempdir().unwrap();
        let target = sandbox.path().join("target");
        fs::create_dir(&target).unwrap();
        let base = sandbox.path().join("logs");
        std::os::unix::fs::symlink(&target, &base).unwrap();
        assert!(Session::start_in(&base, &[], None, "cli").is_err());
        assert!(base.is_symlink());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        fs::remove_file(&base).unwrap();
        fs::create_dir(&base).unwrap();
        std::os::unix::fs::symlink(&target, base.join("projects")).unwrap();
        assert!(Session::start_in(&base, &[], Some(sandbox.path()), "cli").is_err());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        assert!(current().is_none());
    }
}
