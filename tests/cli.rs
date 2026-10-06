use std::fs;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

#[test]
fn first_manual_sync_creates_empty_canonical_tree_but_git_hook_does_not() {
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
    assert!(invoke(&["sync", "--git-hook"]).status.success());
    assert!(!repo.path().join(".agents").exists());
    let sync = invoke(&["sync"]);
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    for directory in ["rules", "agents", "commands", "skills"] {
        assert!(
            repo.path()
                .join(".agents")
                .join(directory)
                .join(".keep")
                .exists()
        );
    }
    assert_eq!(
        fs::read_to_string(repo.path().join(".agents/mcp.yaml")).unwrap(),
        "servers: {}\n"
    );
    assert!(!repo.path().join("AGENTS.md").exists());
    assert!(!repo.path().join("CLAUDE.md").exists());
    assert!(!repo.path().join(".github/copilot-instructions.md").exists());
    assert!(!repo.path().join(".mcp.json").exists());
    assert!(!repo.path().join(".vscode/mcp.json").exists());
    assert!(invoke(&["sync", "--dry-run"]).status.success());
}

#[cfg(unix)]
#[test]
fn global_hook_syncs_a_fresh_clone_without_init() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = tempfile::tempdir().unwrap();
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let rai = bin.join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &rai).unwrap();
    for name in ["launchctl", "systemctl"] {
        let command = bin.join(name);
        fs::write(&command, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(command, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let source = sandbox.path().join("source");
    fs::create_dir_all(source.join(".agents/rules")).unwrap();
    fs::write(source.join(".agents/rules/general.md"), "Clone rules.\n").unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["add", ".agents"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-qm",
                "initial"
            ])
            .status()
            .unwrap()
            .success()
    );
    let global = sandbox.path().join("gitconfig");
    let config = sandbox.path().join("config");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let install = Command::new(&rai)
        .args(["install", "--root"])
        .arg(&source)
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let clone = sandbox.path().join("clone");
    let result = Command::new("git")
        .arg("clone")
        .arg(&source)
        .arg(&clone)
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("PATH", &path)
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
            .contains("Clone rules.")
    );
    let native_source = sandbox.path().join("native-source");
    fs::create_dir(&native_source).unwrap();
    fs::write(native_source.join("CLAUDE.md"), "Native clone guidance.\n").unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&native_source)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&native_source)
            .args(["add", "CLAUDE.md"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&native_source)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-qm",
                "initial"
            ])
            .status()
            .unwrap()
            .success()
    );
    let native_clone = sandbox.path().join("native-clone");
    let native_result = Command::new("git")
        .arg("clone")
        .arg(&native_source)
        .arg(&native_clone)
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        native_result.status.success(),
        "{}",
        String::from_utf8_lossy(&native_result.stderr)
    );
    assert!(!native_clone.join(".agents").exists());
    assert_eq!(
        fs::read_to_string(native_clone.join("CLAUDE.md")).unwrap(),
        "Native clone guidance.\n"
    );
    assert!(!String::from_utf8_lossy(&native_result.stderr).contains("Proceed with migration?"));
    let marker = sandbox.path().join("local-hook-ran");
    let local_hook = clone.join(".git/hooks/pre-commit");
    fs::write(
        &local_hook,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&local_hook, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(clone.join("note.txt"), "new file\n").unwrap();
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&clone)
            .args(["add", "note.txt"])
            .env("GIT_CONFIG_GLOBAL", &global)
            .status()
            .unwrap()
            .success()
    );
    let commit = Command::new("git")
        .arg("-C")
        .arg(&clone)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "more",
        ])
        .env("GIT_CONFIG_GLOBAL", &global)
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
    assert!(marker.exists());
}

