use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod perf;
mod setup;

const CLAUDE: &str = "CLAUDE.md";
const CURSOR: &str = ".cursor/rules/rosettai.mdc";
const IGNORE_START: &str = "# RosettAI generated files";
const IGNORE_END: &str = "# End RosettAI generated files";
const CURSOR_FRONTMATTER: &str =
    "---\ndescription: Shared RosettAI rules\nalwaysApply: true\n---\n";
const MARKER_START: &str = "<!-- rai-generated sha256:";

#[derive(Debug, PartialEq)]
enum Action {
    Create,
    Update,
    Unchanged,
}

struct Change {
    path: PathBuf,
    content: String,
    action: Action,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let snapshot = args
        .iter()
        .any(|arg| arg == "--perf")
        .then(perf::Snapshot::start);
    let result = run(args);
    if let Err(error) = &result {
        eprintln!("rai: {error}");
    }
    if let Some(snapshot) = snapshot {
        snapshot.emit();
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let mut args = args.into_iter();
    let command = args.next().ok_or_else(usage)?;
    let mut dry_run = false;
    let mut json = false;
    let mut perf = false;
    let mut cursor_hook = false;
    let mut repo = None;
    let mut roots = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--perf" => perf = true,
            "--cursor-hook" => cursor_hook = true,
            "--repo" => repo = Some(PathBuf::from(args.next().ok_or("--repo needs a path")?)),
            "--root" => roots.push(PathBuf::from(args.next().ok_or("--root needs a path")?)),
            _ => return Err(format!("unknown option: {arg}")),
        }
    }
    if dry_run && command != "sync" {
        return Err("--dry-run is only valid with sync".into());
    }
    if json && !matches!(command.as_str(), "sync" | "status" | "doctor") {
        return Err("--json is only valid with sync, status, or doctor".into());
    }
    if !roots.is_empty() && command != "setup" {
        return Err("--root is only valid with setup".into());
    }
    if cursor_hook && command != "sync" {
        return Err("--cursor-hook is only valid with sync".into());
    }
    if cursor_hook && (dry_run || json) {
        return Err("--cursor-hook cannot be combined with --dry-run or --json".into());
    }
    if repo.is_some() && command == "setup" {
        return Err("--repo is not valid with setup".into());
    }
    if command == "setup" {
        return setup::setup(roots);
    }
    if command == "watch" {
        if repo.is_some() || dry_run || json {
            return Err("watch takes no options".into());
        }
        return setup::watch(perf);
    }
    let start = repo.unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
    if command == "init" {
        return init(&start);
    }
    let root = if cursor_hook {
        match find_repo(&start) {
            Ok(root) => root,
            Err(error) => {
                print_cursor_hook_result(
                    false,
                    &format!("RosettAI cannot check this prompt: {error}"),
                );
                return Ok(());
            }
        }
    } else if command == "doctor" {
        find_repo(&start).or_else(|_| {
            let start = start.canonicalize().map_err(|e| e.to_string())?;
            Ok::<PathBuf, String>(git_root(&start).unwrap_or(start))
        })?
    } else {
        find_repo(&start)?
    };
    match command.as_str() {
        "sync" if cursor_hook => sync_cursor_hook(&root),
        "sync" => sync_with_format(&root, dry_run, json),
        "status" => status(&root, json),
        "doctor" => doctor(&root, json),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: rai <setup|init|status|sync|doctor> [--repo PATH] [--root PATH] [--dry-run] [--json] [--cursor-hook] [--perf]"
        .into()
}

fn find_repo(start: &Path) -> Result<PathBuf, String> {
    let start = start
        .canonicalize()
        .map_err(|e| format!("{}: {e}", start.display()))?;
    let start = if start.is_file() {
        start.parent().ok_or("no parent directory")?.to_path_buf()
    } else {
        start
    };
    let boundary = git_root(&start).unwrap_or_else(|| start.clone());
    start
        .ancestors()
        .take_while(|path| path.starts_with(&boundary))
        .find(|path| path.join(".agents").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("no .agents/ directory found inside {}", boundary.display()))
}

