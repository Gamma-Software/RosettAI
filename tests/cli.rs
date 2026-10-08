use std::fs;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

#[test]
fn unknown_command_shows_only_public_commands_without_requiring_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rai"))
        .arg("unknown")
        .current_dir(dir.path())
        .env("PATH", "")
        .env("XDG_CACHE_HOME", dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command: unknown"));
    assert!(
        stderr.contains("usage: rai [install|sync|doctor|update|version|uninstall|help] [options]")
    );
    assert!(!stderr.contains("no .agents/"));
    let usage = stderr.lines().find(|line| line.contains("usage:")).unwrap();
    for legacy in ["init", "migrate", "rollback", "status", "watch"] {
        assert!(!usage.contains(legacy));
    }
}

#[test]
fn version_reports_build_metadata_without_a_project_or_runtime_git() {
    let dir = tempfile::tempdir().unwrap();
    let cache_file = dir.path().join("cache/rai/latest-release");
    fs::create_dir_all(cache_file.parent().unwrap()).unwrap();
    fs::write(cache_file, "v999.0.0").unwrap();
    let expected = format!(
        "rai {}\ncommit: {}{}\n",
        env!("CARGO_PKG_VERSION"),
        env!("RAI_BUILD_COMMIT"),
        if env!("RAI_BUILD_DIRTY") == "true" {
            " (dirty)"
        } else {
            ""
        }
    );
    for command in ["version", "--version", "-V"] {
        let result = Command::new(env!("CARGO_BIN_EXE_rai"))
            .arg(command)
            .current_dir(dir.path())
            .env("PATH", "")
            .env("RAI_BUILD_SHA", "runtime-value-must-not-change-the-commit")
            .env("XDG_CACHE_HOME", dir.path().join("cache"))
            .env("XDG_CONFIG_HOME", dir.path().join("config"))
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(String::from_utf8(result.stdout).unwrap(), expected);
        assert!(result.stderr.is_empty());
    }
    assert!(!dir.path().join(".agents").exists());
}

#[test]
fn version_rejects_project_options() {
    let result = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["version", "--repo", "/nonexistent"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("version accepts only --perf"));
}

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
    let config = home.path().join("config");
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
            .env("XDG_CONFIG_HOME", &config)
            .env("APPDATA", &config)
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
    #[cfg(windows)]
    let migration_base = config.join("rai/migrations");
    #[cfg(not(windows))]
    let migration_base = home.path().join(".rai/migrations");
    let backups = fs::read_dir(migration_base.join(repo.path().file_name().unwrap()))
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
            .env("XDG_CONFIG_HOME", dir.path().join("config"))
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
    assert!(String::from_utf8_lossy(&preview.stdout).contains("AGENTS.md — will create"));
    assert!(!repo.path().join("AGENTS.md").exists());

    let first = invoke(false);
    assert!(first.status.success());
    assert!(repo.path().join("AGENTS.md").exists());
    assert!(repo.path().join(".codex/config.toml").exists());

    let second = invoke(false);
    assert!(second.status.success());
    assert!(String::from_utf8_lossy(&second.stdout).contains("AGENTS.md — already synchronized"));
}