#[cfg(unix)]
#[test]
fn global_only_install_restores_previous_hook_path_on_uninstall() {
    let sandbox = tempfile::tempdir().unwrap();
    let rai = sandbox.path().join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &rai).unwrap();
    let previous = sandbox.path().join("existing-hooks");
    fs::create_dir(&previous).unwrap();
    let global = sandbox.path().join("gitconfig");
    assert!(
        Command::new("git")
            .args(["config", "--global", "core.hooksPath"])
            .arg(&previous)
            .env("GIT_CONFIG_GLOBAL", &global)
            .status()
            .unwrap()
            .success()
    );
    let config = sandbox.path().join("config");
    let invoke = |command: &str| {
        Command::new(&rai)
            .arg(command)
            .env("HOME", sandbox.path())
            .env("XDG_CONFIG_HOME", &config)
            .env("GIT_CONFIG_GLOBAL", &global)
            .output()
            .unwrap()
    };
    let install = invoke("install");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(!config.join("rai/roots.txt").exists());
    assert!(config.join("rai/global-hooks/pre-commit").exists());
    assert!(invoke("uninstall").status.success());
    let configured = Command::new("git")
        .args(["config", "--global", "--get", "core.hooksPath"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&configured.stdout).trim(),
        previous.to_str().unwrap()
    );
    assert!(!config.join("rai/global-hooks").exists());
}

#[test]
fn help_and_noninteractive_default_show_english_guidance() {
    for args in [Vec::<&str>::new(), vec!["help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_rai"))
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("ROSETTAI"));
        assert!(stdout.contains("Set up and synchronize this project"));
        assert!(stdout.contains("rai sync"));
    }
}

#[test]
fn init_requires_migration_and_migrate_preserves_native_instructions() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(repo.path().join("AGENTS.md"), "Keep existing rules.\n").unwrap();
    fs::create_dir_all(repo.path().join(".github")).unwrap();
    fs::write(
        repo.path().join(".github/copilot-instructions.md"),
        "Use the same review process.\n",
    )
    .unwrap();
    let invoke = |command: &str, answer: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rai"))
            .args([command, "--repo"])
            .arg(repo.path())
            .env("HOME", home.path())
            .env("GIT_CONFIG_GLOBAL", home.path().join("gitconfig"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(answer.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let init = invoke("init", "n\n");
    assert!(init.status.success());
    assert!(String::from_utf8_lossy(&init.stdout).contains("Migration proposed"));
    assert!(!repo.path().join(".agents").exists());
    let migration = invoke("init", "y\n");
    assert!(
        migration.status.success(),
        "{}",
        String::from_utf8_lossy(&migration.stderr)
    );
    let rules = fs::read_to_string(repo.path().join(".agents/rules/migrated-harness.md")).unwrap();
    assert!(rules.contains("Keep existing rules."));
    assert!(rules.contains("Use the same review process."));
    for directory in ["rules", "agents", "commands", "skills"] {
        assert!(
            repo.path()
                .join(".agents")
                .join(directory)
                .join(".keep")
                .exists()
        );
    }
    assert_eq!(
        fs::read_to_string(repo.path().join(".agents/mcp.yaml")).unwrap(),
        "servers: {}\n"
    );
    let backups = fs::read_dir(
        home.path()
            .join(".rai/migrations")
            .join(repo.path().file_name().unwrap()),
    )
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(backups.len(), 1);
    let backup = backups[0].path();
    assert_eq!(
        fs::read_to_string(backup.join("AGENTS.md")).unwrap(),
        "Keep existing rules.\n"
    );
    assert!(!repo.path().join(".agents/migration-backup").exists());
    assert!(!repo.path().join("AGENTS.md").exists());
    let manifest = fs::read_to_string(backup.join("manifest.json")).unwrap();
    assert!(manifest.contains("AGENTS.md"));
    let rollback = invoke("rollback", "");
    assert!(
        rollback.status.success(),
        "{}",
        String::from_utf8_lossy(&rollback.stderr)
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("AGENTS.md")).unwrap(),
        "Keep existing rules.\n"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join(".github/copilot-instructions.md")).unwrap(),
        "Use the same review process.\n"
    );
    assert!(
        !repo
            .path()
            .join(".agents/rules/migrated-harness.md")
            .exists()
    );
}

#[test]
fn migrate_stops_before_writing_when_native_features_need_manual_conversion() {
    let repo = tempfile::tempdir().unwrap();
    fs::write(repo.path().join("AGENTS.md"), "Rules\n").unwrap();
    fs::create_dir_all(repo.path().join(".codex")).unwrap();
    fs::write(repo.path().join(".codex/config.toml"), "[mcp_servers]\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["migrate", "--repo"])
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(".codex/config.toml"));
    assert!(repo.path().join("AGENTS.md").exists());
    assert!(!repo.path().join(".agents").exists());
}

