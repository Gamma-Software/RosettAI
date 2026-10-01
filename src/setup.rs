use crate::{Action, plan_sync, sync};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const HOOK_MARKER: &str = "# rai-managed-hook";

pub enum Diagnostic {
    ConfigUnavailable,
    ConfigUnreadable,
    NoRoots,
    WorkspaceMissing(PathBuf),
    WatcherMissing,
}

pub fn can_run_setup() -> bool {
    env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
        .is_some_and(|path| {
            !path
                .components()
                .any(|component| component.as_os_str() == "target")
        })
}

pub fn setup(mut roots: Vec<PathBuf>) -> Result<(), String> {
    if roots.is_empty() {
        print!("Workspace directory to watch: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;
        if input.trim().is_empty() {
            return Err("provide a workspace directory with --root PATH".into());
        }
        roots.push(PathBuf::from(input.trim()));
    }
    let executable = env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if executable
        .components()
        .any(|component| component.as_os_str() == "target")
    {
        return Err(
            "install rai first with `cargo install --path .`, then run `rai install`".into(),
        );
    }
    let config = config_dir()?;
    if config.is_symlink() {
        return Err(format!(
            "symlink configuration conflict: {}",
            config.display()
        ));
    }
    fs::create_dir_all(&config).map_err(|e| format!("{}: {e}", config.display()))?;
    let mut saved = read_roots(&config)?;
    for root in roots {
        let root = root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.display()))?;
        if !root.is_dir() {
            return Err(format!("not a directory: {}", root.display()));
        }
        if root.to_string_lossy().contains('\n') {
            return Err("workspace path cannot contain a newline".into());
        }
        if !saved.contains(&root) {
            saved.push(root);
        }
    }
    saved.sort();
    write_roots(&config, &saved)?;
    crate::root_tracking::remember(&config, &saved)?;
    println!(
        "Watching {} workspace director{}",
        saved.len(),
        if saved.len() == 1 { "y" } else { "ies" }
    );
    install_hook_template(&config, &executable)?;
    install_watcher(&config, &executable)?;
    Ok(())
}

pub fn watch(perf: bool) -> Result<(), String> {
    let config = config_dir()?;
    let roots = read_roots(&config)?;
    if roots.is_empty() && !config.join("roots.txt").exists() {
        return Err("no workspace roots configured; run rai install --root PATH".into());
    }
    let mut last_relocation_scan = None;
    loop {
        let snapshot = perf.then(crate::perf::Snapshot::start);
        let current = read_roots(&config)?;
        let scan = last_relocation_scan
            .is_none_or(|last: Instant| last.elapsed() >= Duration::from_secs(300));
        let (resolved, missing) =
            crate::root_tracking::resolve_with_search(&config, &current, scan)?;
        if scan {
            last_relocation_scan = Some(Instant::now());
            for root in missing {
                eprintln!(
                    "rai watch: configured workspace could not be found: {}",
                    root.display()
                );
            }
        }
        watch_once(&resolved);
        if let Some(snapshot) = snapshot {
            snapshot.emit();
        }
        thread::sleep(Duration::from_secs(30));
    }
}

pub fn repair_existing() -> Result<(), String> {
    let roots = read_roots(&config_dir()?)?;
    if roots.is_empty() {
        return Err("no workspace roots configured".into());
    }
    setup(roots)
}

pub fn change_workspace(old: &Path, replacement: Option<&Path>) -> Result<(), String> {
    crate::root_tracking::change_root(&config_dir()?, old, replacement)
}

fn watch_once(roots: &[PathBuf]) {
    for root in roots {
        for repo in discover(root, 5) {
            if let Err(error) = plan_sync(&repo).and_then(|changes| {
                if changes
                    .iter()
                    .all(|change| change.action == Action::Unchanged)
                {
                    Ok(())
                } else {
                    sync(&repo, false)
                }
            }) {
                eprintln!("rai watch: {}: {error}", repo.display());
            }
        }
    }
}

fn discover(root: &Path, depth: usize) -> Vec<PathBuf> {
    if depth == 0 || !root.is_dir() || root.is_symlink() {
        return Vec::new();
    }
    if root.join(".agents").is_dir() {
        return vec![root.to_path_buf()];
    }
    if root.join(".git").exists() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || matches!(name.as_ref(), "node_modules" | "target" | "vendor") {
            continue;
        }
        found.extend(discover(&path, depth - 1));
    }
    found
}

fn config_dir() -> Result<PathBuf, String> {
    crate::user_data::config_dir()
}