#[test]
fn sync_explains_owned_files_and_the_managed_gitignore_block() {
    let repo = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let git_config = config.path().join("gitconfig");
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    let rule = repo.path().join(".agents/rules/general.md");
    fs::write(&rule, "Shared instruction.\n").unwrap();
    fs::create_dir(repo.path().join("frontend")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/scoped.md"),
        "---\npath: frontend\n---\nFrontend instruction.\n",
    )
    .unwrap();
    for name in ["first", "second"] {
        let directory = repo.path().join(".agents/skills").join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test skill\n---\nShared skill instruction.\n"),
        )
        .unwrap();
    }
    fs::write(
        repo.path().join("user-note.txt"),
        "Keep this note outside the sync report.",
    )
    .unwrap();
    fs::write(repo.path().join(".gitignore"), "user-cache/\n").unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(repo.path())
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap()
            .success()
    );
    let invoke = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_rai"))
            .arg("sync")
            .args(args)
            .arg("--repo")
            .arg(repo.path())
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("XDG_CONFIG_HOME", config.path())
            .env("XDG_CACHE_HOME", config.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let preview = invoke(&["--dry-run"]);
    let groups = [
        "Codex",
        "Claude Code",
        "GitHub Copilot",
        "Project maintenance",
    ];
    let positions = groups.map(|group| preview.find(group).expect("agent group in report"));
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{preview}"
    );
    assert!(
        !preview.contains('├') && !preview.contains('└'),
        "{preview}"
    );
    assert!(preview.contains("+ .claude/skills/first/ — will create"));
    assert!(preview.contains("+ .claude/skills/second/ — will create"));
    let claude = &preview[positions[1]..positions[2]];
    assert!(claude.contains("CLAUDE.md — will create"), "{claude}");
    assert!(
        claude.contains(".claude/rules/frontend.md — will create"),
        "{claude}"
    );
    let codex = &preview[positions[0]..positions[1]];
    assert!(
        codex.contains("frontend/AGENTS.md — will create"),
        "{codex}"
    );
    assert!(
        codex.contains(".codex/config.toml — will create"),
        "{codex}"
    );
    let copilot = &preview[positions[2]..positions[3]];
    assert!(
        copilot.contains(".github/copilot-instructions.md — will create"),
        "{copilot}"
    );
    assert!(preview[positions[3]..].contains(".gitignore — will update ignore entries"));
    assert!(!preview.contains("SKILL.md"));
    assert!(!preview.contains("managed by rai"));
    assert!(!preview.contains("rai-managed"));
    assert!(preview.contains("frontend/AGENTS.md — will create"));
    assert!(!preview.contains("user-note.txt"));
    assert!(!repo.path().join("frontend/AGENTS.md").exists());
    assert!(preview.contains("AGENTS.md — will create"));
    assert!(preview.contains(".gitignore — will update ignore entries"));
    assert!(preview.contains("dry-run: no files written"));
    assert!(!repo.path().join("AGENTS.md").exists());
    assert_eq!(
        fs::read_to_string(repo.path().join(".gitignore")).unwrap(),
        "user-cache/\n"
    );
    let created = invoke(&[]);
    assert!(created.contains("AGENTS.md — created"));
    assert!(created.contains(".gitignore — ignore entries updated"));
    assert!(created.contains(".claude/skills/first/ — created"));
    assert!(!created.contains("SKILL.md"));
    assert!(!created.contains("will create"));
    assert!(!created.contains('\u{1b}'));
    let content = fs::read(repo.path().join("AGENTS.md")).unwrap();
    let repeated = invoke(&[]);
    assert!(repeated.contains("AGENTS.md — already synchronized"));
    assert!(repeated.contains(".gitignore — ignore entries already synchronized"));
    assert!(repeated.contains("Everything is synchronized. No files changed."));
    assert!(!repeated.contains("local ↔ expected:"));
    assert!(repeated.contains("✓ .claude/skills/first/ — already synchronized"));
    assert!(!repeated.contains("SKILL.md"));
    assert_eq!(fs::read(repo.path().join("AGENTS.md")).unwrap(), content);
    fs::write(&rule, "Updated shared instruction.\n").unwrap();
    let updated = invoke(&[]);
    assert!(updated.contains("AGENTS.md — updated from .agents/"));
    assert!(updated.contains("before sync · local ↔ expected:"));
    assert!(
        fs::read_to_string(repo.path().join("AGENTS.md"))
            .unwrap()
            .contains("Updated shared instruction.")
    );
    assert!(
        fs::read_to_string(repo.path().join(".gitignore"))
            .unwrap()
            .starts_with("user-cache/\n")
    );
    let skill_source = repo.path().join(".agents/skills/first/SKILL.md");
    let skill_text = fs::read_to_string(&skill_source).unwrap();
    fs::write(
        &skill_source,
        format!("{skill_text}New skill instruction.\n"),
    )
    .unwrap();
    let preview = invoke(&["--dry-run"]);
    assert!(preview.contains("↻ .claude/skills/first/ — will update from .agents/"));
    assert!(!preview.contains("SKILL.md"));
    assert!(!preview.contains("local ↔ expected:"));
    let updated = invoke(&[]);
    assert!(updated.contains("↻ .claude/skills/first/ — updated from .agents/"));
    assert!(!updated.contains("SKILL.md"));
    assert!(!updated.contains("local ↔ expected:"));

    let projection = repo.path().join(".claude/skills/first/SKILL.md");
    let edited = format!(
        "{}Local skill edit.\n",
        fs::read_to_string(&projection)
            .unwrap()
            .replace("rai-generated sha256:", "rai-generated sha256:invalid-")
    );
    fs::write(&projection, &edited).unwrap();
    let blocked = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--dry-run", "--repo"])
        .arg(repo.path())
        .env("GIT_CONFIG_GLOBAL", &git_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("XDG_CONFIG_HOME", config.path())
        .env("XDG_CACHE_HOME", config.path())
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let report = String::from_utf8_lossy(&blocked.stdout);
    assert!(!report.contains(".claude/skills/first/ —"), "{report}");
    assert!(
        report.contains(
            "⚠ .claude/skills/first/SKILL.md — modified locally · rai ownership check failed · not overwritten"
        ),
        "{report}"
    );
    assert!(report.contains("local ↔ expected:"), "{report}");
    assert!(
        report.contains(".agents/skills/ in the matching skill"),
        "{report}"
    );
    assert!(
        report.contains("✓ .claude/skills/second/ — already synchronized"),
        "{report}"
    );
    assert_eq!(report.matches("SKILL.md —").count(), 1, "{report}");
    assert_eq!(fs::read_to_string(&projection).unwrap(), edited);
}