#[test]
fn rollback_refuses_to_replace_edited_migrated_rules() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(repo.path().join("AGENTS.md"), "Original rules.\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["init", "--repo"])
        .arg(repo.path())
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", home.path().join("gitconfig"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    let rules = repo.path().join(".agents/rules/migrated-harness.md");
    fs::write(&rules, "Edited after migration.\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["rollback", "--repo"])
        .arg(repo.path())
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", home.path().join("gitconfig"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("migrated rules changed"));
    assert!(!repo.path().join("AGENTS.md").exists());
    assert_eq!(
        fs::read_to_string(&rules).unwrap(),
        "Edited after migration.\n"
    );
}

#[test]
fn rollback_restores_native_file_after_sync() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(repo.path().join("AGENTS.md"), "Original rules.\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["init", "--repo"])
        .arg(repo.path())
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", home.path().join("gitconfig"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    let sync = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--repo"])
        .arg(repo.path())
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", home.path().join("gitconfig"))
        .output()
        .unwrap();
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let rollback = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["rollback", "--repo"])
        .arg(repo.path())
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", home.path().join("gitconfig"))
        .output()
        .unwrap();
    assert!(
        rollback.status.success(),
        "{}",
        String::from_utf8_lossy(&rollback.stderr)
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("AGENTS.md")).unwrap(),
        "Original rules.\n"
    );
}

