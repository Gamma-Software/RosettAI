use std::fs;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

fn invoke_cursor_hook(repo: &std::path::Path) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--cursor-hook", "--repo"])
        .arg(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"prompt":"Implement the feature","attachments":[]}"#)
        .unwrap();
    child.wait_with_output().unwrap()
}

fn assert_time_taken(stderr: &[u8]) {
    let output = String::from_utf8_lossy(stderr);
    let line = output
        .lines()
        .find(|line| line.starts_with("Time taken: "))
        .expect("timing line");
    let milliseconds = line
        .strip_prefix("Time taken: ")
        .unwrap()
        .strip_suffix(" ms")
        .unwrap();
    assert!(milliseconds.parse::<f64>().unwrap() >= 0.0);
    assert!(!output.contains("cpu_") && !output.contains("rss"));
}

#[test]
fn cli_dry_run_then_sync_then_noop() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Keep tests close.\n",
    )
    .unwrap();

    let invoke = |dry_run: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rai"));
        command.arg("sync").arg("--repo").arg(repo.path());
        if dry_run {
            command.arg("--dry-run");
        }
        command.output().unwrap()
    };

    let preview = invoke(true);
    assert!(preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stdout).contains("Create CLAUDE.md"));
    assert!(!repo.path().join("CLAUDE.md").exists());

    let first = invoke(false);
    assert!(first.status.success());
    assert!(repo.path().join("CLAUDE.md").exists());
    assert!(repo.path().join(".cursor/rules/rosettai.mdc").exists());

    let second = invoke(false);
    assert!(second.status.success());
    assert!(String::from_utf8_lossy(&second.stdout).contains("Unchanged CLAUDE.md"));
}

#[test]
fn cursor_hook_allows_an_already_synchronized_prompt() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Use explicit error types.\n",
    )
    .unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_rai"))
            .args(["sync", "--repo"])
            .arg(repo.path())
            .status()
            .unwrap()
            .success()
    );

    let hook = invoke_cursor_hook(repo.path());
    assert!(hook.status.success());
    assert_eq!(
        String::from_utf8_lossy(&hook.stdout).trim(),
        r#"{"continue":true}"#
    );
    assert!(hook.stderr.is_empty());
}

#[test]
fn cursor_hook_syncs_drift_blocks_once_then_allows_resubmission() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    let source = repo.path().join(".agents/rules/general.md");
    fs::write(&source, "Use the old API.\n").unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_rai"))
            .args(["sync", "--repo"])
            .arg(repo.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(&source, "Use the new API.\n").unwrap();

    let first = invoke_cursor_hook(repo.path());
    let first_json = String::from_utf8_lossy(&first.stdout);
    assert!(first.status.success());
    assert!(first_json.contains(r#""continue":false"#));
    assert!(first_json.contains("Resubmit your prompt"));
    assert!(first_json.contains(".cursor/rules/rosettai.mdc"));
    let projection = fs::read_to_string(repo.path().join(".cursor/rules/rosettai.mdc")).unwrap();
    assert!(projection.contains("Use the new API."));
    assert!(!projection.contains("Use the old API."));

    let second = invoke_cursor_hook(repo.path());
    assert!(second.status.success());
    assert_eq!(
        String::from_utf8_lossy(&second.stdout).trim(),
        r#"{"continue":true}"#
    );
}

#[test]
fn cursor_hook_blocks_conflicts_without_overwriting_user_files() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Canonical rule.\n",
    )
    .unwrap();
    fs::create_dir_all(repo.path().join(".cursor/rules")).unwrap();
    let projection = repo.path().join(".cursor/rules/rosettai.mdc");
    fs::write(&projection, "User-owned Cursor rule.\n").unwrap();

    let hook = invoke_cursor_hook(repo.path());
    let output = String::from_utf8_lossy(&hook.stdout);
    assert!(hook.status.success());
    assert!(output.contains(r#""continue":false"#));
    assert!(output.contains("synchronization is blocked"));
    assert!(output.contains("unowned or modified output conflict"));
    assert_eq!(
        fs::read_to_string(&projection).unwrap(),
        "User-owned Cursor rule.\n"
    );
    assert!(!repo.path().join("CLAUDE.md").exists());
}

#[test]
fn cursor_hook_blocks_when_canonical_rules_are_invalid() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules/scoped")).unwrap();

    let hook = invoke_cursor_hook(repo.path());
    let output = String::from_utf8_lossy(&hook.stdout);
    assert!(hook.status.success());
    assert!(output.contains(r#""continue":false"#));
    assert!(output.contains("scoped rules are not supported"));
    assert!(!repo.path().join(".cursor/rules/rosettai.mdc").exists());
}

#[test]
fn cursor_hook_blocks_when_agents_directory_is_missing() {
    let repo = tempfile::tempdir().unwrap();

    let hook = invoke_cursor_hook(repo.path());
    let output = String::from_utf8_lossy(&hook.stdout);
    assert!(hook.status.success());
    assert!(output.contains(r#""continue":false"#));
    assert!(output.contains("RosettAI cannot check this prompt"));
    assert!(output.contains("no .agents/ directory found"));
    assert!(hook.stderr.is_empty());
}

#[test]
fn perf_reports_success_and_failure_without_polluting_json() {
    let repo = tempfile::tempdir().unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rai"))
            .args(args)
            .arg("--repo")
            .arg(repo.path())
            .arg("--perf")
            .output()
            .unwrap()
    };

    let initialized = invoke(&["init"]);
    assert!(initialized.status.success());
    assert_time_taken(&initialized.stderr);

    let status = invoke(&["status", "--json"]);
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).starts_with("{\"ok\":true"));
    assert_time_taken(&status.stderr);

    let preview = invoke(&["sync", "--dry-run", "--json"]);
    assert!(preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stdout).starts_with("{\"ok\":true"));
    assert_time_taken(&preview.stderr);

    let doctor = invoke(&["doctor", "--json"]);
    assert!(!doctor.status.success());
    assert!(String::from_utf8_lossy(&doctor.stdout).starts_with("{\"ok\":false"));
    assert_time_taken(&doctor.stderr);

    let invalid = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["unknown", "--perf"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert_time_taken(&invalid.stderr);

    let watch = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["watch", "--perf"])
        .env("XDG_CONFIG_HOME", repo.path())
        .output()
        .unwrap();
    assert!(!watch.status.success());
    assert_time_taken(&watch.stderr);
}

#[test]
fn init_status_and_doctor_commands() {
    let repo = tempfile::tempdir().unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rai"))
            .args(args)
            .arg("--repo")
            .arg(repo.path())
            .env("XDG_CONFIG_HOME", repo.path().join("config"))
            .output()
            .unwrap()
    };

    let initialized = invoke(&["init"]);
    assert!(initialized.status.success());
    assert!(repo.path().join(".agents/rules/general.md").exists());
    assert!(!invoke(&["init"]).status.success());

    let before = invoke(&["status", "--json"]);
    assert!(before.status.success());
    assert!(String::from_utf8_lossy(&before.stdout).contains("\"inSync\":false"));
    assert!(invoke(&["sync"]).status.success());
    let after = invoke(&["status", "--json"]);
    assert!(after.status.success());
    assert!(String::from_utf8_lossy(&after.stdout).contains("\"inSync\":true"));

    let doctor = invoke(&["doctor", "--json"]);
    let output = String::from_utf8_lossy(&doctor.stdout);
    assert!(output.contains("\"issues\":"));
    assert!(output.contains("\"solution\":"));
    assert!(output.contains("\"autoFixable\":"));
    assert!(!output.contains("Fix which issue?"));
}