#[test]
fn sync_reports_all_conflicts_and_healthy_files_without_partial_writes() {
    let repo = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let git_config = config.path().join("gitconfig");
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::create_dir(repo.path().join("frontend")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Global rule.\n",
    )
    .unwrap();
    let scoped = repo.path().join(".agents/rules/frontend.md");
    fs::write(&scoped, "---\npath: frontend\n---\nFrontend rule.\n").unwrap();
    let skill = repo.path().join(".agents/skills/obsolete/SKILL.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(
        &skill,
        "---\nname: obsolete\ndescription: Test skill\n---\nSkill instruction.\n",
    )
    .unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(repo.path())
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap()
            .success()
    );
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rai"))
            .arg("sync")
            .args(args)
            .arg("--repo")
            .arg(repo.path())
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("XDG_CONFIG_HOME", config.path())
            .env("XDG_CACHE_HOME", config.path())
            .output()
            .unwrap()
    };
    assert!(run(&[]).status.success());
    for relative in ["AGENTS.md", ".codex/config.toml"] {
        let path = repo.path().join(relative);
        fs::write(
            &path,
            format!(
                "{}\nLocal edit.\n",
                fs::read_to_string(&path)
                    .unwrap()
                    .split_once('\n')
                    .unwrap()
                    .1
            ),
        )
        .unwrap();
    }
    let blocked = run(&[]);
    assert!(!blocked.status.success());
    let report = String::from_utf8_lossy(&blocked.stdout);
    assert!(
        report.contains("✓ .gitignore — ignore entries already synchronized"),
        "{report}"
    );
    assert!(
        report.contains("2 warning(s); 0 to create, 0 to update, 0 to remove."),
        "{report}"
    );
    assert_eq!(
        report.matches("local diff: +2 / −0 lines").count(),
        2,
        "{report}"
    );
    assert_eq!(report.matches("local ↔ expected:").count(), 2, "{report}");
    assert!(!report.contains("100.0% similar"), "{report}");
    fs::write(
        &scoped,
        "---\npath: frontend\n---\nUpdated frontend rule.\n",
    )
    .unwrap();
    fs::remove_file(repo.path().join("frontend/AGENTS.md")).unwrap();
    fs::remove_dir_all(skill.parent().unwrap()).unwrap();
    let preserved = [
        "AGENTS.md",
        ".codex/config.toml",
        "CLAUDE.md",
        ".claude/rules/frontend.md",
        ".claude/skills/obsolete/SKILL.md",
        ".gitignore",
    ]
    .map(|relative| (relative, fs::read(repo.path().join(relative)).unwrap()));
    for args in [&[][..], &["--dry-run"][..], &["--git-hook"][..]] {
        let output = run(args);
        assert!(!output.status.success());
        let report = String::from_utf8(output.stdout).unwrap();
        assert!(
            report.contains("Synchronization blocked"),
            "{report}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            report.contains("⚠ AGENTS.md — rai-managed path"),
            "{report}"
        );
        assert!(
            report.contains("⚠ .codex/config.toml — rai-managed path"),
            "{report}"
        );
        assert!(
            report.contains("✓ CLAUDE.md — already synchronized"),
            "{report}"
        );
        assert!(
            report.contains("+ frontend/AGENTS.md — will create"),
            "{report}"
        );
        assert!(
            report.contains(".claude/rules/frontend.md — will update from .agents/"),
            "{report}"
        );
        assert!(
            report.contains(".claude/skills/obsolete/ — obsolete · will remove"),
            "{report}"
        );
        assert!(report.contains("2 warning(s)"), "{report}");
        assert!(
            report.contains("Sync blocked. No files changed."),
            "{report}"
        );
        assert!(!report.contains("Migration"), "{report}");
        assert!(!report.contains("Synchronization complete"), "{report}");
        assert!(
            report.contains(
                "Keep local edits: copy the changes you want into .agents/rules/ (global rules)"
            ),
            "{report}"
        );
        assert!(
            report.contains("MCP servers belong in .agents/mcp.yaml"),
            "{report}"
        );
        assert!(
            report.contains(
                "Use the canonical version: move 'AGENTS.md' to a backup outside this repository"
            ),
            "{report}"
        );
        assert!(
            report.contains("After resolving the warnings, preview: rai sync --repo"),
            "{report}"
        );
        assert!(
            report.contains("Then synchronize: rai sync --repo"),
            "{report}"
        );
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("For guided diagnosis and available fixes: rai doctor --repo"),
            "{error}"
        );
        assert!(error.contains("output conflict: AGENTS.md"), "{error}");
        assert!(
            error.contains("output conflict: .codex/config.toml"),
            "{error}"
        );
    }
    let output = run(&["--json"]);
    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert!(json["error"].as_str().unwrap().contains("AGENTS.md"));
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains(".codex/config.toml")
    );
    for (relative, content) in &preserved {
        assert_eq!(
            fs::read(repo.path().join(relative)).unwrap(),
            *content,
            "{relative}"
        );
    }
    assert!(!repo.path().join("frontend/AGENTS.md").exists());
    // Even a removed ownership marker must not turn an edited projection into an import.
    fs::write(repo.path().join("AGENTS.md"), "Local replacement.\n").unwrap();
    let output = run(&[]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("AGENTS.md — rai-managed path · file or ownership marker changed")
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("AGENTS.md")).unwrap(),
        "Local replacement.\n"
    );
    let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    fs::write(
        repo.path().join(".gitignore"),
        ignore.replace("# End RosettAI generated files", "# Removed end marker"),
    )
    .unwrap();
    let output = run(&["--dry-run"]);
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("⚠ .gitignore — invalid rai-managed ignore block"),
        "{report}"
    );
    assert!(
        report.contains("✓ CLAUDE.md — already synchronized"),
        "{report}"
    );
    assert!(
        report.contains("Sync blocked. No files changed."),
        "{report}"
    );
    assert!(
        report.contains(
            "repair the '# RosettAI generated files' and '# End RosettAI generated files' markers"
        ),
        "{report}"
    );
    assert!(
        report.contains("Preserve all ignore rules outside that block."),
        "{report}"
    );
}

struct SyncDisplayFixture {
    repo: tempfile::TempDir,
    sandbox: tempfile::TempDir,
}