#[test]
fn setup_requires_migration_before_registering_a_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let installed = temp.path().join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &installed).unwrap();
    let workspace = temp.path().join("workspace");
    let repo = workspace.join("project");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::write(repo.join("AGENTS.md"), "Existing project rules.\n").unwrap();
    let config = temp.path().join("config");
    let output = Command::new(installed)
        .args(["install", "--root"])
        .arg(&workspace)
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run `rai migrate"));
    assert!(!config.join("rai/roots.txt").exists());
}

#[test]
fn migrate_keeps_git_tracked_native_file_untouched() {
    let repo = tempfile::tempdir().unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .arg(repo.path())
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(repo.path().join("AGENTS.md"), "Tracked project rules.\n").unwrap();
    assert!(
        Command::new("git")
            .args(["-C"])
            .arg(repo.path())
            .args(["add", "AGENTS.md"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["migrate", "--repo"])
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Migration proposed"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Migration cancelled"));
    assert_eq!(
        fs::read_to_string(repo.path().join("AGENTS.md")).unwrap(),
        "Tracked project rules.\n"
    );
    assert!(!repo.path().join(".agents").exists());
}

#[cfg(unix)]
#[test]
fn first_init_sets_up_machine_once() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let installed = bin.join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &installed).unwrap();
    for name in ["launchctl", "systemctl"] {
        let command = bin.join(name);
        fs::write(&command, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(command, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let repo = temp.path().join("project");
    fs::create_dir(&repo).unwrap();
    let config = temp.path().join("config");
    let invoke = |command: &str| {
        Command::new(&installed)
            .args([command, "--repo"])
            .arg(&repo)
            .env("HOME", temp.path())
            .env("XDG_CONFIG_HOME", &config)
            .env("GIT_CONFIG_GLOBAL", temp.path().join("gitconfig"))
            .env("PATH", &path)
            .output()
            .unwrap()
    };
    let first = invoke("init");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(repo.join(".agents/rules/.keep").exists());
    assert!(repo.join(".agents/agents/.keep").exists());
    assert!(repo.join(".agents/skills/.keep").exists());
    assert_eq!(
        fs::read_to_string(repo.join(".agents/mcp.yaml")).unwrap(),
        "servers: {}\n"
    );
    assert!(!repo.join(".agents/rules/general.md").exists());
    assert_eq!(
        fs::read_to_string(config.join("rai/roots.txt"))
            .unwrap()
            .trim(),
        repo.canonicalize().unwrap().to_str().unwrap()
    );
    let second = invoke("init");
    assert!(!second.status.success());
    assert!(!String::from_utf8_lossy(&second.stdout).contains("Configuring rai"));
}

#[test]
fn help_works_without_a_project_and_without_ansi_when_piped() {
    let dir = tempfile::tempdir().unwrap();
    let mut expected = None;
    for args in [vec![], vec!["help"], vec!["--help"], vec!["-h"]] {
        let result = Command::new(env!("CARGO_BIN_EXE_rai"))
            .args(args)
            .current_dir(dir.path())
            .env("XDG_CACHE_HOME", dir.path())
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains("ROSETTAI"));
        assert!(text.contains("rai doctor"));
        assert!(text.contains("rai sync"));
        assert!(!text.contains("Options"));
        assert!(text.contains("rai install"));
        assert!(!text.contains('\u{1b}'));
        if let Some(expected) = &expected {
            assert_eq!(&text, expected);
        } else {
            expected = Some(text);
        }
    }
}

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
    assert!(repo.path().join(".agents/rules/.keep").exists());
    assert!(repo.path().join(".agents/agents/.keep").exists());
    assert!(repo.path().join(".agents/skills/.keep").exists());
    assert_eq!(
        fs::read_to_string(repo.path().join(".agents/mcp.yaml")).unwrap(),
        "servers: {}\n"
    );
    assert!(!repo.path().join(".agents/rules/general.md").exists());
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
    assert!(output.contains("Solution: Run rai sync"));
    assert!(output.contains("✖ 1. no .agents/ directory"));
    assert!(output.contains("Not checked until project issues are resolved"));
    assert!(!output.contains("Generated projections are up to date"));
    assert!(output.contains("Project"));
    assert!(output.contains("Installation"));
    assert!(output.contains("Errors:"));
    assert!(output.contains("Warnings:"));
    assert!(!output.contains('\u{1b}'));
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
        .args(["install", "--root"])
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
    let registry: serde_json::Value = serde_json::from_slice(
        &fs::read(sandbox.path().join("config/rai/installed-hooks.json")).unwrap(),
    )
    .unwrap();
    for event in ["post-checkout", "post-merge", "post-rewrite"] {
        assert!(
            registry
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["event"] == event)
        );
    }
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

#[cfg(unix)]
#[test]
fn install_migrates_legacy_unix_configuration() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = tempfile::tempdir().unwrap();
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let rai = bin.join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &rai).unwrap();
    for name in ["systemctl", "launchctl"] {
        let command = bin.join(name);
        fs::write(&command, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let workspace = sandbox.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let previous = sandbox.path().join(".config/rai");
    let hooks = previous.join("git-template/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(
        previous.join("roots.txt"),
        format!("{}\n", workspace.display()),
    )
    .unwrap();
    let old_hook = hooks.join("post-checkout");
    fs::write(&old_hook, "#!/bin/sh\n# rai-managed-hook\nexit 0\n").unwrap();
    fs::write(
        previous.join("installed-hooks.json"),
        serde_json::to_vec(&serde_json::json!([
            {"kind":"git-template","event":"post-checkout","path":old_hook}
        ]))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(sandbox.path().join(".rai/cache")).unwrap();
    let global = sandbox.path().join("gitconfig");
    assert!(
        Command::new("git")
            .args(["config", "--global", "init.templateDir"])
            .arg(previous.join("git-template"))
            .env("GIT_CONFIG_GLOBAL", &global)
            .status()
            .unwrap()
            .success()
    );
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let result = Command::new(&rai)
        .args(["install", "--root"])
        .arg(&workspace)
        .env("HOME", sandbox.path())
        .env_remove("XDG_CONFIG_HOME")
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("PATH", path)
        .env("XDG_CACHE_HOME", sandbox.path().join("cache"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!previous.exists());
    let current = sandbox.path().join(".rai");
    assert!(current.join("cache").exists());
    assert!(current.join("roots.txt").exists());
    let registry: serde_json::Value =
        serde_json::from_slice(&fs::read(current.join("installed-hooks.json")).unwrap()).unwrap();
    assert_eq!(registry.as_array().unwrap().len(), 3);
    assert!(registry.as_array().unwrap().iter().all(|entry| {
        entry["path"]
            .as_str()
            .unwrap()
            .starts_with(&current.to_string_lossy().to_string())
    }));
    let configured = Command::new("git")
        .args(["config", "--global", "--get", "init.templateDir"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&configured.stdout).trim(),
        current.join("git-template").to_string_lossy()
    );
}

#[cfg(unix)]
#[test]
fn uninstall_removes_service_and_template_only() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = tempfile::tempdir().unwrap();
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let rai = bin.join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &rai).unwrap();
    let systemctl = bin.join("systemctl");
    fs::write(&systemctl, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755)).unwrap();
    let launchctl = bin.join("launchctl");
    fs::write(&launchctl, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&launchctl, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let repo = sandbox.path().join("repo");
    fs::create_dir(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .unwrap()
            .success()
    );
    let global = sandbox.path().join("gitconfig");
    let setup = Command::new(&rai)
        .args(["install", "--root"])
        .arg(&repo)
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", sandbox.path().join("config"))
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );

    let owned = "# rai-generated sha256:fixture\ngenerated\n";
    fs::create_dir(repo.join(".codex")).unwrap();
    fs::write(repo.join(".codex/config.toml"), owned).unwrap();
    fs::write(
        repo.join(".codex/custom.toml"),
        "# rai-generated sha256:invalid\nmodified\n",
    )
    .unwrap();
    fs::write(repo.join(".gitignore"), "# RosettAI generated files\n/.codex/config.toml\n/.codex/custom.toml\n# End RosettAI generated files\n").unwrap();
    let hook = repo.join(".git/hooks/post-checkout");
    fs::write(&hook, "#!/bin/sh\n# rai-managed-hook\nexit 0\n").unwrap();

    let output = Command::new(&rai)
        .arg("uninstall")
        .current_dir(&repo)
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", sandbox.path().join("config"))
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !sandbox
            .path()
            .join("config/systemd/user/rai-watch.service")
            .exists()
    );
    assert!(
        !sandbox
            .path()
            .join("Library/LaunchAgents/ai.rosettai.rai.plist")
            .exists()
    );
    assert!(!sandbox.path().join("config/rai/git-template").exists());
    let registry: serde_json::Value = serde_json::from_slice(
        &fs::read(sandbox.path().join("config/rai/installed-hooks.json")).unwrap(),
    )
    .unwrap();
    assert!(registry.as_array().unwrap().is_empty());
    assert!(sandbox.path().join("config/rai/roots.txt").exists());
    assert!(hook.exists());
    assert_eq!(
        fs::read_to_string(repo.join(".codex/config.toml")).unwrap(),
        owned
    );
    assert!(repo.join(".codex/custom.toml").exists());
    let ignore = fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert!(ignore.contains("/.codex/custom.toml"));
    assert!(ignore.contains("/.codex/config.toml"));
    let configured = Command::new("git")
        .args(["config", "--global", "--get", "init.templateDir"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .output()
        .unwrap();
    assert!(!configured.status.success());
    let again = Command::new(&rai)
        .arg("uninstall")
        .current_dir(&repo)
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", sandbox.path().join("config"))
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(again.status.success());
}

#[cfg(unix)]
#[test]
fn doctor_recovers_moved_workspace_and_warns_when_it_disappears() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let rai = bin.join("rai");
    fs::copy(env!("CARGO_BIN_EXE_rai"), &rai).unwrap();
    for command in ["launchctl", "systemctl"] {
        let path = bin.join(command);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let old = dir.path().join("workspace");
    let moved = dir.path().join("workspace-moved");
    fs::create_dir(&old).unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir_all(repo.join(".agents/rules")).unwrap();
    fs::write(repo.join(".agents/rules/general.md"), "Rule\n").unwrap();
    let global = dir.path().join("gitconfig");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let run = |args: &[&str]| {
        Command::new(&rai)
            .args(args)
            .env("HOME", dir.path())
            .env("XDG_CONFIG_HOME", dir.path().join("config"))
            .env("GIT_CONFIG_GLOBAL", &global)
            .env("PATH", &path)
            .env("XDG_CACHE_HOME", dir.path().join("cache"))
            .output()
            .unwrap()
    };
    let install = run(&["install", "--root", old.to_str().unwrap()]);
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    fs::rename(&old, &moved).unwrap();
    let doctor = run(&["doctor", "--json", "--repo", repo.to_str().unwrap()]);
    assert!(!String::from_utf8_lossy(&doctor.stdout).contains("configured workspace is missing"));
    let roots = fs::read_to_string(dir.path().join("config/rai/roots.txt")).unwrap();
    assert!(roots.contains(&moved.to_string_lossy().to_string()));
    fs::remove_dir(&moved).unwrap();
    let doctor = run(&["doctor", "--json", "--repo", repo.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("configured workspace is missing"));
    let json: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let missing = json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| {
            issue["message"]
                .as_str()
                .unwrap_or("")
                .contains("configured workspace is missing")
        })
        .unwrap();
    assert_eq!(missing["autoFixable"], false);
}