fn git_root(start: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    PathBuf::from(String::from_utf8(output.stdout).ok()?.trim())
        .canonicalize()
        .ok()
}

fn sync(root: &Path, dry_run: bool) -> Result<(), String> {
    sync_with_format(root, dry_run, false)
}

fn sync_with_format(root: &Path, dry_run: bool, json: bool) -> Result<(), String> {
    let changes = match plan_sync(root) {
        Ok(changes) => changes,
        Err(error) => {
            if json {
                println!("{{\"ok\":false,\"error\":{}}}", json_string(&error));
            }
            return Err(error);
        }
    };
    if json {
        print_changes_json(root, &changes, dry_run);
    } else {
        for change in &changes {
            let relative = change
                .path
                .strip_prefix(root)
                .expect("planned path inside repo");
            println!("{:?} {}", change.action, relative.display());
        }
        if dry_run {
            println!("dry-run: no files written");
        }
    }
    if dry_run {
        return Ok(());
    }
    apply_changes(changes)
}

fn apply_changes(changes: Vec<Change>) -> Result<(), String> {
    for change in changes {
        if change.action == Action::Unchanged {
            continue;
        }
        if let Some(parent) = change.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&change.path, change.content)
            .map_err(|e| format!("{}: {e}", change.path.display()))?;
    }
    Ok(())
}