#[test]
fn doctor_json_suggests_init_for_unconfigured_repo() {
    let repo = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["doctor", "--json", "--repo"])
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("no .agents/ directory"));
    assert!(output.contains("\"autoFixable\":true"));

    let plain = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["doctor", "--repo"])
        .arg(repo.path())
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&plain.stdout);
    assert!(output.contains("Solution: Run rai init"));
    assert!(!output.contains("Fix which issue?"));
}

#[test]
fn status_reports_unowned_collision() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(repo.path().join(".agents/rules/general.md"), "Rule\n").unwrap();
    fs::write(repo.path().join("CLAUDE.md"), "Manual\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["status", "--json", "--repo"])
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("\"ok\":false"));
    assert!(String::from_utf8_lossy(&result.stdout).contains("unowned"));
}

#[cfg(unix)]
#[test]
fn setup_installs_isolated_hook_that_syncs_a_repo() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = tempfile::tempdir().unwrap();
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let rai = bin.join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &rai).unwrap();
    let launchctl = bin.join("launchctl");
    fs::write(&launchctl, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&launchctl, fs::Permissions::from_mode(0o755)).unwrap();

    let workspace = sandbox.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let config_global = sandbox.path().join("gitconfig");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let result = Command::new(&rai)
        .args(["setup", "--root"])
        .arg(&workspace)
        .arg("--perf")
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", sandbox.path().join("config"))
        .env("GIT_CONFIG_GLOBAL", &config_global)
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_time_taken(&result.stderr);

    let hook = sandbox
        .path()
        .join("config/rai/git-template/hooks/post-checkout");
    assert!(hook.exists());
    let saved = fs::read_to_string(sandbox.path().join("config/rai/roots.txt")).unwrap();
    assert!(saved.contains(&workspace.to_string_lossy().to_string()));

    let repo = workspace.join("example");
    fs::create_dir_all(repo.join(".agents/rules")).unwrap();
    fs::write(repo.join(".agents/rules/general.md"), "Hook rule.\n").unwrap();
    let status = Command::new(&hook).current_dir(&repo).status().unwrap();
    assert!(status.success());
    assert!(
        fs::read_to_string(repo.join("CLAUDE.md"))
            .unwrap()
            .contains("Hook rule.")
    );

    let origin = sandbox.path().join("origin");
    fs::create_dir_all(origin.join(".agents/rules")).unwrap();
    fs::write(origin.join(".agents/rules/general.md"), "Clone rule.\n").unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&origin)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["add", ".agents"])
            .current_dir(&origin)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "test",
            ])
            .current_dir(&origin)
            .status()
            .unwrap()
            .success()
    );
    let clone = workspace.join("clone");
    let result = Command::new("git")
        .arg("clone")
        .arg("-q")
        .arg(&origin)
        .arg(&clone)
        .env("HOME", sandbox.path())
        .env("GIT_CONFIG_GLOBAL", &config_global)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        fs::read_to_string(clone.join("CLAUDE.md"))
            .unwrap()
            .contains("Clone rule.")
    );
}