impl SyncDisplayFixture {
    fn new() -> Self {
        let fixture = Self {
            repo: tempfile::tempdir().unwrap(),
            sandbox: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(fixture.repo.path().join(".agents/rules")).unwrap();
        fs::create_dir(fixture.repo.path().join("frontend")).unwrap();
        fs::write(
            fixture.repo.path().join(".agents/rules/general.md"),
            "Shared instruction.\n",
        )
        .unwrap();
        fs::write(
            fixture.repo.path().join(".agents/rules/frontend.md"),
            "---\npath: frontend\n---\nFrontend instruction.\n",
        )
        .unwrap();
        fs::write(fixture.repo.path().join(".agents/rules/.keep"), "").unwrap();
        fs::create_dir_all(fixture.sandbox.path().join("home")).unwrap();
        fs::create_dir_all(fixture.sandbox.path().join("cache/rai")).unwrap();
        fs::write(
            fixture.sandbox.path().join("cache/rai/latest-release"),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
        assert!(fixture.git(&["init", "-q"]).status.success());
        fixture
    }

    fn isolated_command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(self.repo.path())
            .env("HOME", self.sandbox.path().join("home"))
            .env("XDG_CONFIG_HOME", self.sandbox.path().join("config"))
            .env("APPDATA", self.sandbox.path().join("config"))
            .env("XDG_CACHE_HOME", self.sandbox.path().join("cache"))
            .env("GIT_CONFIG_GLOBAL", self.sandbox.path().join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1");
        command
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        self.isolated_command(env!("CARGO_BIN_EXE_rai"))
            .args(args)
            .output()
            .unwrap()
    }

    fn git(&self, args: &[&str]) -> std::process::Output {
        self.isolated_command("git").args(args).output().unwrap()
    }

    fn files(&self) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
        fn collect(
            root: &std::path::Path,
            directory: &std::path::Path,
            files: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
        ) {
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path == root.join(".git") {
                    continue;
                }
                if entry.file_type().unwrap().is_dir() {
                    collect(root, &path, files);
                } else {
                    files.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut files = std::collections::BTreeMap::new();
        collect(self.repo.path(), self.repo.path(), &mut files);
        files
    }
}

#[test]
fn sync_compact_lists_changes_and_counts_unchanged_files_without_agent_groups() {
    let fixture = SyncDisplayFixture::new();
    let initial = fixture.run(&["sync"]);
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let preview = fixture.run(&["sync", "--dry-run", "--json"]);
    assert!(preview.status.success());
    let state: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let unchanged = state["changes"].as_array().unwrap().len();
    assert!(unchanged > 0);

    let noop = fixture.run(&["sync", "--compact"]);
    assert!(
        noop.status.success(),
        "{}",
        String::from_utf8_lossy(&noop.stderr)
    );
    let report = String::from_utf8(noop.stdout).unwrap();
    assert!(report.contains("No files changed."), "{report}");
    assert!(
        report.contains(&format!("{unchanged} already synchronized")),
        "{report}"
    );
    assert!(!report.contains(" — already synchronized"), "{report}");
    assert!(!report.contains(".codex/config.toml"), "{report}");
    assert!(
        !report.contains("Claude Code") && !report.contains("GitHub Copilot"),
        "{report}"
    );

    fs::write(
        fixture.repo.path().join(".agents/rules/general.md"),
        "Updated shared instruction.\n",
    )
    .unwrap();
    let preview = fixture.run(&["sync", "--compact", "--dry-run"]);
    assert!(preview.status.success());
    let report = String::from_utf8(preview.stdout).unwrap();
    assert!(
        report.contains("AGENTS.md — will update from .agents/"),
        "{report}"
    );
    assert!(
        report.contains("CLAUDE.md — will update from .agents/"),
        "{report}"
    );
    assert!(
        report.contains(".github/copilot-instructions.md — will update from .agents/"),
        "{report}"
    );
    assert!(!report.contains(".codex/config.toml"), "{report}");
    assert!(!report.contains(" — already synchronized"), "{report}");
    assert!(report.contains("already synchronized"), "{report}");
}

#[test]
fn sync_compact_dry_run_preserves_sources_outputs_and_gitignore() {
    let fixture = SyncDisplayFixture::new();
    let before = fixture.files();
    let preview = fixture.run(&["sync", "--compact", "--dry-run"]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let report = String::from_utf8(preview.stdout).unwrap();
    assert!(report.contains("Sync preview"), "{report}");
    assert!(report.contains("AGENTS.md — will create"), "{report}");
    assert!(report.contains(".agents/rules/.keep"), "{report}");
    assert!(report.contains("will remove placeholder"), "{report}");
    assert!(report.contains("dry-run: no files written"), "{report}");
    assert!(!report.contains("Synchronization complete"), "{report}");
    assert_eq!(fixture.files(), before);
    assert!(!fixture.repo.path().join(".gitignore").exists());
    assert!(!fixture.repo.path().join("AGENTS.md").exists());
}

#[test]
fn sync_compact_tracked_conflict_keeps_diagnostic_solutions_and_blocks_all_changes() {
    let fixture = SyncDisplayFixture::new();
    assert!(fixture.run(&["sync"]).status.success());
    assert!(
        fixture
            .git(&["add", "-f", "--", ".codex/config.toml"])
            .status
            .success()
    );
    fs::write(
        fixture.repo.path().join(".agents/rules/general.md"),
        "Pending shared instruction.\n",
    )
    .unwrap();
    let before = fixture.files();
    let index = fs::read(fixture.repo.path().join(".git/index")).unwrap();
    for args in [
        ["sync", "--compact"].as_slice(),
        ["sync", "--compact", "--dry-run"].as_slice(),
    ] {
        let blocked = fixture.run(args);
        assert!(!blocked.status.success());
        let report = String::from_utf8(blocked.stdout).unwrap();
        assert!(report.contains("Synchronization blocked"), "{report}");
        assert!(
            report.contains(".codex/config.toml — tracked by Git"),
            "{report}"
        );
        assert!(
            report.contains("keeps the local file and stages its removal from Git"),
            "{report}"
        );
        assert!(
            report.contains("rm --cached -- '.codex/config.toml'"),
            "{report}"
        );
        assert!(
            report.contains("AGENTS.md — will update from .agents/"),
            "{report}"
        );
        assert!(
            report.contains("Sync blocked. No files changed."),
            "{report}"
        );
        assert!(!report.contains(" — already synchronized"), "{report}");
        assert!(
            report.contains("already synchronized") || report.contains(" synchronized,"),
            "{report}"
        );
        assert!(
            report.contains("After resolving the warnings, preview: rai sync"),
            "{report}"
        );
        assert!(report.contains("Then synchronize: rai sync"), "{report}");
        assert_eq!(fixture.files(), before);
        assert_eq!(
            fs::read(fixture.repo.path().join(".git/index")).unwrap(),
            index
        );
    }
}

#[test]
fn sync_compact_json_preserves_structured_changes_and_failure_results() {
    let fixture = SyncDisplayFixture::new();
    let before = fixture.files();
    let full = fixture.run(&["sync", "--dry-run", "--json"]);
    let compact = fixture.run(&["sync", "--dry-run", "--json", "--compact"]);
    assert!(full.status.success());
    assert!(
        compact.status.success(),
        "{}",
        String::from_utf8_lossy(&compact.stderr)
    );
    let full: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    let compact: serde_json::Value = serde_json::from_slice(&compact.stdout).unwrap();
    assert_eq!(compact, full);
    assert_eq!(fixture.files(), before);
    let synced = fixture.run(&["sync", "--json", "--compact"]);
    assert!(synced.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&synced.stdout).unwrap()["ok"],
        true
    );
    assert!(
        fixture
            .git(&["add", "-f", "--", ".codex/config.toml"])
            .status
            .success()
    );
    let before = fixture.files();
    let blocked = fixture.run(&["sync", "--json", "--compact"]);
    assert!(!blocked.status.success());
    let blocked: serde_json::Value = serde_json::from_slice(&blocked.stdout).unwrap();
    assert_eq!(blocked["ok"], false);
    assert!(
        blocked["error"]
            .as_str()
            .unwrap()
            .contains("tracked output conflict: .codex/config.toml")
    );
    assert_eq!(fixture.files(), before);
}

#[test]
fn compact_option_is_reserved_for_sync() {
    let fixture = SyncDisplayFixture::new();
    let before = fixture.files();
    for command in [
        "status",
        "doctor",
        "init",
        "migrate",
        "rollback",
        "install",
        "uninstall",
        "update",
        "version",
        "help",
    ] {
        let output = fixture.run(&[command, "--compact"]);
        assert!(!output.status.success(), "{command} accepted --compact");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if matches!(command, "version" | "help") {
            assert!(
                stderr.contains("accepts only --perf"),
                "{command}: {stderr}"
            );
        } else {
            assert!(stderr.contains("--compact"), "{command}: {stderr}");
        }
        assert_eq!(fixture.files(), before, "{command} changed project files");
    }
}

#[test]
fn sync_guides_scoped_edits_tracked_files_and_invalid_sources() {
    let repo = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let git_config = config.path().join("gitconfig");
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::create_dir(repo.path().join("frontend")).unwrap();
    let rule = repo.path().join(".agents/rules/frontend.md");
    fs::write(&rule, "---\npath: frontend\n---\nScoped instruction.\n").unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(repo.path())
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap()
            .success()
    );
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rai"))
            .arg("sync")
            .args(args)
            .arg("--repo")
            .arg(repo.path())
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("XDG_CONFIG_HOME", config.path())
            .env("XDG_CACHE_HOME", config.path())
            .output()
            .unwrap()
    };
    assert!(run(&["--json"]).status.success());
    let generated = repo.path().join("frontend/AGENTS.md");
    let original = fs::read_to_string(&generated).unwrap();
    let edited = format!("{}Local addition.\n", original.split_once('\n').unwrap().1);
    fs::write(&generated, &edited).unwrap();
    let output = run(&["--dry-run"]);
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains(".agents/rules/ with path: frontend"),
        "{report}"
    );
    assert_eq!(fs::read_to_string(&generated).unwrap(), edited);
    fs::write(&generated, &original).unwrap();
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["add", "-f", "--", "frontend/AGENTS.md"])
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap()
            .success()
    );
    let output = run(&["--dry-run"]);
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("rm --cached -- 'frontend/AGENTS.md'"),
        "{report}"
    );
    assert!(report.contains("keeps the local file"), "{report}");
    assert!(!report.contains("local ↔ expected:"), "{report}");
    assert_eq!(fs::read_to_string(&generated).unwrap(), original);
    let tracked = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["ls-files", "--error-unmatch", "--", "frontend/AGENTS.md"])
        .env("GIT_CONFIG_GLOBAL", &git_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(tracked.status.success());
    fs::write(
        &rule,
        "---\nname: invalid.md\n---\nKeep this invalid source.\n",
    )
    .unwrap();
    let output = run(&[]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("Solution: Set the rule's name to its original instruction filename"),
        "{error}"
    );
    assert!(error.contains("rai doctor --repo"), "{error}");
    assert_eq!(fs::read_to_string(&generated).unwrap(), original);
    #[cfg(unix)]
    {
        fs::write(&rule, "---\npath: frontend\n---\nScoped instruction.\n").unwrap();
        let outside = config.path().join("outside.toml");
        fs::write(&outside, "Keep this link target.\n").unwrap();
        let projection = repo.path().join(".codex/config.toml");
        fs::remove_file(&projection).unwrap();
        std::os::unix::fs::symlink(&outside, &projection).unwrap();
        let output = run(&["--dry-run"]);
        assert!(!output.status.success());
        let report = String::from_utf8_lossy(&output.stdout);
        assert!(
            report.contains("Solution: inspect the target of '.codex/config.toml'"),
            "{report}"
        );
        assert!(
            report.contains("preserving the target contents"),
            "{report}"
        );
        assert_eq!(
            fs::read_to_string(&outside).unwrap(),
            "Keep this link target.\n"
        );
        assert!(projection.is_symlink());
    }
}