fn read_roots(config: &Path) -> Result<Vec<PathBuf>, String> {
    let path = config.join("roots.txt");
    if !path.exists() {
        return Ok(Vec::new());
    }
    if path.is_symlink() {
        return Err(format!(
            "symlink configuration conflict: {}",
            path.display()
        ));
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(content
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn write_roots(config: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let content = roots
        .iter()
        .map(|root| root.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(config.join("roots.txt"), format!("{content}\n")).map_err(|e| e.to_string())
}

fn git_global(name: &str) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .args(["config", "--global", "--get", name])
        .output()
        .map_err(|e| format!("git config: {e}"))?;
    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )),
        Some(1) => Ok(None),
        _ => Err(format!("git config --global --get {name} failed")),
    }
}

fn install_hook_template(config: &Path, executable: &Path) -> Result<(), String> {
    if git_global("core.hooksPath")?.is_some() {
        println!(
            "Git hooks: skipped (global core.hooksPath is already set); watcher remains active"
        );
        return Ok(());
    }
    let template = config.join("git-template");
    if let Some(existing) = git_global("init.templateDir")?
        && Path::new(&existing) != template
    {
        println!(
            "Git hooks: skipped (global init.templateDir is already set); watcher remains active"
        );
        return Ok(());
    }
    let hooks = template.join("hooks");
    if template.is_symlink() || hooks.is_symlink() {
        return Err(format!(
            "symlink Git template conflict: {}",
            template.display()
        ));
    }
    fs::create_dir_all(&hooks).map_err(|e| format!("{}: {e}", hooks.display()))?;
    let quoted = shell_quote(&executable.to_string_lossy());
    for name in ["post-checkout", "post-merge", "post-rewrite"] {
        let path = hooks.join(name);
        if path.is_symlink() {
            return Err(format!("symlink Git hook conflict: {}", path.display()));
        }
        if path.exists()
            && !fs::read_to_string(&path)
                .map_err(|e| e.to_string())?
                .starts_with(&format!("#!/bin/sh\n{HOOK_MARKER}\n"))
        {
            return Err(format!("unowned Git hook conflict: {}", path.display()));
        }
        let content = format!(
            "#!/bin/sh\n{HOOK_MARKER}\nif [ -d .agents ]; then\n  {quoted} sync --repo . >/dev/null || printf '%s\\n' 'rai sync failed; run rai doctor' >&2\nfi\nexit 0\n"
        );
        fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    let status = Command::new("git")
        .args(["config", "--global", "init.templateDir"])
        .arg(&template)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("could not configure Git template directory".into());
    }
    for name in ["post-checkout", "post-merge", "post-rewrite"] {
        crate::hook_registry::remember_git_template(&hooks.join(name), name)?;
    }
    println!("Git hooks: installed for future clones");
    Ok(())
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
fn install_watcher(_config: &Path, executable: &Path) -> Result<(), String> {
    let home = env::var_os("HOME").ok_or("HOME is not set")?;
    let agents = PathBuf::from(home).join("Library/LaunchAgents");
    fs::create_dir_all(&agents).map_err(|e| format!("{}: {e}", agents.display()))?;
    let plist = agents.join("ai.rosettai.rai.plist");
    if plist.is_symlink() {
        return Err(format!(
            "symlink launch agent conflict: {}",
            plist.display()
        ));
    }
    if plist.exists() {
        let old = fs::read_to_string(&plist).map_err(|e| e.to_string())?;
        if !old.contains("<!-- rai-managed-launch-agent -->") {
            return Err(format!(
                "unowned launch agent conflict: {}",
                plist.display()
            ));
        }
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<!-- rai-managed-launch-agent -->\n<key>Label</key><string>ai.rosettai.rai</string>\n<key>ProgramArguments</key><array><string>{}</string><string>watch</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n</dict></plist>\n",
        xml_escape(&executable.to_string_lossy())
    );
    fs::write(&plist, content).map_err(|e| format!("{}: {e}", plist.display()))?;
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| e.to_string())?;
    if !uid.status.success() {
        return Err("could not determine user ID for launchd".into());
    }
    let domain = format!("gui/{}", String::from_utf8_lossy(&uid.stdout).trim());
    let _ = Command::new("launchctl")
        .args(["bootout", &domain])
        .arg(&plist)
        .output();
    let result = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&plist)
        .output()
        .map_err(|e| e.to_string())?;
    if result.status.success() {
        println!("Watcher: started via launchd");
        Ok(())
    } else {
        Err(format!(
            "launchd could not start watcher: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
    }
}

#[cfg(target_os = "linux")]
fn install_watcher(_config: &Path, executable: &Path) -> Result<(), String> {
    let services = systemd_user_dir()?;
    fs::create_dir_all(&services).map_err(|e| format!("{}: {e}", services.display()))?;
    let service = services.join("rai-watch.service");
    if service.is_symlink() {
        return Err(format!(
            "symlink watcher service conflict: {}",
            service.display()
        ));
    }
    if service.exists()
        && !fs::read_to_string(&service)
            .map_err(|e| e.to_string())?
            .starts_with("# rai-managed-watcher\n")
    {
        return Err(format!(
            "unowned watcher service conflict: {}",
            service.display()
        ));
    }
    let escaped = executable
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    fs::write(&service, format!("# rai-managed-watcher\n[Unit]\nDescription=RosettAI workspace watcher\n\n[Service]\nExecStart=\"{escaped}\" watch\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n"))
        .map_err(|e| format!("{}: {e}", service.display()))?;
    for args in [
        vec!["--user", "daemon-reload"],
        vec!["--user", "enable", "--now", "rai-watch.service"],
        vec!["--user", "restart", "rai-watch.service"],
    ] {
        let output = Command::new("systemctl")
            .args(&args)
            .output()
            .map_err(|e| format!("systemctl: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "systemctl {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    println!("Watcher: started via systemd user service");
    Ok(())
}

#[cfg(target_os = "linux")]
fn systemd_user_dir() -> Result<PathBuf, String> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("systemd/user"));
    }
    let home = env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/systemd/user"))
}

#[cfg(windows)]
fn install_watcher(_config: &Path, executable: &Path) -> Result<(), String> {
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$name = 'RosettAI Watcher'
$existing = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
if ($existing -and $existing.Description -ne 'rai-managed-watcher') {
    throw 'unowned watcher task conflict: RosettAI Watcher'
}
$action = New-ScheduledTaskAction -Execute $env:RAI_WATCH_EXECUTABLE -Argument 'watch'
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Seconds 0) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName $name -Action $action -Trigger $trigger -Settings $settings -Description 'rai-managed-watcher' -Force | Out-Null
Start-ScheduledTask -TaskName $name
"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("RAI_WATCH_EXECUTABLE", executable)
        .output()
        .map_err(|e| format!("PowerShell: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "could not install watcher task: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    println!("Watcher: started via Windows Task Scheduler");
    Ok(())
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn diagnostics() -> Vec<Diagnostic> {
    let Ok(config) = config_dir() else {
        return vec![Diagnostic::ConfigUnavailable];
    };
    let Ok(roots) = read_roots(&config) else {
        return vec![Diagnostic::ConfigUnreadable];
    };
    if roots.is_empty() && !config.join("roots.txt").exists() {
        return vec![Diagnostic::NoRoots];
    }
    let mut issues = Vec::new();
    let Ok((_, missing)) = crate::root_tracking::resolve(&config, &roots) else {
        return vec![Diagnostic::ConfigUnreadable];
    };
    for root in missing {
        issues.push(Diagnostic::WorkspaceMissing(root));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        let plist = PathBuf::from(home).join("Library/LaunchAgents/ai.rosettai.rai.plist");
        if !plist.exists() {
            issues.push(Diagnostic::WatcherMissing);
        }
    }
    #[cfg(target_os = "linux")]
    {
        if systemd_user_dir().map_or(true, |dir| !dir.join("rai-watch.service").exists()) {
            issues.push(Diagnostic::WatcherMissing);
        }
    }
    #[cfg(windows)]
    {
        let installed = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-ScheduledTask -TaskName 'RosettAI Watcher' -ErrorAction SilentlyContinue).Description"])
            .output()
            .is_ok_and(|output| output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "rai-managed-watcher");
        if !installed {
            issues.push(Diagnostic::WatcherMissing);
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_repository_without_entering_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("projects/example");
        fs::create_dir_all(repo.join(".agents/rules")).unwrap();
        fs::write(repo.join(".agents/rules/general.md"), "Rule\n").unwrap();
        assert_eq!(discover(dir.path(), 4), vec![repo.clone()]);
        assert_eq!(
            crate::find_repo(&repo).unwrap(),
            repo.canonicalize().unwrap()
        );
    }

    #[test]
    fn quotes_shell_paths() {
        assert_eq!(shell_quote("a'b"), "'a'\"'\"'b'");
    }

    #[test]
    fn watcher_projects_discovered_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("projects/example");
        fs::create_dir_all(repo.join(".agents/rules")).unwrap();
        fs::write(repo.join(".agents/rules/general.md"), "Watch me.\n").unwrap();
        watch_once(&[dir.path().to_path_buf()]);
        assert!(
            fs::read_to_string(repo.join("AGENTS.md"))
                .unwrap()
                .contains("Watch me.")
        );
    }
}