fn sync_cursor_hook(root: &Path) -> Result<(), String> {
    // Drain Cursor's JSON event payload so a large prompt cannot leave the writer blocked.
    // The project hook runs from the repository root, so repository discovery remains the
    // authoritative source of the path rather than trusting data from stdin.
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("cannot read Cursor hook input: {e}"))?;

    let changes = match plan_sync(root) {
        Ok(changes) => changes,
        Err(error) => {
            print_cursor_hook_result(
                false,
                &format!("RosettAI synchronization is blocked: {error}"),
            );
            return Ok(());
        }
    };
    if changes
        .iter()
        .all(|change| change.action == Action::Unchanged)
    {
        println!("{{\"continue\":true}}");
        return Ok(());
    }
    let changed_paths = changes
        .iter()
        .filter(|change| change.action != Action::Unchanged)
        .map(|change| {
            change
                .path
                .strip_prefix(root)
                .expect("planned path inside repo")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    match apply_changes(changes) {
        Ok(()) => print_cursor_hook_result(
            false,
            &format!(
                "RosettAI synchronized {}. Resubmit your prompt so Cursor resolves the updated rules before calling the model.",
                changed_paths.join(", ")
            ),
        ),
        Err(error) => {
            print_cursor_hook_result(false, &format!("RosettAI synchronization failed: {error}"))
        }
    }
    Ok(())
}

fn print_cursor_hook_result(continue_prompt: bool, message: &str) {
    println!(
        "{{\"continue\":{continue_prompt},\"user_message\":{}}}",
        json_string(message)
    );
}

fn plan_sync(root: &Path) -> Result<Vec<Change>, String> {
    let rules = read_rules(&root.join(".agents/rules"))?;
    let body = format!("# Shared project rules\n\n{rules}");
    let outputs = [
        (CLAUDE, owned(&body, "")),
        (CURSOR, owned(&body, CURSOR_FRONTMATTER)),
    ];

    // Plan every write before applying any of them. A single collision aborts the sync.
    let mut changes = Vec::new();
    for (relative, content) in outputs {
        let path = root.join(relative);
        if is_tracked(root, relative)? {
            return Err(format!(
                "tracked output conflict: {relative}; remove it from Git tracking first"
            ));
        }
        if path
            .ancestors()
            .take_while(|part| *part != root)
            .any(Path::is_symlink)
        {
            return Err(format!("symlink output conflict: {relative}"));
        }
        let action = if path.exists() {
            let current =
                fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if !is_owned(
                &current,
                if relative == CURSOR {
                    CURSOR_FRONTMATTER
                } else {
                    ""
                },
            ) {
                return Err(format!("unowned or modified output conflict: {relative}"));
            }
            if current == content {
                Action::Unchanged
            } else {
                Action::Update
            }
        } else {
            Action::Create
        };
        changes.push(Change {
            path,
            content,
            action,
        });
    }

    let ignore_path = root.join(".gitignore");
    if ignore_path.is_symlink() {
        return Err("symlink output conflict: .gitignore".into());
    }
    let old_ignore = if ignore_path.exists() {
        fs::read_to_string(&ignore_path).map_err(|e| format!(".gitignore: {e}"))?
    } else {
        String::new()
    };
    let new_ignore = update_ignore(&old_ignore)?;
    let ignore_action = if old_ignore == new_ignore {
        Action::Unchanged
    } else if ignore_path.exists() {
        Action::Update
    } else {
        Action::Create
    };
    changes.push(Change {
        path: ignore_path,
        content: new_ignore,
        action: ignore_action,
    });

    Ok(changes)
}

fn init(start: &Path) -> Result<(), String> {
    let start = start
        .canonicalize()
        .map_err(|e| format!("{}: {e}", start.display()))?;
    if !start.is_dir() {
        return Err(format!("not a directory: {}", start.display()));
    }
    let root = git_root(&start).unwrap_or(start);
    let agents = root.join(".agents");
    if agents.exists() || agents.is_symlink() {
        return Err(format!("already exists: {}", agents.display()));
    }
    fs::create_dir(&agents).map_err(|e| format!("{}: {e}", agents.display()))?;
    fs::create_dir(agents.join("rules")).map_err(|e| e.to_string())?;
    fs::write(
        agents.join("rules/general.md"),
        "# Project rules\n\nFollow the repository's existing conventions. Keep changes focused and run the relevant tests.\n",
    )
    .map_err(|e| e.to_string())?;
    println!("Created {}", agents.display());
    println!("Edit .agents/rules/general.md, then run rai sync --dry-run");
    Ok(())
}

fn status(root: &Path, json: bool) -> Result<(), String> {
    let changes = match plan_sync(root) {
        Ok(changes) => changes,
        Err(error) => {
            if json {
                println!("{{\"ok\":false,\"conflict\":{}}}", json_string(&error));
            } else {
                println!("Conflict: {error}");
            }
            return Err("status has a conflict".into());
        }
    };
    let in_sync = changes
        .iter()
        .all(|change| change.action == Action::Unchanged);
    if json {
        print!("{{\"ok\":true,\"inSync\":{in_sync},\"changes\":");
        print_changes_array(root, &changes);
        println!("}}");
    } else {
        for change in &changes {
            println!(
                "{:?} {}",
                change.action,
                change.path.strip_prefix(root).unwrap().display()
            );
        }
        println!("{}", if in_sync { "In sync" } else { "Sync needed" });
    }
    Ok(())
}

#[derive(Clone)]
enum DoctorFix {
    Init,
    Sync,
    SetupNew(PathBuf),
    SetupExisting,
}

struct DoctorIssue {
    message: String,
    solution: String,
    fix: Option<DoctorFix>,
}

fn inspect_doctor(root: &Path) -> (Vec<DoctorIssue>, Vec<Change>) {
    let mut issues = Vec::new();
    let mut changes = Vec::new();
    if !root.join(".agents").exists() {
        issues.push(DoctorIssue {
            message: "no .agents/ directory in this repository".into(),
            solution: "Run rai init to create a starter .agents/rules/general.md; review and customize its content after initialization.".into(),
            fix: Some(DoctorFix::Init),
        });
    } else {
        match plan_sync(root) {
            Ok(planned) => changes = planned,
            Err(error) => issues.push(DoctorIssue {
                solution: solution_for_plan_error(&error),
                message: error,
                fix: None,
            }),
        }
    }
    if changes
        .iter()
        .any(|change| change.action != Action::Unchanged)
    {
        issues.push(DoctorIssue {
            message: "generated projections are out of sync".into(),
            solution:
                "Run rai sync to update only rai-owned outputs and the managed .gitignore block."
                    .into(),
            fix: Some(DoctorFix::Sync),
        });
    }
    if root.join("AGENTS.md").exists() {
        issues.push(DoctorIssue {
            message: "unmanaged AGENTS.md exists".into(),
            solution: "Review and copy its instructions into .agents/rules/, then remove or relocate AGENTS.md after confirming the new source. Automatic import is not implemented.".into(),
            fix: None,
        });
    }
    for issue in setup::diagnostics() {
        let can_setup = setup::can_run_setup();
        let (message, solution, fix) = match issue {
            setup::Diagnostic::ConfigUnavailable => (
                "cannot locate per-user setup configuration".into(),
                "Set HOME or XDG_CONFIG_HOME, then run rai setup --root PATH.".into(),
                None,
            ),
            setup::Diagnostic::ConfigUnreadable => (
                "cannot read configured workspace roots".into(),
                "Inspect the per-user rai/roots.txt file and repair its permissions or contents before rerunning setup.".into(),
                None,
            ),
            setup::Diagnostic::NoRoots => {
                let workspace = root.to_path_buf();
                let command = format!("rai setup --root {}", setup::shell_quote(&workspace.to_string_lossy()));
                (
                    "rai setup has not configured any workspace roots".into(),
                    if can_setup { format!("Run {command} to watch this repository and install available integrations.") } else { format!("Install rai with cargo install --path ., then run {command}.") },
                    can_setup.then_some(DoctorFix::SetupNew(workspace)),
                )
            }
            setup::Diagnostic::WorkspaceMissing(path) => (
                format!("configured workspace is missing: {}", path.display()),
                "Restore that directory or remove its stale entry from the per-user rai/roots.txt file.".into(),
                None,
            ),
            setup::Diagnostic::WatcherMissing => (
                "watcher LaunchAgent is not installed".into(),
                if can_setup { "Rerun rai setup with the existing workspace roots to reinstall the watcher.".into() } else { "Install rai with cargo install --path ., then rerun rai setup with the existing workspace roots.".into() },
                can_setup.then_some(DoctorFix::SetupExisting),
            ),
        };
        issues.push(DoctorIssue {
            message,
            solution,
            fix,
        });
    }
    (issues, changes)
}

fn solution_for_plan_error(error: &str) -> String {
    if error.starts_with("tracked output conflict:") {
        "Review and move the tracked instructions into .agents/rules/, then remove the native file from Git tracking before running rai sync. Adding .gitignore alone will not untrack it.".into()
    } else if error.starts_with("unowned or modified output conflict:") {
        "Back up and review the native file, transfer its intended rules into .agents/rules/, then move the conflicting file aside before running rai sync.".into()
    } else if error.starts_with("missing rules directory:") || error.starts_with("no .md rules in")
    {
        "Create at least one global Markdown rule in .agents/rules/, then run rai sync.".into()
    } else if error.starts_with("scoped rules are not supported") {
        "Keep scoped rules out of this POC or wait for scope-aware adapters; flattening them would change when they apply.".into()
    } else if error.starts_with("malformed RosettAI block") {
        "Repair the RosettAI start/end markers in the root .gitignore, then rerun rai sync.".into()
    } else if error.contains("symlink") {
        "Review the symlink target and replace the symlink with a regular source or output path before syncing.".into()
    } else {
        "Review the reported path and .agents/rules/ source, fix the validation error, then rerun rai doctor.".into()
    }
}

fn doctor(root: &Path, json: bool) -> Result<(), String> {
    let mut fix_all = false;
    let mut passes = 0;
    loop {
        let (issues, changes) = inspect_doctor(root);
        if json {
            print!("{{\"ok\":{},\"issues\":[", issues.is_empty());
            for (index, issue) in issues.iter().enumerate() {
                if index > 0 {
                    print!(",");
                }
                print!(
                    "{{\"message\":{},\"solution\":{},\"autoFixable\":{}}}",
                    json_string(&issue.message),
                    json_string(&issue.solution),
                    issue.fix.is_some()
                );
            }
            print!("],\"changes\":");
            print_changes_array(root, &changes);
            println!("}}");
            return if issues.is_empty() {
                Ok(())
            } else {
                Err(format!("{} issue(s) found", issues.len()))
            };
        }
        if issues.is_empty() {
            println!("No issues found");
            return Ok(());
        }
        for (index, issue) in issues.iter().enumerate() {
            println!("{}. {}", index + 1, issue.message);
            println!("   Solution: {}", issue.solution);
            if issue.fix.is_some() {
                println!("   Automatic fix available");
            }
        }
        let available: Vec<DoctorFix> = issues
            .iter()
            .filter_map(|issue| issue.fix.clone())
            .collect();
        if available.is_empty() {
            return Err(format!("{} issue(s) found", issues.len()));
        }
        if fix_all {
            passes += 1;
            if passes > 8 {
                return Err("automatic fixes did not converge".into());
            }
            for fix in available {
                apply_doctor_fix(root, fix)?;
            }
            println!("Rechecking...");
            continue;
        }
        if !io::stdin().is_terminal() {
            return Err(format!("{} issue(s) found", issues.len()));
        }
        print!("Fix which issue? Enter a number, 'all', or 'no': ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;
        match choose_fixes(&input, &issues) {
            FixChoice::Quit => return Err(format!("{} issue(s) found", issues.len())),
            FixChoice::Invalid => {
                println!("Choose an issue number, 'all', or 'no'.");
                continue;
            }
            FixChoice::Manual(index) => {
                println!("This issue needs manual review: {}", issues[index].solution);
                continue;
            }
            FixChoice::Apply { fixes, all } => {
                fix_all = all;
                for fix in fixes {
                    if let Err(error) = apply_doctor_fix(root, fix) {
                        eprintln!("rai doctor: automatic fix failed: {error}");
                        return Err(
                            "automatic fix failed; remaining issues were not changed".into()
                        );
                    }
                }
                println!("Rechecking...");
            }
        }
    }
}

enum FixChoice {
    Apply { fixes: Vec<DoctorFix>, all: bool },
    Manual(usize),
    Quit,
    Invalid,
}

fn choose_fixes(input: &str, issues: &[DoctorIssue]) -> FixChoice {
    match input.trim().to_ascii_lowercase().as_str() {
        "all" | "a" => FixChoice::Apply {
            fixes: issues
                .iter()
                .filter_map(|issue| issue.fix.clone())
                .collect(),
            all: true,
        },
        "no" | "n" | "q" | "quit" | "" => FixChoice::Quit,
        value => match value
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
        {
            Some(index) if index < issues.len() => match &issues[index].fix {
                Some(fix) => FixChoice::Apply {
                    fixes: vec![fix.clone()],
                    all: false,
                },
                None => FixChoice::Manual(index),
            },
            _ => FixChoice::Invalid,
        },
    }
}

fn apply_doctor_fix(root: &Path, fix: DoctorFix) -> Result<(), String> {
    match fix {
        DoctorFix::Init => init(root),
        DoctorFix::Sync => sync(root, false),
        DoctorFix::SetupNew(workspace) => setup::setup(vec![workspace]),
        DoctorFix::SetupExisting => setup::repair_existing(),
    }
}

fn print_changes_json(root: &Path, changes: &[Change], dry_run: bool) {
    print!("{{\"ok\":true,\"dryRun\":{dry_run},\"changes\":");
    print_changes_array(root, changes);
    println!("}}");
}

fn print_changes_array(root: &Path, changes: &[Change]) {
    print!("[");
    for (index, change) in changes.iter().enumerate() {
        if index > 0 {
            print!(",");
        }
        let path = change.path.strip_prefix(root).unwrap().to_string_lossy();
        let action = match change.action {
            Action::Create => "create",
            Action::Update => "update",
            Action::Unchanged => "unchanged",
        };
        print!(
            "{{\"path\":{},\"action\":{}}}",
            json_string(&path),
            json_string(action)
        );
    }
    print!("]");
}

fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn read_rules(dir: &Path) -> Result<String, String> {
    if dir.is_symlink() || dir.parent().is_some_and(Path::is_symlink) {
        return Err(format!("symlink source unsupported: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("missing rules directory: {}", dir.display()));
    }
    let mut files = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    files.sort();
    let mut sections = Vec::new();
    for path in files {
        if path.is_symlink() {
            return Err(format!("symlink rule unsupported: {}", path.display()));
        }
        if path.is_dir() {
            return Err(format!(
                "scoped rules are not supported in this POC: {}",
                path.display()
            ));
        }
        if path.extension().is_none_or(|ext| ext != "md") {
            return Err(format!("only .md rules are supported: {}", path.display()));
        }
        let name = path.file_name().unwrap().to_string_lossy();
        let content = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        sections.push(format!("## {name}\n\n{}", content.trim()));
    }
    if sections.is_empty() {
        return Err(format!("no .md rules in {}", dir.display()));
    }
    Ok(format!("{}\n", sections.join("\n\n")))
}

fn owned(body: &str, prefix: &str) -> String {
    let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
    format!("{prefix}{MARKER_START}{hash} -->\n{body}")
}

fn is_owned(content: &str, prefix: &str) -> bool {
    let Some(rest) = content.strip_prefix(prefix) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(MARKER_START) else {
        return false;
    };
    let Some((hash, body)) = rest.split_once(" -->\n") else {
        return false;
    };
    hash.len() == 64 && hash == format!("{:x}", Sha256::digest(body.as_bytes()))
}

fn is_tracked(root: &Path, path: &str) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", path])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot check Git tracking: {e}"))?;
    // Git exit code 1 means untracked. Code 128 means this is not a Git checkout.
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) | Some(128) => Ok(false),
        _ => Err(format!("Git tracking check failed for {path}")),
    }
}