#[test]
fn sync_resynchronizes_edited_projections_after_verified_external_backups() {
    let repo = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::create_dir(repo.path().join("frontend")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Global rule.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".agents/rules/front.md"),
        "---\npath: frontend\n---\nFrontend rule.\n",
    )
    .unwrap();
    let run = |options: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rai"))
            .arg("sync")
            .args(options)
            .arg("--repo")
            .arg(repo.path())
            .env("XDG_CONFIG_HOME", config.path())
            .env("APPDATA", config.path())
            .env("XDG_CACHE_HOME", config.path())
            .output()
            .unwrap()
    };
    assert!(run(&["--json"]).status.success());
    let paths = [
        "AGENTS.md",
        "frontend/AGENTS.md",
        ".codex/config.toml",
        ".github/copilot-instructions.md",
        ".claude/settings.json",
    ];
    let originals: Vec<_> = paths
        .iter()
        .map(|path| fs::read_to_string(repo.path().join(path)).unwrap())
        .collect();
    let edits: Vec<_> = paths
        .iter()
        .zip(&originals)
        .map(|(path, original)| {
            let edited = if path.ends_with(".json") {
                let mut json: serde_json::Value = serde_json::from_str(original).unwrap();
                json["local_edit"] = true.into();
                serde_json::to_string_pretty(&json).unwrap()
            } else {
                format!("{original}Local edit.\n")
            };
            fs::write(repo.path().join(path), &edited).unwrap();
            edited
        })
        .collect();
    let preview = run(&["--dry-run", "--json"]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let backups: Vec<_> = paths
        .iter()
        .map(|path| {
            // JSON keeps native path separators, including backslashes on Windows.
            let change = json["changes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|change| {
                    change["path"].as_str().is_some_and(|reported| {
                        std::path::Path::new(reported) == std::path::Path::new(path)
                    })
                })
                .unwrap_or_else(|| panic!("missing update for {path}: {json}"));
            assert_eq!(change["action"], "update");
            std::path::PathBuf::from(change["backup"].as_str().unwrap())
        })
        .collect();
    for ((path, edited), backup) in paths.iter().zip(&edits).zip(&backups) {
        assert_eq!(fs::read_to_string(repo.path().join(path)).unwrap(), *edited);
        assert!(!backup.exists());
        assert!(!backup.starts_with(repo.path()));
    }
    let preview = run(&["--dry-run"]);
    let report = String::from_utf8_lossy(&preview.stdout);
    assert!(preview.status.success());
    assert!(
        report.contains("will back up and resynchronize"),
        "{report}"
    );
    assert!(report.contains("Author unknown"), "{report}");
    // A backup failure must prevent all repository writes, including healthy outputs.
    fs::create_dir_all(backups[0].parent().unwrap()).unwrap();
    fs::write(&backups[0], "different saved content").unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Changed global rule.\n",
    )
    .unwrap();
    let healthy = fs::read(repo.path().join("CLAUDE.md")).unwrap();
    let failure = run(&["--json"]);
    assert!(!failure.status.success());
    let json: serde_json::Value = serde_json::from_slice(&failure.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert!(json["error"].as_str().unwrap().contains("backup conflict"));
    assert_eq!(fs::read(repo.path().join("CLAUDE.md")).unwrap(), healthy);
    for (path, edited) in paths.iter().zip(&edits) {
        assert_eq!(fs::read_to_string(repo.path().join(path)).unwrap(), *edited);
    }
    fs::remove_file(&backups[0]).unwrap();
    let applied = run(&[]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let report = String::from_utf8_lossy(&applied.stdout);
    assert!(report.contains("Synchronization complete"), "{report}");
    assert_eq!(report.matches("Backup saved:").count(), paths.len());
    assert!(report.contains("before sync · local ↔ expected:"));
    for ((path, edited), backup) in paths.iter().zip(&edits).zip(&backups) {
        assert_eq!(fs::read_to_string(backup).unwrap(), *edited);
        assert!(
            !fs::read_to_string(repo.path().join(path))
                .unwrap()
                .contains("Local edit.")
        );
    }
    assert_eq!(
        fs::read_to_string(repo.path().join("frontend/AGENTS.md")).unwrap(),
        originals[1]
    );
    assert!(
        !fs::read_to_string(repo.path().join("AGENTS.md"))
            .unwrap()
            .contains("Frontend rule.")
    );
    let again = run(&["--git-hook"]);
    assert!(again.status.success());
    let report = String::from_utf8_lossy(&again.stdout);
    assert!(report.contains("Everything is synchronized"));
    assert!(!report.contains("Backup saved:"));
}

#[test]
fn projection_hooks_block_direct_edits_and_allow_canonical_edits() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::create_dir(repo.path().join("frontend")).unwrap();
    fs::write(repo.path().join(".agents/rules/general.md"), "Global.\n").unwrap();
    fs::write(
        repo.path().join(".agents/rules/front.md"),
        "---\npath: frontend\n---\nFront.\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--repo"])
        .arg(repo.path())
        .env("XDG_CACHE_HOME", repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let config: toml::Value = fs::read_to_string(repo.path().join(".codex/config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        config["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str(),
        Some("rai guard")
    );
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(repo.path().join(".claude/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        config["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "rai guard"
    );
    let original = fs::read(repo.path().join("AGENTS.md")).unwrap();
    let guard = |tool: &str, input: serde_json::Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rai"))
            .arg("guard")
            .env("XDG_CONFIG_HOME", repo.path().join("config"))
            .env("PATH", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let event = serde_json::json!({"hook_event_name": "PreToolUse", "cwd": repo.path(), "tool_name": tool, "tool_input": input});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(event.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    for (tool, input) in [
        (
            "Write",
            serde_json::json!({"file_path": "AGENTS.md", "content": "edit"}),
        ),
        (
            "Edit",
            serde_json::json!({"file_path": "frontend/../AGENTS.md"}),
        ),
        (
            "MultiEdit",
            serde_json::json!({"file_path": "frontend/AGENTS.md"}),
        ),
        (
            "apply_patch",
            serde_json::json!({"input": "*** Begin Patch\n*** Delete File: AGENTS.md\n*** End Patch"}),
        ),
        (
            "apply_patch",
            serde_json::json!(
                "*** Begin Patch\n*** Update File: main.rs\n*** Move to: AGENTS.md\n*** End Patch"
            ),
        ),
    ] {
        let decision = guard(tool, input);
        assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(
            decision["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap()
                .contains(".agents/rules/")
        );
    }
    for path in [
        ".agents/rules/general.md",
        ".agents/agents/reviewer.md",
        "src/main.rs",
        "unmanaged/AGENTS.md",
    ] {
        assert_eq!(
            guard("Write", serde_json::json!({"file_path": path})),
            serde_json::json!({})
        );
    }
    // Shell commands aren't treated as file edits: this is a tool guard, not a sandbox.
    assert_eq!(
        guard("Bash", serde_json::json!({"command": "cat AGENTS.md"})),
        serde_json::json!({})
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(repo.path().join("AGENTS.md"), repo.path().join("alias.md"))
            .unwrap();
        assert_eq!(
            guard("Write", serde_json::json!({"file_path": "alias.md"}))["hookSpecificOutput"]["permissionDecision"],
            "deny"
        );
    }
    assert_eq!(fs::read(repo.path().join("AGENTS.md")).unwrap(), original);
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

#[test]
fn mistyped_command_suggests_without_running_when_noninteractive() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["syn", "--repo"])
        .arg(dir.path())
        .env("PATH", "")
        .env("XDG_CACHE_HOME", dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Did you mean `rai sync`?"));
    assert!(!stderr.contains("[y/N]"));
    assert!(output.stdout.is_empty());
    assert!(!dir.path().join(".agents").exists());
}

fn command_transcripts(
    root: &std::path::Path,
) -> Vec<(std::path::PathBuf, Vec<serde_json::Value>)> {
    let mut logs = Vec::new();
    if !root.is_dir() {
        return logs;
    }
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            logs.extend(command_transcripts(&path));
        } else if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            let contents = fs::read_to_string(&path).unwrap();
            let events = contents
                .rsplit_once('\n')
                .map_or("", |(complete, _)| complete)
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            logs.push((path, events));
        }
    }
    logs
}

fn transcript_output(events: &[serde_json::Value], stream: &str) -> String {
    events
        .iter()
        .filter(|event| event["event"] == "output" && event["stream"] == stream)
        .map(|event| event["text"].as_str().unwrap())
        .collect()
}

#[test]
fn command_logs_preserve_results_and_separate_projects_with_identical_names() {
    let sandbox = tempfile::tempdir().unwrap();
    let config = sandbox.path().join("config");
    let mut directories = std::collections::HashSet::new();
    for parent in ["one", "two"] {
        let repo = sandbox.path().join(parent).join("project");
        fs::create_dir_all(repo.join(".agents/rules")).unwrap();
        fs::write(repo.join(".agents/rules/general.md"), "Shared rule.\n").unwrap();
        let before = command_transcripts(&config.join("rai/logs")).len();
        for args in [
            vec!["sync", "--json", "--perf"],
            vec!["sync", "--json"],
            vec!["status", "--invalid"],
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_rai"))
                .args(&args)
                .arg("--repo")
                .arg(&repo)
                .env("HOME", sandbox.path())
                .env("XDG_CONFIG_HOME", &config)
                .env("XDG_CACHE_HOME", sandbox.path().join("cache"))
                .env("APPDATA", &config)
                .output()
                .unwrap();
            let logs_root = config.join("rai/logs");
            let logs = command_transcripts(&logs_root);
            let (_, events) = logs
                .iter()
                .find(|(_, events)| {
                    events[0]["args"][0] == args[0]
                        && events[0]["args"][1] == args[1]
                        && events[0]["project"]
                            == repo.canonicalize().unwrap().to_string_lossy().as_ref()
                        && (args.len() == 3 || events[0]["args"].as_array().unwrap().len() == 4)
                })
                .unwrap();
            assert_eq!(
                transcript_output(events, "stdout").as_bytes(),
                output.stdout
            );
            assert_eq!(
                transcript_output(events, "stderr").as_bytes(),
                output.stderr
            );
            assert_eq!(events[0]["event"], "start");
            assert_eq!(events.last().unwrap()["event"], "finish");
            assert_eq!(
                events.last().unwrap()["exitCode"],
                output.status.code().unwrap()
            );
            assert!(events.last().unwrap()["durationMs"].as_f64().unwrap() >= 0.0);
            if !output.status.success() {
                assert!(
                    events.last().unwrap()["error"]
                        .as_str()
                        .unwrap()
                        .contains("unknown option"),
                    "{}",
                    events.last().unwrap()["error"]
                );
            }
        }
        let logs = command_transcripts(&config.join("rai/logs"));
        assert_eq!(logs.len(), before + 3);
        let directory = logs
            .iter()
            .find(|(_, events)| {
                events[0]["project"] == repo.canonicalize().unwrap().to_string_lossy().as_ref()
            })
            .unwrap()
            .0
            .parent()
            .unwrap()
            .to_path_buf();
        directories.insert(directory);
        assert!(!repo.join("logs").exists());
        assert_eq!(
            fs::read_to_string(repo.join(".agents/rules/general.md")).unwrap(),
            "Shared rule.\n"
        );
    }
    assert_eq!(directories.len(), 2);
}

#[test]
fn global_command_logs_survive_concurrent_invocations() {
    let sandbox = tempfile::tempdir().unwrap();
    let mut children = Vec::new();
    for _ in 0..8 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_rai"))
                .arg("version")
                .env("HOME", sandbox.path())
                .env("APPDATA", sandbox.path())
                .env_remove("XDG_CONFIG_HOME")
                .env("PATH", "")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
    }
    let root = if cfg!(windows) {
        sandbox.path().join("rai/logs/global")
    } else {
        sandbox.path().join(".rai/logs/global")
    };
    let logs = command_transcripts(&root);
    assert_eq!(logs.len(), 8);
    for (_, events) in &logs {
        assert_eq!(events[0]["args"], serde_json::json!(["version"]));
        assert!(events[0]["project"].is_null());
        assert!(transcript_output(events, "stdout").starts_with("rai "));
        assert_eq!(events.last().unwrap()["exitCode"], 0);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for (path, _) in &logs {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn command_logging_failure_does_not_change_json_or_command_success() {
    let sandbox = tempfile::tempdir().unwrap();
    let repo = sandbox.path().join("repo");
    fs::create_dir_all(repo.join(".agents/rules")).unwrap();
    let config = sandbox.path().join("config");
    fs::create_dir_all(config.join("rai")).unwrap();
    fs::write(config.join("rai/logs"), "Keep this file.\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["status", "--json", "--repo"])
        .arg(&repo)
        .env("XDG_CONFIG_HOME", &config)
        .env("APPDATA", &config)
        .env("HOME", sandbox.path())
        .env("XDG_CACHE_HOME", sandbox.path().join("cache"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["ok"],
        true
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot create command log"));
    assert_eq!(
        fs::read_to_string(config.join("rai/logs")).unwrap(),
        "Keep this file.\n"
    );
}

#[test]
fn watcher_logs_each_discovered_project_and_failure_separately() {
    let sandbox = tempfile::tempdir().unwrap();
    let workspace = sandbox.path().join("workspace");
    let good = workspace.join("good");
    let bad = workspace.join("bad");
    for repo in [&good, &bad] {
        fs::create_dir_all(repo.join(".agents/rules")).unwrap();
    }
    fs::write(good.join(".agents/rules/general.md"), "Watch rule.\n").unwrap();
    fs::write(
        bad.join(".agents/rules/general.md"),
        "---\nname: invalid\n---\nRule.\n",
    )
    .unwrap();
    let config = sandbox.path().join("config");
    fs::create_dir_all(config.join("rai")).unwrap();
    fs::write(
        config.join("rai/roots.txt"),
        format!("{}\n", workspace.display()),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rai"))
        .arg("watch")
        .env("HOME", sandbox.path())
        .env("XDG_CONFIG_HOME", &config)
        .env("APPDATA", &config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    let complete = loop {
        let logs = command_transcripts(&config.join("rai/logs/projects"));
        if logs.len() == 2
            && logs.iter().all(|(_, events)| {
                events
                    .last()
                    .is_some_and(|event| event["event"] == "finish")
            })
        {
            break true;
        }
        if started.elapsed() > std::time::Duration::from_secs(10) {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(complete, "watcher did not finish its first scan");
    let logs = command_transcripts(&config.join("rai/logs/projects"));
    for (_, events) in logs {
        assert_eq!(events[0]["source"], "watcher");
        assert!(events[0]["parent"].as_str().unwrap().contains("global"));
        let failed = events[0]["project"] == bad.canonicalize().unwrap().to_string_lossy().as_ref();
        assert_eq!(
            events.last().unwrap()["exitCode"],
            if failed { 1 } else { 0 }
        );
        if failed {
            assert!(transcript_output(&events, "stderr").contains("unsupported rule name"));
        } else {
            assert!(transcript_output(&events, "stdout").contains("Synchronization complete"));
        }
    }
}

#[test]
fn command_logs_use_the_project_root_from_a_subdirectory() {
    let sandbox = tempfile::tempdir().unwrap();
    let repo = sandbox.path().join("project");
    fs::create_dir_all(repo.join(".agents/rules")).unwrap();
    fs::create_dir(repo.join("frontend")).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--template=", "-q"])
            .arg(&repo)
            .status()
            .unwrap()
            .success()
    );
    let config = sandbox.path().join("config");
    let mut locations = vec![repo.clone(), repo.join("frontend")];
    #[cfg(unix)]
    {
        let alias = sandbox.path().join("alias");
        std::os::unix::fs::symlink(&repo, &alias).unwrap();
        locations.push(alias);
    }
    for location in &locations {
        let output = Command::new(env!("CARGO_BIN_EXE_rai"))
            .args(["status", "--json"])
            .current_dir(location)
            .env("HOME", sandbox.path())
            .env("XDG_CONFIG_HOME", &config)
            .env("APPDATA", &config)
            .env("XDG_CACHE_HOME", sandbox.path().join("cache"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let logs = command_transcripts(&config.join("rai/logs/projects"));
    assert_eq!(logs.len(), locations.len());
    let directories: std::collections::HashSet<_> = logs
        .iter()
        .map(|(path, events)| {
            assert_eq!(
                events[0]["project"],
                repo.canonicalize().unwrap().to_string_lossy().as_ref()
            );
            path.parent().unwrap()
        })
        .collect();
    assert_eq!(directories.len(), 1);
    assert!(!repo.join("AGENTS.md").exists());
}
