use std::fs;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

#[cfg(unix)]
#[test]
fn update_installs_verified_release_archive() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let installed = dir.path().join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &installed).unwrap();
    let source = dir.path().join("source");
    let bin = dir.path().join("bin");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&bin).unwrap();
    let replacement = source.join("rai");
    fs::write(&replacement, b"updated rai fixture").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let archive = dir.path().join("archive.tar.gz");
    assert!(
        Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&source)
            .arg("rai")
            .status()
            .unwrap()
            .success()
    );
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        _ => return,
    };
    let hash = format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()));
    fs::write(
        dir.path().join("SHA256SUMS"),
        format!("{hash}  rai-v999.0.0-{target}.tar.gz\n"),
    )
    .unwrap();
    let fake_curl = bin.join("curl");
    fs::write(
        &fake_curl,
        "#!/bin/sh\ndestination=\nprevious=\nfor arg in \"$@\"; do\n  if [ \"$previous\" = --output ]; then destination=$arg; fi\n  previous=$arg\n  url=$arg\ndone\ncase \"$url\" in\n  */releases/latest) printf '{\"tag_name\":\"v999.0.0\"}' ;;\n  */SHA256SUMS) cp \"$RAI_TEST_RELEASE_DIR/SHA256SUMS\" \"$destination\" ;;\n  *.tar.gz) cp \"$RAI_TEST_RELEASE_DIR/archive.tar.gz\" \"$destination\" ;;\n  *) exit 1 ;;\nesac\n",
    )
    .unwrap();
    fs::set_permissions(&fake_curl, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let output = Command::new(&installed)
        .arg("update")
        .env("PATH", path)
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env("RAI_TEST_RELEASE_DIR", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(installed).unwrap(), b"updated rai fixture");
}

#[test]
fn update_warning_appears_after_command_without_changing_json() {
    let cache = tempfile::tempdir().unwrap();
    let cache_file = cache.path().join("rai/latest-release");
    fs::create_dir_all(cache_file.parent().unwrap()).unwrap();
    fs::write(cache_file, "v999.0.0").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["doctor", "--json", "--repo"])
        .arg(cache.path())
        .env("XDG_CACHE_HOME", cache.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(serde_json::from_slice::<serde_json::Value>(&result.stdout).is_ok());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr
            .lines()
            .last()
            .unwrap()
            .contains("Warning: rai v999.0.0 is available")
    );
}

fn invoke_codex_hook(repo: &std::path::Path) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--codex-hook", "--repo"])
        .arg(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"prompt":"test"}"#)
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn codex_hook_blocks_after_sync_then_allows_next_prompt() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Use the test convention.\n",
    )
    .unwrap();
    let first = invoke_codex_hook(repo.path());
    assert!(first.status.success());
    assert!(String::from_utf8_lossy(&first.stdout).contains("\"decision\":\"block\""));
    assert!(repo.path().join("AGENTS.md").exists());
    let second = invoke_codex_hook(repo.path());
    assert_eq!(
        String::from_utf8_lossy(&second.stdout).trim(),
        "{\"continue\":true}"
    );
}

#[test]
fn removed_cursor_hook_option_is_rejected() {
    let repo = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--cursor-hook", "--repo"])
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown option: --cursor-hook"));
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
    assert!(String::from_utf8_lossy(&preview.stdout).contains("Create AGENTS.md"));
    assert!(!repo.path().join("AGENTS.md").exists());

    let first = invoke(false);
    assert!(first.status.success());
    assert!(repo.path().join("AGENTS.md").exists());
    assert!(repo.path().join(".codex/config.toml").exists());

    let second = invoke(false);
    assert!(second.status.success());
    assert!(String::from_utf8_lossy(&second.stdout).contains("Unchanged AGENTS.md"));
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
    fs::write(repo.path().join("AGENTS.md"), "Manual\n").unwrap();
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
    let systemctl = bin.join("systemctl");
    fs::write(&systemctl, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755)).unwrap();

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
    #[cfg(target_os = "linux")]
    {
        let service =
            fs::read_to_string(sandbox.path().join("config/systemd/user/rai-watch.service"))
                .unwrap();
        assert!(service.starts_with("# rai-managed-watcher\n"));
        assert!(service.contains(&format!("ExecStart=\"{}\" watch", rai.display())));
    }

    let repo = workspace.join("example");
    fs::create_dir_all(repo.join(".agents/rules")).unwrap();
    fs::write(repo.join(".agents/rules/general.md"), "Hook rule.\n").unwrap();
    let status = Command::new(&hook).current_dir(&repo).status().unwrap();
    assert!(status.success());
    assert!(
        fs::read_to_string(repo.join("AGENTS.md"))
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
        fs::read_to_string(clone.join("AGENTS.md"))
            .unwrap()
            .contains("Clone rule.")
    );
}