fn update_ignore(old: &str) -> Result<String, String> {
    let block = format!("{IGNORE_START}\n/{CLAUDE}\n/{CURSOR}\n{IGNORE_END}\n");
    match (old.find(IGNORE_START), old.find(IGNORE_END)) {
        (None, None) => {
            let mut updated = old.to_owned();
            if !updated.is_empty() && !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push_str(&block);
            Ok(updated)
        }
        (Some(start), Some(end)) if start < end => {
            let end = end + IGNORE_END.len();
            let end = if old.as_bytes().get(end) == Some(&b'\n') {
                end + 1
            } else {
                end
            };
            Ok(format!("{}{}{}", &old[..start], block, &old[end..]))
        }
        _ => Err("malformed RosettAI block in .gitignore".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".agents/rules")).unwrap();
        fs::write(
            dir.path().join(".agents/rules/general.md"),
            "Prefer clear names.\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn creates_both_projections_and_is_idempotent() {
        let dir = repo();
        sync(dir.path(), false).unwrap();
        let claude = fs::read_to_string(dir.path().join(CLAUDE)).unwrap();
        let cursor = fs::read_to_string(dir.path().join(CURSOR)).unwrap();
        assert!(claude.contains("Prefer clear names."));
        assert!(cursor.starts_with(CURSOR_FRONTMATTER));
        assert!(cursor.contains("Prefer clear names."));
        assert!(is_owned(&claude, ""));
        assert!(is_owned(&cursor, CURSOR_FRONTMATTER));
        let ignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        sync(dir.path(), false).unwrap();
        assert_eq!(claude, fs::read_to_string(dir.path().join(CLAUDE)).unwrap());
        assert_eq!(cursor, fs::read_to_string(dir.path().join(CURSOR)).unwrap());
        assert_eq!(
            ignore,
            fs::read_to_string(dir.path().join(".gitignore")).unwrap()
        );
        assert_eq!(ignore.matches(IGNORE_START).count(), 1);
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = repo();
        sync(dir.path(), true).unwrap();
        assert!(!dir.path().join(CLAUDE).exists());
        assert!(!dir.path().join(CURSOR).exists());
        assert!(!dir.path().join(".gitignore").exists());
    }

    #[test]
    fn unowned_collision_aborts_all_writes() {
        let dir = repo();
        fs::write(dir.path().join(CLAUDE), "Personal instructions\n").unwrap();
        let error = sync(dir.path(), false).unwrap_err();
        assert!(error.contains("unowned"));
        assert!(!dir.path().join(CURSOR).exists());
        assert!(!dir.path().join(".gitignore").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(CLAUDE)).unwrap(),
            "Personal instructions\n"
        );
    }

    #[test]
    fn edited_projection_is_not_overwritten() {
        let dir = repo();
        sync(dir.path(), false).unwrap();
        let path = dir.path().join(CLAUDE);
        fs::write(
            &path,
            format!("{}manual edit\n", fs::read_to_string(&path).unwrap()),
        )
        .unwrap();
        let error = sync(dir.path(), false).unwrap_err();
        assert!(error.contains("modified"));
        assert!(fs::read_to_string(path).unwrap().contains("manual edit"));
    }

    #[test]
    fn rule_changes_update_both_outputs() {
        let dir = repo();
        sync(dir.path(), false).unwrap();
        fs::write(
            dir.path().join(".agents/rules/general.md"),
            "Write tests.\n",
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        assert!(
            fs::read_to_string(dir.path().join(CLAUDE))
                .unwrap()
                .contains("Write tests.")
        );
        assert!(
            fs::read_to_string(dir.path().join(CURSOR))
                .unwrap()
                .contains("Write tests.")
        );
    }

    #[test]
    fn scoped_rules_are_rejected() {
        let dir = repo();
        fs::create_dir(dir.path().join(".agents/rules/frontend")).unwrap();
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("scoped rules")
        );
        assert!(!dir.path().join(CLAUDE).exists());
    }

    #[test]
    fn tracked_native_file_is_rejected() {
        let dir = repo();
        let status = Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        fs::write(dir.path().join(CLAUDE), "Tracked instructions\n").unwrap();
        let status = Command::new("git")
            .args(["add", CLAUDE])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("tracked output")
        );
    }

    #[test]
    fn repo_discovery_does_not_escape_git_root() {
        let dir = tempfile::tempdir().unwrap();
        let status = Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let error = find_repo(dir.path()).unwrap_err();
        assert!(error.contains("no .agents/ directory found inside"));
    }

    #[test]
    fn doctor_selection_supports_one_all_and_manual() {
        let issues = vec![
            DoctorIssue {
                message: "drift".into(),
                solution: "sync".into(),
                fix: Some(DoctorFix::Sync),
            },
            DoctorIssue {
                message: "manual".into(),
                solution: "review".into(),
                fix: None,
            },
            DoctorIssue {
                message: "setup".into(),
                solution: "configure".into(),
                fix: Some(DoctorFix::SetupExisting),
            },
        ];
        assert!(
            matches!(choose_fixes("1", &issues), FixChoice::Apply { fixes, all: false } if fixes.len() == 1)
        );
        assert!(matches!(choose_fixes("2", &issues), FixChoice::Manual(1)));
        assert!(
            matches!(choose_fixes("all", &issues), FixChoice::Apply { fixes, all: true } if fixes.len() == 2)
        );
        assert!(matches!(choose_fixes("no", &issues), FixChoice::Quit));
        assert!(matches!(choose_fixes("4", &issues), FixChoice::Invalid));
    }

    #[test]
    fn doctor_proposes_init_when_agents_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let (issues, _) = inspect_doctor(dir.path());
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("no .agents/")
                    && matches!(issue.fix, Some(DoctorFix::Init)))
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_parent_is_rejected() {
        use std::os::unix::fs::symlink;
        let dir = repo();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join(".cursor")).unwrap();
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("symlink output")
        );
    }
}
