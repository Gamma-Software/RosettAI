use chrono::Local;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod claude;
mod codex;
mod copilot;
mod hook_registry;
mod perf;
mod root_tracking;
mod setup;
mod uninstall;
mod update;
mod user_data;

const CODEX: &str = "AGENTS.md";
const LEGACY_CURSOR: &str = ".cursor/rules/rosettai.mdc";
const IGNORE_START: &str = "# RosettAI generated files";
const IGNORE_END: &str = "# End RosettAI generated files";
const LEGACY_CURSOR_FRONTMATTER: &str =
    "---\ndescription: Shared RosettAI rules\nalwaysApply: true\n---\n";
const MARKER_START: &str = "<!-- rai-generated sha256:";

#[derive(Debug, PartialEq)]
enum Action {
    Create,
    Update,
    Delete,
    Unchanged,
}

struct Change {
    path: PathBuf,
    content: String,
    action: Action,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let is_update = args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "update" | "uninstall"));
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
    if !is_update {
        update::automatic_warning();
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return helper();
    }
    let mut args = args.into_iter();
    let command = args.next().ok_or_else(usage)?;
    let mut dry_run = false;
    let mut json = false;
    let mut perf = false;
    let mut codex_hook = false;
    let mut git_hook = false;
    let mut repo = None;
    let mut roots = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--perf" => perf = true,
            "--codex-hook" => codex_hook = true,
            "--git-hook" => git_hook = true,
            "--repo" => repo = Some(PathBuf::from(args.next().ok_or("--repo needs a path")?)),
            "--root" => roots.push(PathBuf::from(args.next().ok_or("--root needs a path")?)),
            _ => return Err(format!("unknown option: {arg}")),
        }
    }
    if command == "help" {
        if dry_run || json || codex_hook || git_hook || repo.is_some() || !roots.is_empty() {
            return Err("help accepts only --perf".into());
        }
        print_help();
        return Ok(());
    }
    if dry_run && command != "sync" {
        return Err("--dry-run is only valid with sync".into());
    }
    if json && !matches!(command.as_str(), "sync" | "status" | "doctor") {
        return Err("--json is only valid with sync, status, or doctor".into());
    }
    if !roots.is_empty() && command != "install" {
        return Err("--root is only valid with install".into());
    }
    if codex_hook && command != "sync" {
        return Err("--codex-hook is only valid with sync".into());
    }
    if git_hook && command != "sync" {
        return Err("--git-hook is only valid with sync".into());
    }
    if git_hook && (codex_hook || dry_run || json) {
        return Err("--git-hook cannot be combined with --codex-hook, --dry-run, or --json".into());
    }
    if codex_hook && (dry_run || json) {
        return Err("--codex-hook cannot be combined with --dry-run or --json".into());
    }
    if repo.is_some() && matches!(command.as_str(), "install" | "uninstall") {
        return Err("--repo is not valid with install or uninstall".into());
    }
    if command == "install" {
        return setup::setup(roots);
    }
    if command == "uninstall" {
        return uninstall::uninstall();
    }
    if command == "update" {
        if repo.is_some() || codex_hook || json {
            return Err("update accepts only --perf".into());
        }
        return update::install();
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
    if command == "migrate" {
        return migrate(&start);
    }
    if command == "rollback" {
        return rollback_migration(&start);
    }
    if command == "sync" && !codex_hook && !git_hook && !dry_run && !json {
        let start = start.canonicalize().map_err(|e| e.to_string())?;
        let root = git_root(&start).unwrap_or(start);
        let rules = root.join(".agents/rules");
        if rules.exists() || rules.is_symlink() {
            read_rules(&root)?;
        }
        let mut native = native_sources(&root)?;
        if !native.is_empty() {
            while !native.is_empty() {
                let previous = existing_migration_backup(&root)?;
                let pending = if let Some(backup) = &previous {
                    let manifest = read_migration_manifest(backup, &root)?;
                    manifest["sources"].as_array().is_some_and(|sources| {
                        sources.iter().any(|source| {
                            source["path"]
                                .as_str()
                                .is_some_and(|path| native.iter().any(|p| p == path))
                        })
                    })
                } else {
                    false
                };
                if pending {
                    if !finish_migration(&root, true)? {
                        return Ok(());
                    }
                } else {
                    migrate(&root)?;
                    if existing_migration_backup(&root)? == previous {
                        return Ok(());
                    }
                    finish_migration(&root, false)?;
                }
                native = native_sources(&root)?;
            }
            print!("Synchronize now? [y/N] ");
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut answer = String::new();
            io::stdin()
                .read_line(&mut answer)
                .map_err(|e| e.to_string())?;
            if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                println!("Synchronization deferred");
                return Ok(());
            }
        } else if !root.join(".agents").exists() {
            init(&root)?;
        }
    }
    if command == "sync" && git_hook {
        match find_repo(&start) {
            Ok(_) => {}
            Err(error) if error.starts_with("no .agents/ directory found") => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    let root = if codex_hook {
        match find_repo(&start) {
            Ok(root) => root,
            Err(error) => {
                print_codex_hook_result(&format!("RosettAI cannot check this prompt: {error}"));
                return Ok(());
            }
        }
    } else if command == "doctor" {
        find_repo(&start).or_else(|_| {
            let start = start.canonicalize().map_err(|e| e.to_string())?;
            Ok::<PathBuf, String>(git_root(&start).unwrap_or(start))
        })?
    } else {
        match find_repo(&start) {
            Ok(root) => root,
            Err(error) => {
                if command == "sync" && json {
                    println!("{{\"ok\":false,\"error\":{}}}", json_string(&error));
                }
                return Err(error);
            }
        }
    };
    if command == "sync" && (git_hook || dry_run || json) {
        let native = native_sources(&root)?;
        if !native.is_empty() {
            let error = format!(
                "unmanaged native configuration: {}; run rai sync manually to review migration",
                native.join(", ")
            );
            if json {
                println!("{{\"ok\":false,\"error\":{}}}", json_string(&error));
            }
            return Err(error);
        }
    }
    match command.as_str() {
        "sync" if codex_hook => sync_codex_hook(&root),
        "sync" => sync_with_format(&root, dry_run, json),
        "status" => status(&root, json),
        "doctor" => doctor(&root, json),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: rai [help|init|sync|doctor|status|install|uninstall|migrate|rollback|update] [options]"
        .into()
}

fn print_help() {
    println!(
        "RosettAI CLI\n\nRun `rai` for the interactive helper.\n\nCommon commands:\n  rai sync               Set up the current project if needed, then synchronize\n  rai doctor             Diagnose and repair configuration problems\n  rai help               Show this help\n\nOther commands:\n  rai init               Create an empty canonical source tree or offer migration\n  rai status             Show projection status\n  rai install            Set up global Git hooks; optionally watch a directory (--root PATH)\n  rai uninstall          Remove machine integration\n  rai migrate            Import supported native instructions\n  rai rollback           Restore instructions from a migration backup\n  rai update             Update the CLI\n\nOptions: --repo PATH, --root PATH, --dry-run, --json, --perf\nRun `rai <command> --repo PATH` to target another project where supported."
    );
}

fn helper() -> Result<(), String> {
    if !io::stdin().is_terminal() {
        print_help();
        return Ok(());
    }
    let current = env::current_dir().map_err(|e| e.to_string())?;
    loop {
        let root = git_root(&current).unwrap_or_else(|| current.clone());
        println!("\nRosettAI — {}", current.display());
        if root.join(".agents").is_dir() {
            println!("  Project: configured");
        } else if !native_sources(&root)?.is_empty() {
            println!("  Project: native configuration found; migration available");
        } else {
            println!("  Project: not configured");
        }
        println!("  1. Sync this project (set up if needed)");
        println!("  2. Diagnose a problem");
        println!("  3. Manage this machine");
        println!("  4. Help");
        print!("Choose an option (q to quit): ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut choice = String::new();
        if io::stdin()
            .read_line(&mut choice)
            .map_err(|e| e.to_string())?
            == 0
        {
            return Ok(());
        }
        match choice.trim() {
            "1" => run(vec![
                "sync".into(),
                "--repo".into(),
                current.to_string_lossy().into_owned(),
            ])?,
            "2" => {
                let root = find_repo(&current).unwrap_or(current.clone());
                doctor(&root, false)?;
            }
            "3" => machine_helper(&current)?,
            "4" => print_help(),
            "q" | "Q" => return Ok(()),
            _ => println!("Choose 1–4 or q."),
        }
    }
}

fn machine_helper(current: &Path) -> Result<(), String> {
    println!(
        "\nMachine integration:\n  1. Watch this directory\n  2. Choose a workspace directory\n  3. Update the CLI\n  4. Uninstall machine integration\n  5. Roll back project migration"
    );
    print!("Choose an option (Enter to return): ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut choice = String::new();
    io::stdin()
        .read_line(&mut choice)
        .map_err(|e| e.to_string())?;
    match choice.trim() {
        "1" => setup::setup(vec![current.to_path_buf()]),
        "2" => {
            print!("Workspace directory to watch: ");
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut path = String::new();
            io::stdin()
                .read_line(&mut path)
                .map_err(|e| e.to_string())?;
            if path.trim().is_empty() {
                Ok(())
            } else {
                setup::setup(vec![PathBuf::from(path.trim())])
            }
        }
        "3" => update::install(),
        "4" => uninstall::uninstall(),
        "5" => rollback_migration(current),
        "" => Ok(()),
        _ => {
            println!("Choose 1–5 or press Enter.");
            Ok(())
        }
    }
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
        if change.action == Action::Delete {
            fs::remove_file(&change.path).map_err(|e| format!("{}: {e}", change.path.display()))?;
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

fn sync_codex_hook(root: &Path) -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    let native = native_sources(root)?;
    if !native.is_empty() {
        print_codex_hook_result(&format!(
            "RosettAI found unmanaged native configuration: {}. Run rai sync manually to review migration.",
            native.join(", ")
        ));
        return Ok(());
    }
    let changes = match plan_sync(root) {
        Ok(changes) => changes,
        Err(error) => {
            print_codex_hook_result(&format!("RosettAI synchronization is blocked: {error}"));
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
    let paths = changes
        .iter()
        .filter(|change| change.action != Action::Unchanged)
        .map(|change| {
            change
                .path
                .strip_prefix(root)
                .unwrap()
                .display()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ");
    match apply_changes(changes) {
        Ok(()) => print_codex_hook_result(&format!(
            "RosettAI synchronized {paths}. Resubmit the prompt in a new Codex session so instructions and MCP config reload."
        )),
        Err(error) => print_codex_hook_result(&format!("RosettAI synchronization failed: {error}")),
    }
    Ok(())
}

fn print_codex_hook_result(message: &str) {
    println!(
        "{{\"decision\":\"block\",\"reason\":{}}}",
        json_string(message)
    );
}

fn projection_outputs(
    root: &Path,
    rules: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<(String, String, String)>, String> {
    let mut outputs = rules
        .iter()
        .map(|(scope, sections)| {
            let relative = if scope.is_empty() {
                CODEX.to_string()
            } else {
                format!("{scope}/{CODEX}")
            };
            let body = format!("# Shared project rules\n\n{}\n", sections.join("\n\n"));
            (relative, owned(&body, ""), "markdown".to_string())
        })
        .collect::<Vec<_>>();
    for (path, body) in codex::outputs(root)? {
        outputs.push((path, owned_comment(&body), "hash".to_string()));
    }
    for (path, body, kind) in copilot::outputs(root, rules)? {
        let content = if kind == "jsonc" {
            owned_slash_comment(&body)
        } else {
            owned(&body, "")
        };
        outputs.push((path, content, kind));
    }
    for (path, body, kind) in claude::outputs(root, rules)? {
        let content = if kind == "json" {
            owned_json(&body)?
        } else {
            owned(&body, "")
        };
        outputs.push((path, content, kind));
    }

    Ok(outputs)
}

fn plan_sync(root: &Path) -> Result<Vec<Change>, String> {
    let rules = read_rules(root)?;
    codex::check_version()?;
    let outputs = projection_outputs(root, &rules)?;

    // Plan every write before applying any of them. A single collision aborts the sync.
    let mut changes = Vec::new();
    for (relative, content, marker) in outputs {
        let path = root.join(&relative);
        if is_tracked(root, &relative)? {
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
            let valid = match marker.as_str() {
                "hash" => is_owned_comment(&current),
                "jsonc" => is_owned_slash_comment(&current),
                "json" => is_owned_json(&current),
                _ => is_owned(&current, ""),
            };
            if !valid {
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

    {
        let agents_dir = root.join(".codex/agents");
        if agents_dir.is_symlink() {
            return Err("symlink output conflict: .codex/agents".into());
        }
        if agents_dir.is_dir() {
            for entry in fs::read_dir(&agents_dir).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_symlink() {
                    return Err(format!("symlink output conflict: {}", path.display()));
                }
                if path.extension().is_some_and(|ext| ext == "toml")
                    && !changes.iter().any(|c| c.path == path)
                {
                    let relative = path
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    if is_tracked(root, &relative)? {
                        continue;
                    }
                    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                    if is_owned_comment(&content) {
                        changes.push(Change {
                            path,
                            content: String::new(),
                            action: Action::Delete,
                        });
                    }
                }
            }
        }
    }

    for directory in [
        ".github/agents",
        ".github/instructions",
        ".claude/agents",
        ".claude/rules",
        ".claude/skills",
    ] {
        let dir = root.join(directory);
        if dir.is_symlink() {
            return Err(format!("symlink output conflict: {directory}"));
        }
        if dir.is_dir() {
            for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_symlink() {
                    return Err(format!("symlink output conflict: {}", path.display()));
                }
                if path.is_file() && !changes.iter().any(|change| change.path == path) {
                    let relative = path
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    if !is_tracked(root, &relative)? {
                        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                        if is_owned(&content, "") {
                            changes.push(Change {
                                path: path.clone(),
                                content: String::new(),
                                action: Action::Delete,
                            });
                        }
                    }
                }
                if directory == ".claude/skills" && path.is_dir() {
                    let skill = path.join("SKILL.md");
                    if skill.is_file() && !changes.iter().any(|change| change.path == skill) {
                        let relative = skill
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();
                        if !is_tracked(root, &relative)? {
                            let content = fs::read_to_string(&skill).map_err(|e| e.to_string())?;
                            if is_owned(&content, "") {
                                changes.push(Change {
                                    path: skill,
                                    content: String::new(),
                                    action: Action::Delete,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    for (relative, prefix) in [(LEGACY_CURSOR, LEGACY_CURSOR_FRONTMATTER)] {
        let path = root.join(relative);
        if path.is_symlink()
            || path
                .ancestors()
                .take_while(|part| *part != root)
                .any(Path::is_symlink)
        {
            continue;
        }
        if path.is_file() && !is_tracked(root, relative)? {
            let current = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            if is_owned(&current, prefix) {
                changes.push(Change {
                    path,
                    content: String::new(),
                    action: Action::Delete,
                });
            }
        }
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
    for relative in managed_output_paths(&old_ignore)? {
        let path = root.join(&relative);
        if changes.iter().any(|change| change.path == path) || is_tracked(root, &relative)? {
            continue;
        }
        if path
            .ancestors()
            .take_while(|part| *part != root)
            .any(Path::is_symlink)
            || !path.is_file()
        {
            continue;
        }
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if is_owned(&content, "")
            || is_owned_comment(&content)
            || is_owned_slash_comment(&content)
            || is_owned_json(&content)
        {
            changes.push(Change {
                path,
                content: String::new(),
                action: Action::Delete,
            });
        }
    }
    if git_root(root).is_none() {
        return Ok(changes);
    }

    let paths = changes
        .iter()
        .filter(|change| change.action != Action::Delete)
        .filter_map(|change| change.path.strip_prefix(root).ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let new_ignore = update_ignore_paths(&old_ignore, &paths)?;
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
    let native = native_sources(&root)?;
    if !native.is_empty() {
        return migrate(&root);
    }
    let agents = root.join(".agents");
    if agents.exists() || agents.is_symlink() {
        return Err(format!("already exists: {}", agents.display()));
    }
    setup::ensure_first_setup(&root)?;
    fs::create_dir(&agents).map_err(|e| format!("{}: {e}", agents.display()))?;
    create_canonical_layout(&agents)?;
    println!("Created {}", agents.display());
    println!("Add resources under .agents/ when needed, then run rai sync");
    Ok(())
}

fn create_canonical_layout(agents: &Path) -> Result<(), String> {
    for directory in ["rules", "agents", "commands", "skills"] {
        let path = agents.join(directory);
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        let keep = path.join(".keep");
        if !keep.exists() {
            fs::write(keep, "").map_err(|e| e.to_string())?;
        }
    }
    let mcp = agents.join("mcp.yaml");
    if !mcp.exists() && !agents.join("mcp.json").exists() {
        fs::write(mcp, "servers: {}\n").map_err(|e| e.to_string())?;
    }
    Ok(())
}

const IMPORTABLE_NATIVE: [&str; 3] = ["AGENTS.md", "CLAUDE.md", ".github/copilot-instructions.md"];
const INSTRUCTION_NAMES: [&str; 3] = ["AGENTS.md", "CLAUDE.md", "copilot-instructions.md"];

fn instruction_scope(relative: &str) -> Result<String, String> {
    if IMPORTABLE_NATIVE.contains(&relative) {
        return Ok(String::new());
    }
    let path = Path::new(relative);
    if relative.contains(['\\', '\n', '\r'])
        || !path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        || !path
            .file_name()
            .is_some_and(|name| name == "AGENTS.md" || name == "CLAUDE.md")
    {
        return Err(format!("unsupported migration source: {relative}"));
    }
    let scope = path
        .parent()
        .and_then(Path::to_str)
        .ok_or("invalid migration source path")?;
    if scope.is_empty()
        || Path::new(scope).components().any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some(".git" | ".agents" | ".codex" | ".claude" | ".github" | ".cursor")
            )
        })
    {
        return Err(format!("unsupported migration source: {relative}"));
    }
    Ok(scope.to_owned())
}

fn nested_instruction_sources(
    root: &Path,
    directory: &Path,
    found: &mut Vec<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            // Harness internals, dependencies, build outputs, and fixture data are not project scopes.
            if matches!(
                entry.file_name().to_str(),
                Some(
                    ".git"
                        | ".agents"
                        | ".codex"
                        | ".claude"
                        | ".github"
                        | ".cursor"
                        | "node_modules"
                        | "target"
                        | "fixtures"
                )
            ) || path.join(".git").exists()
                || path.join(".agents").exists()
            {
                continue;
            }
            nested_instruction_sources(root, &path, found)?;
        } else if directory != root
            && (kind.is_file() || kind.is_symlink())
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| matches!(name, "AGENTS.md" | "CLAUDE.md"))
            && !owned_native_file(&path)?
        {
            found.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .ok_or("non-UTF-8 migration path")?
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

pub(crate) fn native_sources(root: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for relative in IMPORTABLE_NATIVE {
        let path = root.join(relative);
        if (path.exists() || path.is_symlink()) && !owned_native_file(&path)? {
            found.push(relative.to_string());
        }
    }
    for directory in [
        ".claude/rules",
        ".claude/agents",
        ".claude/skills",
        ".claude/commands",
        ".github/instructions",
        ".github/agents",
        ".github/prompts",
        ".codex/agents",
        ".codex/rules",
        ".codex/skills",
        ".cursor/rules",
        ".cursor/agents",
        ".cursor/skills",
        ".cursor/commands",
    ] {
        let dir = root.join(directory);
        if dir.is_symlink() {
            found.push(directory.to_string());
        } else if dir.is_dir() {
            for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if (path.is_symlink() || path.is_file() || path.is_dir())
                    && !owned_native_file(&path)?
                {
                    found.push(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
    }
    for relative in [
        ".codex/config.toml",
        ".claude/settings.json",
        ".github/copilot-mcp.json",
        ".mcp.json",
        ".vscode/mcp.json",
        ".cursor/mcp.json",
    ] {
        let path = root.join(relative);
        if (path.exists() || path.is_symlink()) && !owned_native_file(&path)? {
            found.push(relative.to_string());
        }
    }
    nested_instruction_sources(root, root, &mut found)?;
    found.sort();
    Ok(found)
}

fn owned_native_file(path: &Path) -> Result<bool, String> {
    if path.is_symlink() {
        return Ok(false);
    }
    if path.is_dir() {
        let skill = path.join("SKILL.md");
        return Ok(skill.is_file()
            && !skill.is_symlink()
            && fs::read_dir(path).map_err(|e| e.to_string())?.count() == 1
            && owned_native_file(&skill)?);
    }
    if !path.is_file() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(match path.extension().and_then(|ext| ext.to_str()) {
        Some("toml") => is_owned_comment(&content),
        Some("json") => is_owned_json(&content) || is_owned_slash_comment(&content),
        _ => is_owned(&content, ""),
    })
}

fn migration_base() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let base = user_data::config_dir()?;
    #[cfg(not(windows))]
    let base = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?).join(".rai");
    let migrations = base.join("migrations");
    for path in [&base, &migrations] {
        if path.is_symlink() || (path.exists() && !path.is_dir()) {
            return Err(format!(
                "migration backup directory conflict: {}",
                path.display()
            ));
        }
    }
    Ok(migrations)
}

fn migration_project_dir(root: &Path) -> Result<PathBuf, String> {
    let name = root
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or("cannot name migration backup for filesystem root")?;
    let project = migration_base()?.join(name);
    if project.is_symlink() || (project.exists() && !project.is_dir()) {
        return Err(format!(
            "migration backup directory conflict: {}",
            project.display()
        ));
    }
    Ok(project)
}

fn migration_backup(root: &Path) -> Result<PathBuf, String> {
    let project = migration_project_dir(root)?;
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    for suffix in 1..=1000 {
        let name = if suffix == 1 {
            timestamp.clone()
        } else {
            format!("{timestamp}-{suffix}")
        };
        let candidate = project.join(name);
        if !candidate.exists() && !candidate.is_symlink() {
            return Ok(candidate);
        }
    }
    Err("no available migration backup directory for this second".into())
}

fn find_manifest_backup(directory: &Path, root: &Path) -> Result<Option<PathBuf>, String> {
    if directory.is_dir() {
        let mut entries = fs::read_dir(directory)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries.into_iter().rev() {
            if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let backup = entry.path();
            let manifest_path = backup.join("manifest.json");
            if !manifest_path.is_file() || manifest_path.is_symlink() {
                continue;
            }
            let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(
                &fs::read(&manifest_path).map_err(|e| e.to_string())?,
            ) else {
                continue;
            };
            if manifest["repository"] == root.to_string_lossy().as_ref() {
                return Ok(Some(backup));
            }
        }
    }
    Ok(None)
}

fn existing_migration_backup(root: &Path) -> Result<Option<PathBuf>, String> {
    let base = migration_base()?;
    let project = migration_project_dir(root)?;
    if let Some(backup) = find_manifest_backup(&project, root)? {
        return Ok(Some(backup));
    }
    if let Some(backup) = find_manifest_backup(&base, root)? {
        return Ok(Some(backup));
    }
    let previous = base.join(sha256_hex(root.to_string_lossy().as_bytes()));
    if previous.join("manifest.json").is_file() {
        return Ok(Some(previous));
    }
    let legacy = root.join(".agents/migration-backup");
    if legacy.join("manifest.json").is_file() {
        return Ok(Some(legacy));
    }
    Ok(None)
}

fn read_migration_manifest(backup: &Path, root: &Path) -> Result<serde_json::Value, String> {
    let path = backup.join("manifest.json");
    if has_symlink_component(backup, &path) || backup.is_symlink() {
        return Err("symlink migration backup conflict".into());
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("migration manifest: {e}"))?;
    if manifest["repository"]
        .as_str()
        .is_some_and(|repository| repository != root.to_string_lossy())
    {
        return Err("migration backup belongs to another repository".into());
    }
    Ok(manifest)
}

struct MigratedRule {
    path: String,
    hash: String,
    scope: String,
    name: Option<String>,
}

fn manifest_rules(manifest: &serde_json::Value) -> Result<Vec<MigratedRule>, String> {
    if manifest["version"] == 1 && manifest["generated"] == ".agents/rules/migrated-harness.md" {
        return Ok(vec![MigratedRule {
            path: ".agents/rules/migrated-harness.md".into(),
            hash: manifest["generatedSha256"]
                .as_str()
                .ok_or("invalid migration manifest hash")?
                .into(),
            scope: String::new(),
            name: manifest["generatedName"].as_str().map(str::to_owned),
        }]);
    }
    if manifest["version"] != 2 {
        return Err("unsupported migration manifest".into());
    }
    let mut paths = HashSet::new();
    let mut scopes = HashSet::new();
    let rules = manifest["generatedRules"]
        .as_array()
        .ok_or("invalid migration manifest rules")?;
    if rules.is_empty() {
        return Err("empty migration manifest rules".into());
    }
    rules
        .iter()
        .map(|rule| {
            let path = rule["path"].as_str().ok_or("invalid migrated rule path")?;
            let file = Path::new(path);
            let scope = rule["scope"]
                .as_str()
                .ok_or("invalid migrated rule scope")?;
            if file.parent() != Some(Path::new(".agents/rules"))
                || file.extension().is_none_or(|ext| ext != "md")
                || !paths.insert(path)
                || !scopes.insert(scope)
                || (!scope.is_empty() && instruction_scope(&format!("{scope}/AGENTS.md"))? != scope)
            {
                return Err("invalid migrated rule path or scope".into());
            }
            Ok(MigratedRule {
                path: path.into(),
                hash: rule["sha256"]
                    .as_str()
                    .ok_or("invalid migrated rule hash")?
                    .into(),
                scope: scope.into(),
                name: rule["name"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

fn render_migrated_rules(bodies: &[(String, String)]) -> Result<BTreeMap<String, String>, String> {
    let mut sections: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (relative, body) in bodies {
        sections
            .entry(instruction_scope(relative)?)
            .or_default()
            .push(format!("## From {relative}\n\n{}", body.trim()));
    }
    Ok(sections
        .into_iter()
        .map(|(scope, sections)| {
            let prefix = if scope.is_empty() {
                String::new()
            } else {
                format!("---\npath: {scope}\n---\n\n")
            };
            let content = format!(
                "{prefix}# Migrated harness instructions\n\n{}\n",
                sections.join("\n\n")
            );
            (scope, content)
        })
        .collect())
}

fn with_rule_name(content: &str, name: &str) -> Result<String, String> {
    let (scope, _, body) = parse_rule(content)?;
    let metadata = if scope.is_empty() {
        serde_json::json!({"name": name})
    } else {
        serde_json::json!({"name": name, "path": scope})
    };
    let yaml = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    Ok(format!(
        "---\n{yaml}---\n\n{}",
        body.trim_start_matches('\n')
    ))
}

fn finish_migration(root: &Path, confirm: bool) -> Result<bool, String> {
    let backup = existing_migration_backup(root)?.ok_or("migration backup missing")?;
    let manifest = read_migration_manifest(&backup, root)?;
    let rules = manifest_rules(&manifest)?;
    let sources = manifest["sources"]
        .as_array()
        .ok_or("invalid migration manifest sources")?;
    let mut bodies = Vec::new();
    let mut tracked = Vec::new();
    for source in sources {
        let relative = source["path"].as_str().ok_or("invalid migration source")?;
        instruction_scope(relative)?;
        let saved = backup.join(relative);
        let original = root.join(relative);
        if has_symlink_component(&backup, &saved) || has_symlink_component(root, &original) {
            return Err(format!("symlink migration source conflict: {relative}"));
        }
        let body = fs::read_to_string(&saved).map_err(|e| format!("{}: {e}", saved.display()))?;
        let hash = source["sha256"]
            .as_str()
            .ok_or("invalid migration source hash")?;
        if sha256_hex(body.as_bytes()) != hash {
            return Err(format!("migration backup changed: {relative}"));
        }
        bodies.push((relative.to_owned(), body));
        if is_tracked(root, relative)? {
            if !original.is_file()
                || sha256_hex(&fs::read(&original).map_err(|e| e.to_string())?) != hash
            {
                return Err(format!("tracked migration source changed: {relative}"));
            }
            let staged = Command::new("git")
                .args(["show", &format!(":{relative}")])
                .current_dir(root)
                .output()
                .map_err(|e| e.to_string())?;
            if !staged.status.success() || sha256_hex(&staged.stdout) != hash {
                return Err(format!("staged migration source changed: {relative}"));
            }
            tracked.push(relative.to_string());
        } else if original.exists() || original.is_symlink() {
            return Err(format!("untracked migration source reappeared: {relative}"));
        }
    }
    let mut generated = render_migrated_rules(&bodies)?;
    if generated.len() != rules.len() {
        return Err("migration manifest does not match saved instructions".into());
    }
    for rule in &rules {
        if let Some(name) = &rule.name {
            let body = generated
                .get_mut(&rule.scope)
                .ok_or("missing migrated rule scope")?;
            *body = with_rule_name(body, name)?;
        }
        let target = root.join(&rule.path);
        let body = generated
            .get(&rule.scope)
            .ok_or("missing migrated rule scope")?;
        parse_rule(body).map_err(|e| format!("{}: {e}", rule.path))?;
        if has_symlink_component(root, &target) {
            return Err("symlink migrated rule conflict".into());
        }
        if sha256_hex(body.as_bytes()) != rule.hash {
            return Err("migration manifest does not match saved instructions".into());
        }
        if target.exists() && fs::read(&target).map_err(|e| e.to_string())? != body.as_bytes() {
            return Err("migrated rule changed; review it before resuming migration".into());
        }
    }
    if confirm {
        println!("Resume migration for {}:", root.display());
        for rule in &rules {
            if !root.join(&rule.path).exists() {
                println!("  Restore {} from the verified backup", rule.path);
            }
        }
        for relative in &tracked {
            println!(
                "  Remove tracked {relative} from Git and replace it with a generated projection"
            );
        }
        print!("Proceed? [y/N] ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Migration deferred; no files changed");
            return Ok(false);
        }
    }
    for rule in &rules {
        let target = root.join(&rule.path);
        if !target.exists() {
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::write(&target, &generated[&rule.scope]).map_err(|e| e.to_string())?;
        }
    }
    for relative in &tracked {
        let removed = Command::new("git")
            .args(["rm", "--cached", "-q", "--", relative])
            .current_dir(root)
            .status()
            .map_err(|e| e.to_string())?;
        if !removed.success() {
            return Err(format!(
                "cannot remove tracked migration source from Git: {relative}"
            ));
        }
        fs::remove_file(root.join(relative)).map_err(|e| format!("{relative}: {e}"))?;
    }
    Ok(true)
}

fn migrate(start: &Path) -> Result<(), String> {
    let start = start
        .canonicalize()
        .map_err(|e| format!("{}: {e}", start.display()))?;
    let root = git_root(&start).unwrap_or(start);
    let found = native_sources(&root)?;
    if found.is_empty() {
        return Err("no unmanaged harness configuration found to migrate".into());
    }
    println!(
        "Discovered unmigrated harness configuration in {}:",
        root.display()
    );
    for relative in &found {
        println!("  {relative}");
    }
    println!();
    let unsupported = found
        .iter()
        .filter(|path| instruction_scope(path).is_err())
        .collect::<Vec<_>>();
    if !unsupported.is_empty() {
        return Err(format!(
            "migration requires manual conversion of: {}; copy these resources into .agents/ before running rai sync",
            unsupported
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut tracked = Vec::new();
    for relative in &found {
        if has_symlink_component(&root, &root.join(relative)) {
            return Err(format!("symlink migration source conflict: {relative}"));
        }
        if is_tracked(&root, relative)? {
            tracked.push(relative.as_str());
        }
    }
    let agents = root.join(".agents");
    if agents.is_symlink() {
        return Err("symlink source conflict: .agents".into());
    }
    let backup = migration_backup(&root)?;
    if backup.exists() || backup.is_symlink() {
        return Err("migration destination or backup already exists".into());
    }
    let mut bodies = Vec::new();
    let mut sources = Vec::new();
    for relative in &found {
        let body =
            fs::read_to_string(root.join(relative)).map_err(|e| format!("{relative}: {e}"))?;
        sources.push(serde_json::json!({"path": relative, "sha256": sha256_hex(body.as_bytes())}));
        bodies.push((relative.to_owned(), body));
    }
    for (relative, body) in &bodies {
        if tracked.contains(&relative.as_str()) {
            let staged = Command::new("git")
                .args(["show", &format!(":{relative}")])
                .current_dir(&root)
                .output()
                .map_err(|e| e.to_string())?;
            if !staged.status.success() || staged.stdout != body.as_bytes() {
                return Err(format!("staged migration source changed: {relative}"));
            }
        }
    }
    let mut generated = render_migrated_rules(&bodies)?;
    let mut rules = Vec::new();
    for (scope, body) in &mut generated {
        let name = if scope.is_empty() {
            "migrated-harness".to_string()
        } else {
            format!("AGENTS-{}", scope.replace('/', "-"))
        };
        let mut relative = format!(".agents/rules/{name}.md");
        if scope.is_empty() && root.join(&relative).exists() {
            relative = format!(
                ".agents/rules/{name}-{}.md",
                backup.file_name().unwrap().to_string_lossy()
            );
        }
        let target = root.join(&relative);
        if target.exists()
            || has_symlink_component(&root, &target)
            || rules.iter().any(|rule: &serde_json::Value| {
                rule["path"]
                    .as_str()
                    .is_some_and(|path| path.eq_ignore_ascii_case(&relative))
            })
        {
            return Err(format!("migration destination conflict: {relative}"));
        }
        let original_name = INSTRUCTION_NAMES
            .into_iter()
            .find(|name| {
                bodies.iter().any(|(source, _)| {
                    Path::new(source).file_name().and_then(|file| file.to_str()) == Some(*name)
                        && instruction_scope(source)
                            .is_ok_and(|source_scope| source_scope == scope.as_str())
                })
            })
            .ok_or("missing migration source for rule scope")?;
        *body = with_rule_name(body, original_name)?;
        parse_rule(body)?;
        rules.push(serde_json::json!({"path": relative, "scope": scope, "name": original_name, "sha256": sha256_hex(body.as_bytes())}));
    }
    let ignore_change = if git_root(&root).is_some() {
        let mut projected_rules = if agents.join("rules").is_dir() {
            read_rules(&root)?
        } else {
            BTreeMap::new()
        };
        for (scope, body) in &generated {
            projected_rules
                .entry(scope.clone())
                .or_default()
                .push(body.clone());
        }
        let path = root.join(".gitignore");
        if path.is_symlink() {
            return Err("symlink output conflict: .gitignore".into());
        }
        let old = if path.exists() {
            Some(fs::read_to_string(&path).map_err(|e| format!(".gitignore: {e}"))?)
        } else {
            None
        };
        let mut paths = projection_outputs(&root, &projected_rules)?
            .into_iter()
            .map(|(path, _, _)| path)
            .collect::<Vec<_>>();
        paths.extend(managed_output_paths(old.as_deref().unwrap_or(""))?);
        let mut seen = std::collections::HashSet::new();
        paths.retain(|path| seen.insert(path.clone()));
        let new = update_ignore_paths(old.as_deref().unwrap_or(""), &paths)?;
        if old.as_deref() == Some(&new) {
            None
        } else {
            Some((old, new))
        }
    } else {
        None
    };
    if let Some((old, _)) = &ignore_change {
        println!(
            "Discovered {} .gitignore: generated projections need ignore entries.",
            if old.is_some() { "outdated" } else { "missing" }
        );
    }
    println!("Migration proposed for {}:", root.display());
    for relative in &found {
        let scope = instruction_scope(relative)?;
        let rule = rules
            .iter()
            .find(|rule| rule["scope"].as_str() == Some(scope.as_str()))
            .ok_or("missing migration destination for source")?;
        println!(
            "  Copy instructions from {relative} -> {} (scope: {})",
            rule["path"].as_str().unwrap(),
            if scope.is_empty() {
                "repository"
            } else {
                &scope
            }
        );
        if tracked.contains(&relative.as_str()) {
            println!("  Remove {relative} from Git before generating its projection");
        }
    }
    if let Some((old, new)) = &ignore_change {
        println!(
            "  {} .gitignore: ignore generated harness projections",
            if old.is_some() { "Update" } else { "Create" }
        );
        let old_entries = old.as_deref().unwrap_or("");
        let mut displayed = std::collections::HashSet::new();
        for entry in new
            .lines()
            .filter(|line| line.starts_with('/') && !old_entries.lines().any(|old| old == *line))
        {
            let entry = ["/.claude", "/.github", "/.codex", "/.vscode", "/.cursor"]
                .into_iter()
                .find(|root| {
                    entry
                        .strip_prefix(root)
                        .is_some_and(|suffix| suffix.starts_with('/'))
                })
                .unwrap_or(entry);
            if displayed.insert(entry) {
                println!("    Add {entry}");
            }
        }
    }
    println!("  Rollback this migration with `rai rollback`");
    print!("Proceed with migration? [y/N] ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        println!("Migration cancelled; no files changed");
        return Ok(());
    }
    for (relative, body) in &bodies {
        if has_symlink_component(&root, &root.join(relative))
            || fs::read(root.join(relative)).map_err(|e| e.to_string())? != body.as_bytes()
        {
            return Err(format!(
                "migration source changed during confirmation: {relative}"
            ));
        }
    }
    if let Some((old, _)) = &ignore_change {
        let path = root.join(".gitignore");
        let current = if path.exists() {
            Some(fs::read_to_string(&path).map_err(|e| format!(".gitignore: {e}"))?)
        } else {
            None
        };
        if path.is_symlink() || &current != old {
            return Err(".gitignore changed during confirmation".into());
        }
    }
    let legacy = rules.len() == 1
        && rules[0]["path"] == ".agents/rules/migrated-harness.md"
        && rules[0]["scope"] == "";
    let mut manifest = serde_json::json!({
        "version": if legacy { 1 } else { 2 },
        "repository": root.to_string_lossy(),
        "createdAgents": !agents.exists(),
        "sources": sources,
    });
    if let Some((old, new)) = &ignore_change {
        manifest["gitignore"] = serde_json::json!({"before": old, "after": new});
    }
    if legacy {
        manifest["generated"] = serde_json::json!(".agents/rules/migrated-harness.md");
        manifest["generatedSha256"] = rules[0]["sha256"].clone();
        manifest["generatedName"] = rules[0]["name"].clone();
    } else {
        manifest["generatedRules"] = serde_json::json!(rules);
    }
    fs::create_dir_all(backup.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::create_dir(&backup).map_err(|e| format!("{}: {e}", backup.display()))?;
    for relative in &found {
        let saved = backup.join(relative);
        fs::create_dir_all(saved.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(root.join(relative), &saved).map_err(|e| format!("{relative}: {e}"))?;
    }
    fs::write(
        backup.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    create_canonical_layout(&agents)?;
    for rule in &rules {
        fs::write(
            root.join(rule["path"].as_str().unwrap()),
            &generated[rule["scope"].as_str().unwrap()],
        )
        .map_err(|e| e.to_string())?;
    }
    for relative in &found {
        if !tracked.contains(&relative.as_str()) {
            fs::remove_file(root.join(relative)).map_err(|e| format!("{relative}: {e}"))?;
        }
    }
    if let Some((_, new)) = &ignore_change {
        fs::write(root.join(".gitignore"), new).map_err(|e| format!(".gitignore: {e}"))?;
    }
    println!(
        "Migrated {} native instruction file(s) into {} canonical rule(s)",
        found.len(),
        rules.len()
    );
    println!(
        "Originals saved under {}. Review the rules, then run rai sync --dry-run",
        backup.display()
    );
    setup::ensure_first_setup(&root)?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn has_symlink_component(root: &Path, path: &Path) -> bool {
    path.ancestors()
        .take_while(|part| *part != root)
        .any(Path::is_symlink)
}

fn rollback_migration(start: &Path) -> Result<(), String> {
    let start = start
        .canonicalize()
        .map_err(|e| format!("{}: {e}", start.display()))?;
    let root = git_root(&start).unwrap_or(start);
    let backup =
        existing_migration_backup(&root)?.ok_or("no migration backup found for this repository")?;
    let manifest = read_migration_manifest(&backup, &root)?;
    let rules = manifest_rules(&manifest)?;
    for rule in &rules {
        let target = root.join(&rule.path);
        if has_symlink_component(&root, &target)
            || !target.is_file()
            || sha256_hex(&fs::read(&target).map_err(|e| e.to_string())?) != rule.hash
        {
            return Err("migrated rules changed; review them before rollback".into());
        }
    }
    let sources = manifest["sources"]
        .as_array()
        .ok_or("invalid migration manifest sources")?;
    let restore_ignore = manifest
        .get("gitignore")
        .filter(|_| git_root(&root).is_some());
    if let Some(ignore) = restore_ignore {
        let path = root.join(".gitignore");
        if path.is_symlink()
            || fs::read_to_string(&path).ok().as_deref() != ignore["after"].as_str()
        {
            return Err("rollback conflict: .gitignore changed since migration".into());
        }
    }
    for source in sources {
        let relative = source["path"].as_str().ok_or("invalid migration source")?;
        instruction_scope(relative)?;
        let saved = backup.join(relative);
        let original = root.join(relative);
        if has_symlink_component(&backup, &saved)
            || !saved.is_file()
            || sha256_hex(&fs::read(&saved).map_err(|e| e.to_string())?)
                != source["sha256"]
                    .as_str()
                    .ok_or("invalid migration source hash")?
        {
            return Err(format!("migration backup changed: {relative}"));
        }
        if has_symlink_component(&root, &original) {
            return Err(format!("symlink rollback conflict: {relative}"));
        }
        if original.exists() || original.is_symlink() {
            if original.is_file()
                && !original.is_symlink()
                && sha256_hex(&fs::read(&original).map_err(|e| e.to_string())?)
                    == source["sha256"]
                        .as_str()
                        .ok_or("invalid migration source hash")?
            {
                continue;
            }
            let owned_projection = !original.is_symlink()
                && original.is_file()
                && !is_tracked(&root, relative)?
                && is_owned(
                    &fs::read_to_string(&original).map_err(|e| e.to_string())?,
                    "",
                );
            if !owned_projection {
                return Err(format!("rollback conflict: {relative} already exists"));
            }
        }
    }
    for source in sources {
        let relative = source["path"].as_str().unwrap();
        let original = root.join(relative);
        if original.is_file()
            && sha256_hex(&fs::read(&original).map_err(|e| e.to_string())?)
                == source["sha256"].as_str().unwrap()
        {
            continue;
        }
        fs::create_dir_all(original.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(backup.join(relative), &original).map_err(|e| format!("{relative}: {e}"))?;
    }
    for rule in &rules {
        fs::remove_file(root.join(&rule.path)).map_err(|e| e.to_string())?;
    }
    if let Some(ignore) = restore_ignore {
        let path = root.join(".gitignore");
        if let Some(before) = ignore["before"].as_str() {
            fs::write(path, before).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    println!(
        "Restored {} original instruction file(s); backup retained at {}",
        sources.len(),
        backup.display()
    );
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
    WorkspaceMissing(PathBuf),
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
            solution: "Run rai sync to create an empty .agents/ source tree, then add the resources your project needs.".into(),
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
    if root.join("AGENTS.md").exists()
        && !fs::read_to_string(root.join("AGENTS.md"))
            .ok()
            .is_some_and(|s| is_owned(&s, ""))
    {
        issues.push(DoctorIssue {
            message: "unmanaged AGENTS.md exists".into(),
            solution: "Run rai migrate to import untracked root instructions, or manually convert tracked native files before sync.".into(),
            fix: None,
        });
    }
    for issue in setup::diagnostics() {
        let can_setup = setup::can_run_setup();
        let (message, solution, fix) = match issue {
            setup::Diagnostic::ConfigUnavailable => (
                "cannot locate per-user setup configuration".into(),
                "Set the user configuration directory (HOME or XDG_CONFIG_HOME on Unix, APPDATA on Windows), then run rai install --root PATH.".into(),
                None,
            ),
            setup::Diagnostic::ConfigUnreadable => (
                "cannot read configured workspace roots".into(),
                "Inspect the per-user rai/roots.txt and root-identities.json files and repair their permissions or contents before rerunning rai install.".into(),
                None,
            ),
            setup::Diagnostic::NoRoots => {
                let workspace = root.to_path_buf();
                let command = format!("rai install --root {}", setup::shell_quote(&workspace.to_string_lossy()));
                (
                    "rai install has not configured any workspace roots".into(),
                    if can_setup { format!("Run {command} to watch this repository and install available integrations.") } else { format!("Install rai with cargo install --path ., then run {command}.") },
                    can_setup.then_some(DoctorFix::SetupNew(workspace)),
                )
            }
            setup::Diagnostic::WorkspaceMissing(path) => (
                format!("configured workspace is missing: {}", path.display()),
                "Choose a new location, stop watching it, or keep it for later.".into(),
                Some(DoctorFix::WorkspaceMissing(path)),
            ),
            setup::Diagnostic::WatcherMissing => (
                "watcher service is not installed".into(),
                if can_setup { "Rerun rai install with the existing workspace roots to reinstall the watcher.".into() } else { "Install rai with cargo install --path ., then rerun rai install with the existing workspace roots.".into() },
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
    if error.contains("unsupported rule name:") {
        "Set the rule's name to its original instruction filename: AGENTS.md, CLAUDE.md, or copilot-instructions.md. Use path to select its repository directory.".into()
    } else if error.starts_with("tracked output conflict:") {
        "Review and move the tracked instructions into .agents/rules/, then remove the native file from Git tracking before running rai sync. Adding .gitignore alone will not untrack it.".into()
    } else if error.starts_with("unowned or modified output conflict:") {
        "Back up and review the native file, transfer its intended rules into .agents/rules/, then move the conflicting file aside before running rai sync.".into()
    } else if error.starts_with("missing rules directory:") {
        "Create .agents/rules/ and add rules there when needed, then run rai sync.".into()
    } else if error.starts_with("scoped rule target is not a regular directory:") {
        "Create the target repository directory or remove the rule's path frontmatter to make it global.".into()
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
    let mut deferred = HashSet::new();
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
                    issue
                        .fix
                        .as_ref()
                        .is_some_and(|fix| !matches!(fix, DoctorFix::WorkspaceMissing(_)))
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
        if io::stdin().is_terminal() {
            let pending = issues
                .iter()
                .filter_map(|issue| match &issue.fix {
                    Some(DoctorFix::WorkspaceMissing(path)) if !deferred.contains(path) => {
                        Some(path.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut changed = false;
            for path in pending {
                match prompt_missing_workspace(&path)? {
                    MissingChoice::Replace(new_path) => {
                        setup::change_workspace(&path, Some(&new_path))?;
                        changed = true;
                    }
                    MissingChoice::Remove => {
                        setup::change_workspace(&path, None)?;
                        changed = true;
                    }
                    MissingChoice::Keep => {
                        deferred.insert(path);
                    }
                }
            }
            if changed {
                println!("Rechecking...");
                continue;
            }
        }
        for (index, issue) in issues.iter().enumerate() {
            println!("{}. {}", index + 1, issue.message);
            println!("   Solution: {}", issue.solution);
            if issue
                .fix
                .as_ref()
                .is_some_and(|fix| !matches!(fix, DoctorFix::WorkspaceMissing(_)))
            {
                println!("   Automatic fix available");
            }
        }
        let available: Vec<DoctorFix> = issues
            .iter()
            .filter_map(|issue| {
                issue
                    .fix
                    .clone()
                    .filter(|fix| !matches!(fix, DoctorFix::WorkspaceMissing(_)))
            })
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

enum MissingChoice {
    Replace(PathBuf),
    Remove,
    Keep,
}

fn prompt_missing_workspace(path: &Path) -> Result<MissingChoice, String> {
    loop {
        println!("Configured workspace not found: {}", path.display());
        print!("Choose [1] new location, [2] stop watching, [3] keep for later: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        match answer.trim() {
            "1" => {
                print!("New workspace path: ");
                io::stdout().flush().map_err(|e| e.to_string())?;
                let mut input = String::new();
                io::stdin()
                    .read_line(&mut input)
                    .map_err(|e| e.to_string())?;
                let candidate = PathBuf::from(input.trim());
                if candidate.is_dir() && !candidate.is_symlink() {
                    return Ok(MissingChoice::Replace(candidate));
                }
                println!("That path is not a regular directory.");
            }
            "2" => return Ok(MissingChoice::Remove),
            "3" | "" => return Ok(MissingChoice::Keep),
            _ => println!("Enter 1, 2, or 3."),
        }
    }
}

fn choose_fixes(input: &str, issues: &[DoctorIssue]) -> FixChoice {
    match input.trim().to_ascii_lowercase().as_str() {
        "all" | "a" => FixChoice::Apply {
            fixes: issues
                .iter()
                .filter_map(|issue| {
                    issue
                        .fix
                        .clone()
                        .filter(|fix| !matches!(fix, DoctorFix::WorkspaceMissing(_)))
                })
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
                Some(DoctorFix::WorkspaceMissing(_)) => FixChoice::Manual(index),
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
        DoctorFix::WorkspaceMissing(_) => {
            Err("choose how to handle the missing workspace interactively".into())
        }
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
            Action::Delete => "delete",
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

fn read_rules(root: &Path) -> Result<BTreeMap<String, Vec<String>>, String> {
    let commands = root.join(".agents/commands");
    if commands.is_symlink() {
        return Err("symlink source unsupported: .agents/commands".into());
    }
    if commands.exists() {
        for entry in fs::read_dir(&commands).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.file_name().is_some_and(|name| name == ".keep")
                && path.is_file()
                && !path.is_symlink()
            {
                continue;
            }
            return Err(format!(
                "command projection is not supported yet: {}",
                path.display()
            ));
        }
    }
    let dir = root.join(".agents/rules");
    if dir.is_symlink() || dir.parent().is_some_and(Path::is_symlink) {
        return Err(format!("symlink source unsupported: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("missing rules directory: {}", dir.display()));
    }
    let mut sections: BTreeMap<String, Vec<String>> = BTreeMap::new();
    {
        let mut files = fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        files.sort();
        for path in files {
            if path.file_name().is_some_and(|name| name == ".keep")
                && path.is_file()
                && !path.is_symlink()
            {
                continue;
            }
            if path.is_symlink() {
                return Err(format!("symlink rule unsupported: {}", path.display()));
            }
            if path.is_dir() {
                return Err(format!(
                    "rules must be Markdown files directly in .agents/rules: {}",
                    path.display()
                ));
            }
            if path.extension().is_none_or(|ext| ext != "md") {
                return Err(format!("only .md rules are supported: {}", path.display()));
            }
            let source = path.strip_prefix(&dir).map_err(|e| e.to_string())?;
            let content =
                fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let (scope, name, body) =
                parse_rule(&content).map_err(|e| format!("{}: {e}", path.display()))?;
            if !scope.is_empty() {
                let target = root.join(&scope);
                if target.is_symlink() || !target.is_dir() {
                    return Err(format!(
                        "scoped rule target is not a regular directory: {}",
                        target.display()
                    ));
                }
            }
            sections.entry(scope).or_default().push(format!(
                "## {}\n\n{}",
                name.unwrap_or_else(|| source.to_string_lossy().into_owned()),
                body.trim()
            ));
        }
    }
    Ok(sections)
}

fn parse_rule(content: &str) -> Result<(String, Option<String>, &str), String> {
    let Some(rest) = content.strip_prefix("---\n") else {
        return Ok((String::new(), None, content));
    };
    let (metadata, body) = rest
        .split_once("\n---\n")
        .ok_or("unterminated rule frontmatter")?;
    let metadata: serde_yaml::Value =
        serde_yaml::from_str(metadata).map_err(|e| format!("invalid rule frontmatter: {e}"))?;
    let fields = metadata
        .as_mapping()
        .ok_or("rule frontmatter must be an object")?;
    if fields.is_empty() {
        return Err("rule frontmatter requires `type`, `name` or `path`".into());
    }
    let mut scope = String::new();
    let mut name = None;
    for (field, value) in fields {
        match field.as_str() {
            Some("type") => {
                if value.as_str() != Some("rule") {
                    return Err("rule type must be `rule` in .agents/rules".into());
                }
            }
            Some("name") => {
                let value = value.as_str().ok_or("rule name must be a string")?;
                if value.trim().is_empty() || value.chars().any(char::is_control) {
                    return Err("rule name must be a nonempty single-line string".into());
                }
                if !INSTRUCTION_NAMES.contains(&value) {
                    return Err(format!(
                        "unsupported rule name: {value}; expected AGENTS.md, CLAUDE.md or copilot-instructions.md"
                    ));
                }
                name = Some(value.to_owned());
            }
            Some("path") => {
                let value = value.as_str().ok_or("rule path must be a string")?.trim();
                if value == "." {
                    scope = String::new();
                } else if value.is_empty()
                    || value.contains('\\')
                    || value.chars().any(char::is_control)
                    || !Path::new(value)
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_)))
                {
                    return Err(format!("invalid rule path: {value}"));
                } else {
                    scope = value.to_owned();
                }
            }
            _ => {
                return Err(
                    "unsupported rule frontmatter field; expected `type`, `name` or `path`".into(),
                );
            }
        }
    }
    Ok((scope, name, body))
}

fn managed_output_paths(ignore: &str) -> Result<Vec<String>, String> {
    let Some(start) = ignore.find(IGNORE_START) else {
        return Ok(Vec::new());
    };
    let end = ignore
        .find(IGNORE_END)
        .ok_or("malformed RosettAI block in .gitignore")?;
    if start >= end {
        return Err("malformed RosettAI block in .gitignore".into());
    }
    let mut paths = Vec::new();
    for line in ignore[start + IGNORE_START.len()..end].lines() {
        let Some(relative) = line.strip_prefix('/') else {
            continue;
        };
        let candidate = Path::new(relative);
        if candidate
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            paths.push(relative.to_owned());
        }
    }
    Ok(paths)
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

fn owned_comment(body: &str) -> String {
    let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
    format!("# rai-generated sha256:{hash}\n{body}")
}

fn is_owned_comment(content: &str) -> bool {
    let Some(rest) = content.strip_prefix("# rai-generated sha256:") else {
        return false;
    };
    let Some((hash, body)) = rest.split_once('\n') else {
        return false;
    };
    hash.len() == 64 && hash == format!("{:x}", Sha256::digest(body.as_bytes()))
}

fn owned_slash_comment(body: &str) -> String {
    let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
    format!("// rai-generated sha256:{hash}\n{body}")
}

fn is_owned_slash_comment(content: &str) -> bool {
    let Some(rest) = content.strip_prefix("// rai-generated sha256:") else {
        return false;
    };
    let Some((hash, body)) = rest.split_once('\n') else {
        return false;
    };
    hash.len() == 64 && hash == format!("{:x}", Sha256::digest(body.as_bytes()))
}

fn owned_json(body: &str) -> Result<String, String> {
    let mut value: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
    value
        .as_object_mut()
        .ok_or("JSON projection must be an object")?
        .insert("_rai_generated_sha256".into(), hash.into());
    serde_json::to_string_pretty(&value)
        .map(|s| format!("{s}\n"))
        .map_err(|e| e.to_string())
}

fn is_owned_json(content: &str) -> bool {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content) else {
        return false;
    };
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let Some(hash) = object
        .remove("_rai_generated_sha256")
        .and_then(|v| v.as_str().map(str::to_owned))
    else {
        return false;
    };
    let Ok(body) = serde_json::to_string_pretty(&value) else {
        return false;
    };
    hash == format!("{:x}", Sha256::digest(format!("{body}\n").as_bytes()))
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

fn update_ignore_paths(old: &str, paths: &[String]) -> Result<String, String> {
    let entries = paths
        .iter()
        .filter(|p| p.as_str() != ".gitignore")
        .map(|p| format!("/{p}\n"))
        .collect::<String>();
    let block = format!("{IGNORE_START}\n{entries}{IGNORE_END}\n");
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
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
        fs::create_dir_all(dir.path().join(".agents/rules")).unwrap();
        fs::write(
            dir.path().join(".agents/rules/general.md"),
            "Prefer clear names.\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn creates_codex_projections_and_is_idempotent() {
        let dir = repo();
        sync(dir.path(), false).unwrap();
        let agents = fs::read_to_string(dir.path().join(CODEX)).unwrap();
        let config = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert!(agents.contains("Prefer clear names."));
        assert!(is_owned(&agents, ""));
        assert!(is_owned_comment(&config));
        assert!(dir.path().join("CLAUDE.md").exists());
        assert!(!dir.path().join(LEGACY_CURSOR).exists());
        let ignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        sync(dir.path(), false).unwrap();
        assert_eq!(agents, fs::read_to_string(dir.path().join(CODEX)).unwrap());
        assert_eq!(
            config,
            fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap()
        );
        assert_eq!(
            ignore,
            fs::read_to_string(dir.path().join(".gitignore")).unwrap()
        );
        assert_eq!(ignore.matches(IGNORE_START).count(), 1);
    }

    #[test]
    fn codex_projection_converts_rules_mcp_and_preserves_skills() {
        let dir = repo();
        fs::write(dir.path().join(".agents/mcp.yaml"), "servers:\n  docs:\n    transport: http\n    url: https://example.com/mcp\n    bearer_token_env_var: DOCS_TOKEN\n  local:\n    transport: stdio\n    command: npx\n    args: [-y, example-mcp]\n    env_vars: [LOCAL_TOKEN]\n    default_tools_approval_mode: approve\n").unwrap();
        fs::create_dir_all(dir.path().join(".agents/skills/checks")).unwrap();
        fs::write(
            dir.path().join(".agents/skills/checks/SKILL.md"),
            "---\nname: checks\ndescription: Check changes.\n---\n\nRun tests.\n",
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        let agents = fs::read_to_string(dir.path().join(CODEX)).unwrap();
        let config = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert!(agents.contains("Prefer clear names."));
        assert!(config.contains("[mcp_servers.docs]"));
        assert!(config.contains("[mcp_servers.local]"));
        assert!(config.contains("[[hooks.UserPromptSubmit]]"));
        let parsed: toml::Value = config.parse().unwrap();
        assert_eq!(
            parsed["mcp_servers"]["docs"]["url"].as_str(),
            Some("https://example.com/mcp")
        );
        assert_eq!(
            parsed["mcp_servers"]["local"]["default_tools_approval_mode"].as_str(),
            Some("approve")
        );
        assert_eq!(
            parsed["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"].as_str(),
            Some("rai sync --codex-hook")
        );
        assert!(dir.path().join(".agents/skills/checks/SKILL.md").exists());
        assert!(is_owned(&agents, ""));
        assert!(is_owned_comment(&config));
        assert_eq!(
            plan_sync(dir.path())
                .unwrap()
                .iter()
                .filter(|c| c.action != Action::Unchanged)
                .count(),
            0
        );
    }

    #[test]
    fn codex_version_parser_and_gate() {
        assert_eq!(
            codex::parse_version("codex-cli 0.152.1\n").unwrap(),
            (0, 152, 1)
        );
        assert_eq!(
            codex::parse_version("codex-cli 0.153.0-beta.1").unwrap(),
            (0, 153, 0)
        );
        assert!(codex::parse_version("other 0.152.1").is_err());
    }

    #[test]
    fn codex_unowned_agents_file_aborts() {
        let dir = repo();
        fs::write(dir.path().join(CODEX), "User rules\n").unwrap();
        assert!(sync(dir.path(), false).unwrap_err().contains("unowned"));
        assert!(!dir.path().join(".codex/config.toml").exists());
    }

    #[test]
    fn canonical_markdown_subagents_are_projected() {
        let dir = repo();
        fs::create_dir_all(dir.path().join(".agents/subagents")).unwrap();
        let source = dir.path().join(".agents/subagents/reviewer.md");
        fs::write(
            &source,
            "---\nname: reviewer\ndescription: Review changes\nmodel: gpt-6-luna\nmodel_reasoning_effort: high\nsandbox_mode: read-only\nnickname_candidates: [Atlas, Delta]\n---\n\nCheck tests and regressions.\n",
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        let config = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        let agent = fs::read_to_string(dir.path().join(".codex/agents/reviewer.toml")).unwrap();
        assert!(config.contains("[agents.reviewer]"));
        assert!(config.contains("config_file = \"agents/reviewer.toml\""));
        assert!(is_owned_comment(&agent));
        let parsed: toml::Value = agent.parse().unwrap();
        assert_eq!(parsed["name"].as_str(), Some("reviewer"));
        assert_eq!(parsed["model"].as_str(), Some("gpt-6-luna"));
        assert_eq!(
            parsed["developer_instructions"].as_str(),
            Some("Check tests and regressions.\n")
        );
        assert_eq!(
            parsed["nickname_candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>(),
            ["Atlas", "Delta"]
        );
    }

    #[test]
    fn removing_markdown_subagent_removes_only_owned_projection() {
        let dir = repo();
        fs::create_dir_all(dir.path().join(".agents/subagents")).unwrap();
        let source = dir.path().join(".agents/subagents/reviewer.md");
        fs::write(
            &source,
            "---\nname: reviewer\ndescription: Review changes\n---\n\nCheck tests.\n",
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        let projection = dir.path().join(".codex/agents/reviewer.toml");
        assert!(projection.exists());
        fs::remove_file(source).unwrap();
        sync(dir.path(), false).unwrap();
        assert!(!projection.exists());
        let config = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert!(!config.contains("agents.reviewer"));
    }

    #[test]
    fn legacy_sources_remain_supported_without_ambiguous_duplicates() {
        let dir = repo();
        fs::create_dir_all(dir.path().join(".agents/subagents")).unwrap();
        fs::write(
            dir.path().join(".agents/subagents/reviewer.yaml"),
            "name: reviewer\ndescription: Review changes\ndeveloper_instructions: Check tests.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join(".agents/mcp.json"),
            r#"{"servers":{"docs":{"transport":"http","url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        assert!(dir.path().join(".codex/agents/reviewer.toml").exists());
        fs::write(
            dir.path().join(".agents/subagents/reviewer.md"),
            "---\nname: reviewer\ndescription: Review changes\n---\n\nCheck tests.\n",
        )
        .unwrap();
        assert!(
            plan_sync(dir.path())
                .err()
                .unwrap()
                .contains("duplicate subagent name")
        );
        fs::remove_file(dir.path().join(".agents/subagents/reviewer.md")).unwrap();
        fs::write(dir.path().join(".agents/mcp.yaml"), "servers: {}\n").unwrap();
        assert!(
            plan_sync(dir.path())
                .err()
                .unwrap()
                .contains("both .agents/mcp.yaml and .agents/mcp.json")
        );
    }

    #[test]
    fn removes_only_owned_legacy_outputs() {
        let dir = repo();
        fs::create_dir_all(dir.path().join(".cursor/rules")).unwrap();
        fs::write(dir.path().join("CLAUDE.md"), owned("legacy\n", "")).unwrap();
        fs::write(dir.path().join(LEGACY_CURSOR), "manual rule\n").unwrap();
        sync(dir.path(), false).unwrap();
        assert!(dir.path().join("CLAUDE.md").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(LEGACY_CURSOR)).unwrap(),
            "manual rule\n"
        );
        assert!(dir.path().join(CODEX).exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_codex_agents_directory_is_rejected() {
        use std::os::unix::fs::symlink;
        let dir = repo();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".codex")).unwrap();
        symlink(outside.path(), dir.path().join(".codex/agents")).unwrap();
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("symlink output")
        );
        assert!(!dir.path().join(CODEX).exists());
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = repo();
        sync(dir.path(), true).unwrap();
        assert!(!dir.path().join(CODEX).exists());
        assert!(!dir.path().join(".codex/config.toml").exists());
        assert!(!dir.path().join(".gitignore").exists());
    }

    #[test]
    fn unowned_collision_aborts_all_writes() {
        let dir = repo();
        fs::write(dir.path().join(CODEX), "Personal instructions\n").unwrap();
        let error = sync(dir.path(), false).unwrap_err();
        assert!(error.contains("unowned"));
        assert!(!dir.path().join(".codex/config.toml").exists());
        assert!(!dir.path().join(".gitignore").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(CODEX)).unwrap(),
            "Personal instructions\n"
        );
    }

    #[test]
    fn edited_projection_is_not_overwritten() {
        let dir = repo();
        sync(dir.path(), false).unwrap();
        let path = dir.path().join(CODEX);
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
    fn rule_changes_update_codex_output() {
        let dir = repo();
        sync(dir.path(), false).unwrap();
        fs::write(
            dir.path().join(".agents/rules/general.md"),
            "Write tests.\n",
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        assert!(
            fs::read_to_string(dir.path().join(CODEX))
                .unwrap()
                .contains("Write tests.")
        );
    }

    #[test]
    fn scoped_rules_generate_nested_agents_without_leaking_to_root() {
        let dir = repo();
        fs::create_dir_all(dir.path().join("frontend/components")).unwrap();
        fs::write(
            dir.path().join(".agents/rules/frontend.md"),
            "---\npath: frontend\n---\nUse frontend conventions.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join(".agents/rules/components.md"),
            "---\npath: frontend/components\n---\nUse component conventions.\n",
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        let root = fs::read_to_string(dir.path().join(CODEX)).unwrap();
        let frontend = fs::read_to_string(dir.path().join("frontend/AGENTS.md")).unwrap();
        let components =
            fs::read_to_string(dir.path().join("frontend/components/AGENTS.md")).unwrap();
        assert!(root.contains("Prefer clear names."));
        assert!(!root.contains("frontend conventions"));
        assert!(frontend.contains("frontend conventions"));
        assert!(!frontend.contains("component conventions"));
        assert!(components.contains("component conventions"));
        let ignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(ignore.contains("/frontend/AGENTS.md"));
        assert!(ignore.contains("/frontend/components/AGENTS.md"));
    }

    #[test]
    fn two_flat_rules_merge_into_same_scope() {
        let dir = repo();
        fs::create_dir(dir.path().join("frontend")).unwrap();
        fs::write(
            dir.path().join(".agents/rules/frontend.md"),
            "---\npath: frontend\n---\nNamed scope.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join(".agents/rules/frontend-extra.md"),
            "---\npath: frontend\n---\nExtra scope.\n",
        )
        .unwrap();
        sync(dir.path(), false).unwrap();
        let projected = fs::read_to_string(dir.path().join("frontend/AGENTS.md")).unwrap();
        assert!(projected.contains("Named scope."));
        assert!(projected.contains("Extra scope."));
        assert!(
            !fs::read_to_string(dir.path().join(CODEX))
                .unwrap()
                .contains("Extra scope.")
        );
    }

    #[test]
    fn scoped_rule_without_target_directory_fails_before_writing() {
        let dir = repo();
        fs::write(
            dir.path().join(".agents/rules/frontend.md"),
            "---\npath: frontend\n---\nFrontend only.\n",
        )
        .unwrap();
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("scoped rule target")
        );
        assert!(!dir.path().join(CODEX).exists());
    }

    #[test]
    fn invalid_scope_cannot_escape_repository() {
        let dir = repo();
        fs::write(
            dir.path().join(".agents/rules/frontend.md"),
            "---\npath: ../outside\n---\nUnsafe.\n",
        )
        .unwrap();
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("invalid rule path")
        );
        assert!(!dir.path().join(CODEX).exists());
    }

    #[test]
    fn nested_rule_sources_are_rejected() {
        let dir = repo();
        fs::create_dir(dir.path().join(".agents/rules/frontend")).unwrap();
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("directly in .agents/rules")
        );
        assert!(!dir.path().join(CODEX).exists());
    }

    #[test]
    fn removed_scoped_rule_removes_only_owned_nested_projection() {
        let dir = repo();
        fs::create_dir(dir.path().join("frontend")).unwrap();
        let source = dir.path().join(".agents/rules/frontend.md");
        fs::write(&source, "---\npath: frontend\n---\nFrontend only.\n").unwrap();
        sync(dir.path(), false).unwrap();
        let projection = dir.path().join("frontend/AGENTS.md");
        assert!(projection.exists());
        fs::remove_file(source).unwrap();
        sync(dir.path(), false).unwrap();
        assert!(!projection.exists());
        assert!(
            !fs::read_to_string(dir.path().join(".gitignore"))
                .unwrap()
                .contains("/frontend/AGENTS.md")
        );
    }

    #[test]
    fn unowned_scoped_output_blocks_all_writes() {
        let dir = repo();
        fs::create_dir(dir.path().join("frontend")).unwrap();
        fs::write(
            dir.path().join(".agents/rules/frontend.md"),
            "---\npath: frontend\n---\nFrontend only.\n",
        )
        .unwrap();
        fs::write(dir.path().join("frontend/AGENTS.md"), "Handwritten\n").unwrap();
        assert!(sync(dir.path(), false).unwrap_err().contains("unowned"));
        assert!(!dir.path().join(CODEX).exists());
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
        fs::write(dir.path().join(CODEX), "Tracked instructions\n").unwrap();
        let status = Command::new("git")
            .args(["add", CODEX])
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
        symlink(outside.path(), dir.path().join(".codex")).unwrap();
        assert!(
            sync(dir.path(), false)
                .unwrap_err()
                .contains("symlink output")
        );
    }
}
