use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use chrono::NaiveDateTime;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const FIXTURES: &str = "tests/integration/fixtures/sync-scenarios";

struct Case {
    home: TempDir,
    repo: PathBuf,
}

impl Case {
    fn new(fixture: Option<&str>) -> Self {
        let home = tempfile::tempdir().unwrap();
        let repo = home.path().join("project");
        fs::create_dir(&repo).unwrap();
        if let Some(name) = fixture {
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(FIXTURES)
                .join(name);
            copy_tree(&source, &repo);
        }
        fs::write(
            home.path().join("gitconfig"),
            "[core]\n    autocrlf = false\n",
        )
        .unwrap();
        Self { home, repo }
    }

    fn run(&self, args: &[&str], answer: &str) -> Output {
        self.run_at(&self.repo, args, answer)
    }

    fn run_at(&self, repo: &Path, args: &[&str], answer: &str) -> Output {
        let mut child = self
            .command(env!("CARGO_BIN_EXE_rai"))
            .args(args)
            .arg("--repo")
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
            .write_all(answer.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .env("HOME", self.home.path())
            .env("APPDATA", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join("config"))
            .env("XDG_CACHE_HOME", self.home.path().join("cache"))
            .env("GIT_CONFIG_GLOBAL", self.home.path().join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1");
        command
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.repo.join(relative)).unwrap()
    }

    fn migration_root(&self) -> PathBuf {
        self.home.path().join(if cfg!(windows) {
            "rai/migrations"
        } else {
            ".rai/migrations"
        })
    }

    fn backup(&self) -> PathBuf {
        let migrations = self.migration_root().join("project");
        let backups = fs::read_dir(migrations)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(backups.len(), 1);
        backups[0].path()
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn display_paths(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.to_owned()
    }
}

fn stdout(output: &Output) -> String {
    let text = String::from_utf8_lossy(&output.stdout);
    // Keep JSON escapes and values intact; normalize only terminal diagnostics.
    if text.trim_start().starts_with('{') {
        text.into_owned()
    } else {
        display_paths(&text)
    }
}

#[test]
fn recursive_migration_preserves_scopes_and_rolls_back_every_original() {
    let case = Case::new(Some("nested-native"));
    let result = case.run(&["sync"], "y\ny\n");
    assert!(result.status.success(), "{}", stderr(&result));
    let output = stdout(&result);
    let discovered = output
        .split("Discovered unmigrated harness configuration in")
        .nth(1)
        .unwrap()
        .split("Migration proposed")
        .next()
        .unwrap();
    for source in [
        "AGENTS.md",
        "src/AGENTS.md",
        "src/CLAUDE.md",
        "src/deep/AGENTS.md",
    ] {
        assert!(discovered.contains(&format!("  {source}\n")));
    }
    let global = case.read("AGENTS.md");
    assert!(global.contains("Global repository instructions."));
    assert!(!global.contains("Source directory instructions."));
    assert!(!global.contains("Deep directory instructions."));
    let source = case.read("src/AGENTS.md");
    assert!(source.contains("Source directory instructions."));
    assert!(source.contains("Additional source directory instructions."));
    assert!(!source.contains("Deep directory instructions."));
    assert!(
        case.read("src/deep/AGENTS.md")
            .contains("Deep directory instructions.")
    );
    let backup = case.backup();
    assert_eq!(
        fs::read_to_string(backup.join("src/AGENTS.md")).unwrap(),
        "Source directory instructions.\n"
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], 2);
    assert_eq!(manifest["generatedRules"].as_array().unwrap().len(), 3);
    assert!(case.repo.join(".agents/rules/AGENTS-src.md").is_file());
    assert!(case.repo.join(".agents/rules/AGENTS-src-deep.md").is_file());
    for rule in manifest["generatedRules"].as_array().unwrap() {
        let scope = rule["scope"].as_str().unwrap();
        let path = rule["path"].as_str().unwrap();
        let content = case.read(path);
        let header = content
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap()
            .0;
        let metadata: serde_yaml::Value = serde_yaml::from_str(header).unwrap();
        assert_eq!(metadata["name"].as_str(), Some("AGENTS.md"));
        if !scope.is_empty() {
            assert_eq!(metadata["path"].as_str(), Some(scope));
        }
    }
    let repeated = case.run(&["sync"], "");
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(!stdout(&repeated).contains("Migration proposed"));
    let rollback = case.run(&["rollback"], "");
    assert!(rollback.status.success(), "{}", stderr(&rollback));
    assert_eq!(case.read("AGENTS.md"), "Global repository instructions.\n");
    assert_eq!(
        case.read("src/AGENTS.md"),
        "Source directory instructions.\n"
    );
    assert_eq!(
        case.read("src/CLAUDE.md"),
        "Additional source directory instructions.\n"
    );
    assert_eq!(
        case.read("src/deep/AGENTS.md"),
        "Deep directory instructions.\n"
    );
}

#[test]
fn nested_migration_decline_and_hooks_preserve_sources() {
    for args in [
        vec!["sync"],
        vec!["sync", "--git-hook"],
        vec!["sync", "--dry-run"],
        vec!["sync", "--json"],
    ] {
        let case = Case::new(Some("nested-native"));
        fs::create_dir_all(case.repo.join(".agents/rules")).unwrap();
        let result = case.run(&args, "n\n");
        assert!(
            stdout(&result).contains("src/AGENTS.md") || stderr(&result).contains("src/AGENTS.md")
        );
        assert_eq!(
            case.read("src/AGENTS.md"),
            "Source directory instructions.\n"
        );
        assert!(!case.migration_root().exists());
        assert!(!case.repo.join(".codex/config.toml").exists());
    }
}

#[test]
fn sync_migrates_new_nested_instructions_after_a_previous_migration() {
    let case = Case::new(Some("native"));
    assert!(case.run(&["sync"], "y\ny\n").status.success());
    let original_rule = case.read(".agents/rules/migrated-harness.md");
    let original_global = case.read("AGENTS.md");
    fs::create_dir(case.repo.join("src")).unwrap();
    fs::write(case.repo.join("src/AGENTS.md"), "New local instructions.\n").unwrap();
    let result = case.run(&["sync"], "y\ny\n");
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(stdout(&result).contains("Migration proposed"));
    assert_eq!(
        case.read(".agents/rules/migrated-harness.md"),
        original_rule
    );
    assert_eq!(case.read("AGENTS.md"), original_global);
    assert!(
        case.read("src/AGENTS.md")
            .contains("New local instructions.")
    );
    assert_eq!(
        fs::read_dir(case.migration_root().join("project"))
            .unwrap()
            .count(),
        2
    );
    let rollback = case.run(&["rollback"], "");
    assert!(rollback.status.success(), "{}", stderr(&rollback));
    assert_eq!(case.read("src/AGENTS.md"), "New local instructions.\n");
    assert_eq!(
        case.read(".agents/rules/migrated-harness.md"),
        original_rule
    );
}

#[test]
fn recursive_detection_stops_at_fixture_dependency_and_project_boundaries() {
    let case = Case::new(None);
    for directory in [
        "fixtures/sample",
        "node_modules/package",
        "target/build",
        "other",
        "standalone",
    ] {
        fs::create_dir_all(case.repo.join(directory)).unwrap();
        fs::write(
            case.repo.join(directory).join("AGENTS.md"),
            "Do not import.\n",
        )
        .unwrap();
    }
    fs::create_dir(case.repo.join("other/.agents")).unwrap();
    fs::create_dir(case.repo.join("standalone/.git")).unwrap();
    let result = case.run(&["sync"], "");
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(!stdout(&result).contains("Migration proposed"));
    assert!(!case.migration_root().exists());
}

#[test]
fn tracked_nested_migration_can_resume_a_missing_scoped_rule() {
    let case = Case::new(None);
    fs::create_dir(case.repo.join("src")).unwrap();
    fs::write(
        case.repo.join("src/AGENTS.md"),
        "Tracked source instructions.\n",
    )
    .unwrap();
    assert!(
        case.command("git")
            .arg("init")
            .arg(&case.repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        case.command("git")
            .arg("-C")
            .arg(&case.repo)
            .args(["add", "src/AGENTS.md"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let migrated = case.run(&["migrate"], "y\n");
    assert!(migrated.status.success(), "{}", stderr(&migrated));
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(case.backup().join("manifest.json")).unwrap())
            .unwrap();
    let rule = manifest["generatedRules"][0]["path"].as_str().unwrap();
    fs::remove_file(case.repo.join(rule)).unwrap();
    let resumed = case.run(&["sync"], "y\ny\n");
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    assert!(stdout(&resumed).contains("Resume migration"));
    assert!(
        case.read(rule)
            .starts_with("---\nname: AGENTS.md\npath: src\n---\n")
    );
    assert!(
        case.read("src/AGENTS.md")
            .contains("Tracked source instructions.")
    );
    let files = case
        .command("git")
        .arg("-C")
        .arg(&case.repo)
        .args(["ls-files", "src/AGENTS.md"])
        .output()
        .unwrap();
    assert!(files.stdout.is_empty());
    let repeated = case.run(&["sync"], "");
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(!stdout(&repeated).contains("Migration proposed"));
}

#[test]
fn nested_source_with_different_staged_content_blocks_before_any_write() {
    let case = Case::new(None);
    fs::create_dir(case.repo.join("src")).unwrap();
    fs::write(case.repo.join("src/AGENTS.md"), "Staged instructions.\n").unwrap();
    assert!(
        case.command("git")
            .arg("init")
            .arg(&case.repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        case.command("git")
            .arg("-C")
            .arg(&case.repo)
            .args(["add", "src/AGENTS.md"])
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(case.repo.join("src/AGENTS.md"), "Working instructions.\n").unwrap();
    let result = case.run(&["sync"], "y\ny\n");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("staged migration source changed: src/AGENTS.md"));
    assert_eq!(case.read("src/AGENTS.md"), "Working instructions.\n");
    assert!(!case.repo.join(".agents").exists());
    assert!(!case.migration_root().exists());
}

#[test]
fn ambiguous_scoped_rule_names_block_before_migration() {
    for directories in [["src/a-b", "src-a/b"], ["Src/a-b", "src-a/b"]] {
        let case = Case::new(None);
        for directory in directories {
            fs::create_dir_all(case.repo.join(directory)).unwrap();
            fs::write(
                case.repo.join(directory).join("AGENTS.md"),
                "Preserve instructions.\n",
            )
            .unwrap();
        }
        let result = case.run(&["sync"], "y\ny\n");
        assert!(!result.status.success());
        assert!(
            stderr(&result)
                .contains("migration destination conflict: .agents/rules/AGENTS-src-a-b.md")
        );
        assert!(!case.repo.join(".agents").exists());
        assert!(!case.migration_root().exists());
        for directory in directories {
            assert_eq!(
                case.read(&format!("{directory}/AGENTS.md")),
                "Preserve instructions.\n"
            );
        }
    }
}

#[test]
fn scoped_migration_preserves_an_existing_rule_with_the_requested_name() {
    let case = Case::new(Some("nested-native"));
    fs::create_dir_all(case.repo.join(".agents/rules")).unwrap();
    fs::write(
        case.repo.join(".agents/rules/AGENTS-src.md"),
        "Existing canonical instructions.\n",
    )
    .unwrap();
    let result = case.run(&["sync"], "y\ny\n");
    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("migration destination conflict: .agents/rules/AGENTS-src.md")
    );
    assert_eq!(
        case.read(".agents/rules/AGENTS-src.md"),
        "Existing canonical instructions.\n"
    );
    assert_eq!(
        case.read("src/AGENTS.md"),
        "Source directory instructions.\n"
    );
    assert!(!case.migration_root().exists());
}

fn stderr(output: &Output) -> String {
    display_paths(&String::from_utf8_lossy(&output.stderr))
}

#[test]
fn rule_metadata_controls_name_and_scope_after_the_source_is_renamed() {
    let case = Case::new(Some("named-rules"));
    let first = case.run(&["sync"], "");
    assert!(first.status.success(), "{}", stderr(&first));
    let global = case.read("AGENTS.md");
    assert!(global.contains("## AGENTS.md"));
    assert!(!global.contains("Named source instructions."));
    let source = case.read("src/AGENTS.md");
    assert!(source.contains("## AGENTS.md"));
    assert!(source.contains("Named source instructions."));
    assert!(!source.contains("storage-name.md"));
    for output in [
        "src/AGENTS.md",
        ".claude/rules/src.md",
        ".github/instructions/src.instructions.md",
    ] {
        let content = case.read(output);
        assert!(content.contains("## AGENTS.md"));
        assert!(!content.contains("name: AGENTS.md"));
    }
    fs::rename(
        case.repo.join(".agents/rules/storage-name.md"),
        case.repo.join(".agents/rules/unrelated-filename.md"),
    )
    .unwrap();
    let repeated = case.run(&["sync"], "");
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(stdout(&repeated).contains("AGENTS.md — already synchronized"));
    assert_eq!(case.read("src/AGENTS.md"), source);
    assert_eq!(case.read("AGENTS.md"), global);
}

#[test]
fn migrated_rule_metadata_keeps_the_original_instruction_filename() {
    for original in [
        "src/AGENTS.md",
        "src/CLAUDE.md",
        ".github/copilot-instructions.md",
    ] {
        let case = Case::new(None);
        fs::create_dir_all(case.repo.join(original).parent().unwrap()).unwrap();
        fs::write(case.repo.join(original), "Original instructions.\n").unwrap();
        let migrated = case.run(&["sync"], "y\nn\n");
        assert!(migrated.status.success(), "{}", stderr(&migrated));
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(case.backup().join("manifest.json")).unwrap())
                .unwrap();
        let rule = if manifest["version"] == 1 {
            manifest["generated"].as_str().unwrap()
        } else {
            manifest["generatedRules"][0]["path"].as_str().unwrap()
        };
        let content = case.read(rule);
        let header = content
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap()
            .0;
        let metadata: serde_yaml::Value = serde_yaml::from_str(header).unwrap();
        assert_eq!(
            metadata["name"].as_str(),
            Path::new(original).file_name().unwrap().to_str()
        );
        if original.starts_with("src/") {
            assert_eq!(rule, ".agents/rules/AGENTS-src.md");
            assert_eq!(metadata["path"].as_str(), Some("src"));
        }
        let rollback = case.run(&["rollback"], "");
        assert!(rollback.status.success(), "{}", stderr(&rollback));
        assert_eq!(case.read(original), "Original instructions.\n");
    }
}

#[test]
fn invalid_rule_metadata_fails_before_any_projection_is_written() {
    for header in [
        "name: ''\npath: src",
        "name: 123\npath: src",
        "name: [file.md]\npath: src",
        "name: AGENTS.md\nname: CLAUDE.md\npath: src",
        "name: AGENTS.md\npath: src\npath: .",
        "name: |\n  multiple\n  lines\npath: src",
        "name: AGENTS.md\npath: ../outside",
        "name: AGENTS.md\npath: false",
        "name: AGENTS.md\nunknown: value",
        "name: repository.md\npath: .",
        "name: AGENTS-src.md\npath: src",
    ] {
        let case = Case::new(Some("named-rules"));
        let content = format!("---\n{header}\n---\n\nKeep these instructions.\n");
        fs::write(case.repo.join(".agents/rules/storage-name.md"), &content).unwrap();
        let result = case.run(&["sync"], "");
        assert!(
            !result.status.success(),
            "accepted invalid metadata: {header}"
        );
        assert_eq!(case.read(".agents/rules/storage-name.md"), content);
        for output in [
            "AGENTS.md",
            "src/AGENTS.md",
            ".codex/config.toml",
            "CLAUDE.md",
            ".gitignore",
        ] {
            assert!(!case.repo.join(output).exists(), "{header}: {output}");
        }
    }
}

#[test]
fn unsupported_rule_name_is_reported_without_changing_existing_projections() {
    let case = Case::new(Some("invalid-rule-name"));
    assert!(
        case.command("git")
            .args(["init", "-q"])
            .current_dir(&case.repo)
            .status()
            .unwrap()
            .success()
    );
    let original = case.read(".agents/rules/repository.md");
    fs::write(
        case.repo.join(".agents/rules/repository.md"),
        original.replace("name: repository.md", "name: AGENTS.md"),
    )
    .unwrap();
    let initial = case.run(&["sync"], "");
    assert!(initial.status.success(), "{}", stderr(&initial));
    let snapshots = [
        "AGENTS.md",
        "CLAUDE.md",
        ".github/copilot-instructions.md",
        ".gitignore",
    ]
    .map(|path| (path, case.read(path)));
    fs::write(case.repo.join(".agents/rules/repository.md"), &original).unwrap();
    for args in [
        vec!["sync"],
        vec!["sync", "--dry-run"],
        vec!["sync", "--json"],
        vec!["sync", "--git-hook"],
        vec!["status", "--json"],
        vec!["doctor", "--json"],
        vec!["sync", "--codex-hook"],
    ] {
        let result = case.run(&args, "");
        let output = stdout(&result);
        let json: Option<serde_json::Value> = serde_json::from_str(&output).ok();
        let message = json.as_ref().and_then(|value| {
            value["error"]
                .as_str()
                .or_else(|| value["conflict"].as_str())
                .or_else(|| value["reason"].as_str())
                .or_else(|| value["issues"][0]["message"].as_str())
        });
        let diagnostic = display_paths(&format!(
            "{}{}",
            message.unwrap_or(&output),
            stderr(&result)
        ));
        assert!(
            diagnostic.contains("unsupported rule name: repository.md"),
            "{args:?}: {diagnostic}"
        );
        assert!(
            diagnostic.contains(".agents/rules/repository.md"),
            "{args:?}: {diagnostic}"
        );
        if args[0] == "doctor" {
            let json: serde_json::Value = serde_json::from_str(&stdout(&result)).unwrap();
            assert_eq!(json["ok"], false);
            assert!(
                json["issues"][0]["solution"]
                    .as_str()
                    .unwrap()
                    .contains("original instruction filename")
            );
            assert_eq!(json["issues"][0]["autoFixable"], false);
        } else if args.contains(&"--codex-hook") {
            let json: serde_json::Value = serde_json::from_str(&stdout(&result)).unwrap();
            assert_eq!(json["decision"], "block");
        } else {
            assert!(!result.status.success(), "{args:?}");
        }
        assert_eq!(case.read(".agents/rules/repository.md"), original);
        for (path, content) in &snapshots {
            assert_eq!(case.read(path), *content, "{args:?}: {path}");
        }
    }
}

#[test]
fn invalid_existing_rule_blocks_before_importing_new_native_instructions() {
    let case = Case::new(Some("invalid-rule-name"));
    fs::create_dir(case.repo.join("src")).unwrap();
    fs::write(
        case.repo.join("src/AGENTS.md"),
        "Preserve native instructions.\n",
    )
    .unwrap();
    let result = case.run(&["sync"], "y\ny\n");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("unsupported rule name: repository.md"));
    assert_eq!(
        case.read("src/AGENTS.md"),
        "Preserve native instructions.\n"
    );
    assert!(!case.repo.join(".agents/rules/AGENTS-src.md").exists());
    assert!(!case.repo.join("AGENTS.md").exists());
    assert!(!case.migration_root().exists());
}

#[test]
fn resuming_an_unsupported_recorded_rule_does_not_remove_its_tracked_source() {
    let case = Case::new(None);
    fs::create_dir(case.repo.join("src")).unwrap();
    fs::write(case.repo.join("src/AGENTS.md"), "Tracked instructions.\n").unwrap();
    assert!(
        case.command("git")
            .arg("init")
            .arg(&case.repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        case.command("git")
            .arg("-C")
            .arg(&case.repo)
            .args(["add", "src/AGENTS.md"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(case.run(&["migrate"], "y\n").status.success());
    let manifest_path = case.backup().join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let rule = manifest["generatedRules"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    manifest["generatedRules"][0]["name"] = serde_json::json!("repository.md");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::remove_file(case.repo.join(&rule)).unwrap();
    let result = case.run(&["sync"], "y\ny\n");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("unsupported rule name: repository.md"));
    assert_eq!(case.read("src/AGENTS.md"), "Tracked instructions.\n");
    assert!(!case.repo.join(rule).exists());
    let tracked = case
        .command("git")
        .arg("-C")
        .arg(&case.repo)
        .args(["ls-files", "src/AGENTS.md"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&tracked.stdout).trim(),
        "src/AGENTS.md"
    );
}

#[test]
fn recorded_migrations_without_name_metadata_still_resume() {
    for relative in ["AGENTS.md", "src/AGENTS.md"] {
        let case = Case::new(None);
        fs::create_dir_all(case.repo.join(relative).parent().unwrap()).unwrap();
        fs::write(case.repo.join(relative), "Legacy instructions.\n").unwrap();
        assert!(
            case.command("git")
                .arg("init")
                .arg(&case.repo)
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(
            case.command("git")
                .arg("-C")
                .arg(&case.repo)
                .args(["add", relative])
                .output()
                .unwrap()
                .status
                .success()
        );
        let migrated = case.run(&["migrate"], "y\n");
        assert!(migrated.status.success(), "{}", stderr(&migrated));
        let backup = case.backup();
        let manifest_path = backup.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        let rule = if relative == "AGENTS.md" {
            ".agents/rules/migrated-harness.md"
        } else {
            ".agents/rules/AGENTS-src.md"
        };
        let content = case.read(rule);
        let (_, body) = content
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap();
        let legacy = if relative == "AGENTS.md" {
            body.trim_start_matches('\n').to_string()
        } else {
            format!("---\npath: src\n---\n{body}")
        };
        let hash = format!("{:x}", Sha256::digest(legacy.as_bytes()));
        if relative == "AGENTS.md" {
            manifest.as_object_mut().unwrap().remove("generatedName");
            manifest["generatedSha256"] = serde_json::json!(hash);
        } else {
            manifest["generatedRules"][0]
                .as_object_mut()
                .unwrap()
                .remove("name");
            manifest["generatedRules"][0]["sha256"] = serde_json::json!(hash);
        }
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::remove_file(case.repo.join(rule)).unwrap();
        let resumed = case.run(&["sync"], "y\ny\n");
        assert!(resumed.status.success(), "{}", stderr(&resumed));
        assert_eq!(case.read(rule), legacy);
        assert!(case.read(relative).contains("Legacy instructions."));
    }
}

#[test]
fn manual_sync_bootstraps_only_the_canonical_skeleton() {
    let case = Case::new(None);
    let result = case.run(&["sync"], "");
    assert!(result.status.success(), "{}", stderr(&result));
    for directory in ["rules", "agents", "commands", "skills"] {
        assert!(
            case.repo
                .join(".agents")
                .join(directory)
                .join(".keep")
                .exists()
        );
    }
    assert_eq!(case.read(".agents/mcp.yaml"), "servers: {}\n");
    for output in ["AGENTS.md", "CLAUDE.md", ".mcp.json", ".vscode/mcp.json"] {
        assert!(!case.repo.join(output).exists(), "{output}");
    }
    assert!(case.run(&["sync"], "").status.success());
}

#[test]
fn sync_cleans_populated_canonical_directories_and_previews_without_writes() {
    for args in [vec!["sync", "--json"], vec!["sync", "--git-hook"]] {
        let case = Case::new(Some("typed-resources"));
        let removed = [
            ".agents/rules/.keep",
            ".agents/agents/.keep",
            ".agents/subagents/.keep",
            ".agents/skills/.keep",
            ".agents/skills/example/.keep",
            ".agents/extra/assets/.keep",
            ".agents/extra/.keep",
        ];
        let retained = [
            ".agents/.keep",
            ".agents/commands/.keep",
            ".agents/extra/empty/.keep",
            "src/.keep",
        ];
        for path in removed.iter().chain(&retained) {
            let path = case.repo.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        fs::write(
            case.repo.join(".agents/subagents/legacy.md"),
            "---\nname: legacy\ndescription: Legacy agent\n---\nReview changes.\n",
        )
        .unwrap();
        fs::write(
            case.repo.join(".agents/skills/example/SKILL.md"),
            "---\nname: example\ndescription: Example skill\n---\nCheck the assets.\n",
        )
        .unwrap();
        fs::write(case.repo.join(".agents/extra/assets/icon.svg"), "<svg/>\n").unwrap();
        // Canonical placeholders can be Git-tracked; cleanup must leave the index alone.
        assert!(
            case.command("git")
                .args(["init", "-q"])
                .current_dir(&case.repo)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            case.command("git")
                .args(["add", ".agents"])
                .current_dir(&case.repo)
                .status()
                .unwrap()
                .success()
        );
        let index = fs::read(case.repo.join(".git/index")).unwrap();
        let rules = case.read(".agents/rules/source.md");
        for options in [
            vec!["sync", "--dry-run", "--json"],
            vec!["status", "--json"],
        ] {
            let preview = case.run(&options, "");
            assert!(preview.status.success(), "{}", stderr(&preview));
            let value: serde_json::Value = serde_json::from_str(&stdout(&preview)).unwrap();
            for path in removed {
                assert!(value["changes"].as_array().unwrap().iter().any(|change| {
                    change["path"]
                        .as_str()
                        .is_some_and(|reported| Path::new(reported) == Path::new(path))
                        && change["action"] == "delete"
                }));
                assert!(case.repo.join(path).is_file());
            }
            assert!(!case.repo.join("AGENTS.md").exists());
        }
        let preview = case.run(&["sync", "--dry-run"], "");
        assert!(preview.status.success(), "{}", stderr(&preview));
        assert!(stdout(&preview).contains("directory is populated · will remove placeholder"));
        let result = case.run(&args, "");
        assert!(result.status.success(), "{}", stderr(&result));
        for path in removed {
            assert!(!case.repo.join(path).exists(), "{path}");
        }
        for path in retained {
            assert!(case.repo.join(path).is_file(), "{path}");
        }
        assert_eq!(case.read(".agents/rules/source.md"), rules);
        assert!(
            case.read("src/AGENTS.md")
                .contains("Scoped source instructions.")
        );
        assert!(
            !case
                .read("AGENTS.md")
                .contains("Scoped source instructions.")
        );
        assert_eq!(case.read(".agents/extra/assets/icon.svg"), "<svg/>\n");
        assert_eq!(fs::read(case.repo.join(".git/index")).unwrap(), index);
        let repeated = case.run(&["sync", "--json"], "");
        assert!(repeated.status.success(), "{}", stderr(&repeated));
        let value: serde_json::Value = serde_json::from_str(&stdout(&repeated)).unwrap();
        assert!(
            value["changes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|change| change["action"] == "unchanged")
        );
    }
}

#[test]
fn blocked_sync_preserves_canonical_placeholders() {
    let case = Case::new(Some("typed-resources"));
    assert!(case.run(&["sync"], "").status.success());
    let keep = case.repo.join(".agents/rules/.keep");
    fs::write(&keep, "").unwrap();
    fs::write(case.repo.join(".codex/config.toml"), "user configuration\n").unwrap();
    let result = case.run(&["sync", "--json"], "");
    assert!(!result.status.success());
    assert!(keep.is_file());
    assert_eq!(case.read(".codex/config.toml"), "user configuration\n");
}

#[cfg(unix)]
#[test]
fn placeholder_cleanup_preserves_symlinks_and_directories_named_keep() {
    use std::os::unix::fs::symlink;

    let case = Case::new(Some("typed-resources"));
    let outside = case.home.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join(".keep"), "").unwrap();
    fs::write(outside.join("data"), "preserve\n").unwrap();
    let extra = case.repo.join(".agents/extra");
    fs::create_dir_all(extra.join("directory/.keep")).unwrap();
    fs::write(extra.join("directory/data"), "preserve\n").unwrap();
    fs::write(extra.join("data"), "preserve\n").unwrap();
    symlink(&outside, extra.join("linked-directory")).unwrap();
    symlink(outside.join(".keep"), extra.join(".keep")).unwrap();
    let result = case.run(&["sync", "--json"], "");
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(extra.join(".keep").is_symlink());
    assert!(extra.join("linked-directory").is_symlink());
    assert!(extra.join("directory/.keep").is_dir());
    assert!(outside.join(".keep").is_file());
    assert_eq!(
        fs::read_to_string(outside.join("data")).unwrap(),
        "preserve\n"
    );
}

#[test]
fn read_only_modes_never_bootstrap_an_unconfigured_project() {
    for option in ["--dry-run", "--json"] {
        let case = Case::new(None);
        let result = case.run(&["sync", option], "");
        assert!(!result.status.success());
        if option == "--json" {
            let value: serde_json::Value = serde_json::from_str(&stdout(&result)).unwrap();
            assert_eq!(value["ok"], false);
            assert!(value["error"].as_str().unwrap().contains(".agents"));
        }
        assert!(!case.repo.join(".agents").exists());
    }
}

#[test]
fn hook_skips_repositories_without_canonical_sources() {
    for fixture in [None, Some("native"), Some("unsupported")] {
        let case = Case::new(fixture);
        let result = case.run(&["sync", "--git-hook"], "");
        assert!(result.status.success(), "{}", stderr(&result));
        assert!(!case.repo.join(".agents").exists());
    }
}

#[test]
fn migration_decline_or_eof_preserves_every_source() {
    for answer in ["n\n", ""] {
        let case = Case::new(Some("native"));
        let result = case.run(&["sync"], answer);
        assert!(result.status.success(), "{}", stderr(&result));
        assert!(stdout(&result).contains("Migration cancelled"));
        assert!(!case.repo.join(".agents").exists());
        assert_eq!(case.read("AGENTS.md"), "Keep the project instructions.\n");
        assert_eq!(case.read("CLAUDE.md"), "Review changes before delivery.\n");
    }
}

#[test]
fn migration_can_stop_after_import_or_continue_to_sync() {
    for (answer, sync) in [("y\nn\n", false), ("y\ny\n", true)] {
        let case = Case::new(Some("native"));
        let result = case.run(&["sync"], answer);
        assert!(result.status.success(), "{}", stderr(&result));
        assert!(stdout(&result).contains("Synchronize now?"));
        assert!(case.repo.join(".agents/rules/migrated-harness.md").exists());
        assert!(case.backup().join("manifest.json").exists());
        assert_eq!(
            case.backup().parent().unwrap().file_name().unwrap(),
            "project"
        );
        let name = case
            .backup()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(NaiveDateTime::parse_from_str(&name, "%Y-%m-%d_%H-%M-%S").is_ok());
        assert!(!case.repo.join(".agents/migration-backup").exists());
        assert!(!case.repo.join("AGENTS.md").exists() || sync);
        assert_eq!(case.repo.join(".codex/config.toml").exists(), sync);
        if sync {
            assert!(
                case.read("AGENTS.md")
                    .contains("Keep the project instructions.")
            );
            assert!(
                case.read("CLAUDE.md")
                    .contains("Review changes before delivery.")
            );
        }
    }
}

#[test]
fn unsupported_native_sources_block_before_writing() {
    let unsupported = Case::new(Some("unsupported"));
    let result = unsupported.run(&["sync"], "y\n");
    assert!(!result.status.success());
    assert!(stderr(&result).contains(".codex/config.toml"));
    assert!(stdout(&result).contains("  .codex/config.toml\n"));
    assert!(!unsupported.repo.join(".agents").exists());
}

#[test]
fn tracked_native_source_migrates_and_projects_after_approval() {
    let tracked = Case::new(Some("native"));
    assert!(
        tracked
            .command("git")
            .arg("init")
            .arg(&tracked.repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        tracked
            .command("git")
            .arg("-C")
            .arg(&tracked.repo)
            .args(["add", "AGENTS.md"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        tracked
            .command("git")
            .arg("-C")
            .arg(&tracked.repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-qm",
                "initial",
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let declined = tracked.run(&["sync"], "n\n");
    assert!(declined.status.success(), "{}", stderr(&declined));
    assert!(!tracked.repo.join(".agents").exists());
    let approved = tracked.run(&["sync"], "y\ny\n");
    assert!(approved.status.success(), "{}", stderr(&approved));
    let preview = stdout(&declined);
    let discovered = preview
        .find("Discovered unmigrated harness configuration in")
        .unwrap();
    let source = preview.find("  AGENTS.md\n").unwrap();
    let proposed = preview.find("Migration proposed").unwrap();
    assert!(discovered < source && source < proposed);
    assert!(preview.contains(
        "Copy instructions from AGENTS.md -> .agents/rules/migrated-harness.md (scope: repository)"
    ));
    assert!(preview.contains("Remove AGENTS.md from Git before generating its projection"));
    assert!(preview.contains("Rollback this migration with `rai rollback`"));
    assert!(!preview.contains("manifest.json"));
    assert!(!preview.contains(&display_paths(&tracked.backup().display().to_string())));
    assert!(
        tracked
            .read("AGENTS.md")
            .contains("Keep the project instructions.")
    );
    assert!(
        tracked
            .repo
            .join(".agents/rules/migrated-harness.md")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(tracked.backup().join("AGENTS.md")).unwrap(),
        "Keep the project instructions.\n"
    );
    assert!(tracked.repo.join(".codex/config.toml").exists());
    let status = tracked
        .command("git")
        .arg("-C")
        .arg(&tracked.repo)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).contains("AGENTS.md"));
    let completed = tracked.run(&["sync"], "");
    assert!(completed.status.success(), "{}", stderr(&completed));
    assert!(
        tracked
            .read("AGENTS.md")
            .contains("Keep the project instructions.")
    );
    assert!(tracked.repo.join(".codex/config.toml").exists());
}

#[test]
fn recorded_migration_restores_missing_rule_and_finishes() {
    let case = Case::new(Some("native"));
    assert!(
        case.command("git")
            .arg("init")
            .arg(&case.repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        case.command("git")
            .arg("-C")
            .arg(&case.repo)
            .args(["add", "AGENTS.md"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let migrated = case.run(&["migrate"], "y\n");
    assert!(migrated.status.success(), "{}", stderr(&migrated));
    fs::remove_file(case.repo.join(".agents/rules/migrated-harness.md")).unwrap();
    let declined = case.run(&["sync"], "n\n");
    assert!(declined.status.success());
    assert!(!case.repo.join(".agents/rules/migrated-harness.md").exists());
    assert_eq!(case.read("AGENTS.md"), "Keep the project instructions.\n");
    let resumed = case.run(&["sync"], "y\ny\n");
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    assert!(
        case.read(".agents/rules/migrated-harness.md")
            .contains("Keep the project instructions.")
    );
    assert!(
        case.read("AGENTS.md")
            .contains("Keep the project instructions.")
    );
    assert!(case.repo.join(".codex/config.toml").exists());
}

#[test]
fn rollback_of_approved_tracked_migration_keeps_original() {
    let case = Case::new(Some("native"));
    assert!(
        case.command("git")
            .arg("init")
            .arg(&case.repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        case.command("git")
            .arg("-C")
            .arg(&case.repo)
            .args(["add", "AGENTS.md"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(case.run(&["migrate"], "y\n").status.success());
    let rollback = case.run(&["rollback"], "");
    assert!(rollback.status.success(), "{}", stderr(&rollback));
    assert_eq!(case.read("AGENTS.md"), "Keep the project instructions.\n");
    assert_eq!(case.read("CLAUDE.md"), "Review changes before delivery.\n");
    assert!(!case.repo.join(".agents/rules/migrated-harness.md").exists());
}

#[test]
fn rollback_can_read_an_older_repository_local_backup() {
    let case = Case::new(Some("native"));
    assert!(case.run(&["sync"], "y\nn\n").status.success());
    let legacy = case.repo.join(".agents/migration-backup");
    fs::rename(case.backup(), &legacy).unwrap();
    let rollback = case.run(&["rollback"], "");
    assert!(rollback.status.success(), "{}", stderr(&rollback));
    assert_eq!(case.read("AGENTS.md"), "Keep the project instructions.\n");
    assert_eq!(case.read("CLAUDE.md"), "Review changes before delivery.\n");
}

#[test]
fn rollback_can_read_a_previous_full_hash_backup() {
    let case = Case::new(Some("native"));
    assert!(case.run(&["sync"], "y\nn\n").status.success());
    let root = case.repo.canonicalize().unwrap();
    let previous_id = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
    let previous = case.migration_root().join(previous_id);
    fs::rename(case.backup(), &previous).unwrap();
    let rollback = case.run(&["rollback"], "");
    assert!(rollback.status.success(), "{}", stderr(&rollback));
    assert_eq!(case.read("AGENTS.md"), "Keep the project instructions.\n");
}

#[test]
fn rollback_can_read_a_previous_short_hash_backup() {
    let case = Case::new(Some("native"));
    assert!(case.run(&["sync"], "y\nn\n").status.success());
    let previous = case.migration_root().join("abcdef123456");
    fs::rename(case.backup(), &previous).unwrap();
    let rollback = case.run(&["rollback"], "");
    assert!(rollback.status.success(), "{}", stderr(&rollback));
    assert_eq!(case.read("AGENTS.md"), "Keep the project instructions.\n");
}

#[test]
fn rollback_can_read_a_previous_flat_dated_backup() {
    let case = Case::new(Some("native"));
    assert!(case.run(&["sync"], "y\nn\n").status.success());
    let previous = case
        .migration_root()
        .join(case.backup().file_name().unwrap());
    fs::rename(case.backup(), &previous).unwrap();
    let rollback = case.run(&["rollback"], "");
    assert!(rollback.status.success(), "{}", stderr(&rollback));
    assert_eq!(case.read("AGENTS.md"), "Keep the project instructions.\n");
}

#[test]
fn two_projects_share_the_backup_root_without_overwriting_each_other() {
    let case = Case::new(Some("native"));
    let other_parent = case.home.path().join("other-parent");
    fs::create_dir(&other_parent).unwrap();
    let other = other_parent.join("project");
    fs::create_dir(&other).unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURES)
            .join("native"),
        &other,
    );
    assert!(case.run(&["sync"], "y\nn\n").status.success());
    let second = case.run_at(&other, &["sync"], "y\nn\n");
    assert!(second.status.success(), "{}", stderr(&second));
    let backups = fs::read_dir(case.migration_root().join("project"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(backups.len(), 2);
    for repo in [
        case.repo.canonicalize().unwrap(),
        other.canonicalize().unwrap(),
    ] {
        assert!(backups.iter().any(|entry| {
            let manifest = fs::read_to_string(entry.path().join("manifest.json")).unwrap();
            let value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
            value["repository"] == repo.to_string_lossy().as_ref()
        }));
    }
}

#[test]
fn existing_canonical_sources_sync_and_remain_idempotent() {
    let case = Case::new(Some("canonical"));
    assert!(case.run(&["sync", "--dry-run"], "").status.success());
    assert!(!case.repo.join("AGENTS.md").exists());
    assert!(case.run(&["sync"], "").status.success());
    assert!(
        case.read("AGENTS.md")
            .contains("Follow the project conventions.")
    );
    for projection in [
        "AGENTS.md",
        "CLAUDE.md",
        ".github/copilot-instructions.md",
        ".codex/config.toml",
    ] {
        assert!(case.repo.join(projection).is_file(), "missing {projection}");
    }
    let second = case.run(&["sync", "--git-hook"], "");
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(!stdout(&second).contains("Migration proposed"));
    assert!(stdout(&second).contains("AGENTS.md — already synchronized"));
}

#[test]
fn existing_canonical_sources_detect_additional_native_configuration() {
    let case = Case::new(Some("canonical-extra"));
    for args in [&["sync"][..], &["sync", "--git-hook"][..]] {
        let result = case.run(args, "");
        assert!(!result.status.success());
        assert!(stderr(&result).contains(".claude/commands/review.md"));
        assert!(!case.repo.join("AGENTS.md").exists());
        assert_eq!(
            case.read(".claude/commands/review.md"),
            "Check the project before delivery.\n"
        );
    }
}

#[test]
fn existing_canonical_rules_survive_approved_import_with_both_sync_choices() {
    for (answer, sync) in [("y\nn\n", false), ("y\ny\n", true)] {
        let case = Case::new(Some("canonical"));
        fs::write(case.repo.join("CLAUDE.md"), "Existing Claude guidance.\n").unwrap();
        let result = case.run(&["sync"], answer);
        assert!(result.status.success(), "{}", stderr(&result));
        assert_eq!(
            case.read(".agents/rules/project.md"),
            "Follow the project conventions.\n"
        );
        assert!(
            case.read(".agents/rules/migrated-harness.md")
                .contains("Existing Claude guidance.")
        );
        assert_eq!(
            fs::read_to_string(case.backup().join("CLAUDE.md")).unwrap(),
            "Existing Claude guidance.\n"
        );
        assert_eq!(case.repo.join("AGENTS.md").exists(), sync);
        assert_eq!(case.repo.join("CLAUDE.md").exists(), sync);
    }
}

#[test]
fn read_only_modes_report_unmanaged_native_sources_without_writes() {
    for option in ["--dry-run", "--json"] {
        let case = Case::new(Some("canonical-extra"));
        let result = case.run(&["sync", option], "");
        assert!(!result.status.success());
        assert!(stderr(&result).contains(".claude/commands/review.md"));
        if option == "--json" {
            let value: serde_json::Value = serde_json::from_str(&stdout(&result)).unwrap();
            assert_eq!(value["ok"], false);
            assert!(value["error"].as_str().unwrap().contains("review.md"));
        }
        assert!(!case.repo.join("AGENTS.md").exists());
        assert!(!case.repo.join(".agents/rules/migrated-harness.md").exists());
    }
}

#[cfg(unix)]
#[test]
fn symlinked_native_source_blocks_migration_without_following_link() {
    use std::os::unix::fs::symlink;

    let case = Case::new(Some("canonical"));
    let outside = case.home.path().join("outside.md");
    fs::write(&outside, "Outside instructions.\n").unwrap();
    symlink(&outside, case.repo.join("CLAUDE.md")).unwrap();
    let result = case.run(&["sync"], "y\n");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("symlink migration source"));
    assert_eq!(
        fs::read_to_string(&outside).unwrap(),
        "Outside instructions.\n"
    );
    assert!(!case.repo.join(".agents/rules/migrated-harness.md").exists());
}

#[test]
fn unowned_native_collision_is_reviewed_manually_and_preserved_by_hook() {
    let case = Case::new(Some("canonical-collision"));
    let hook = case.run(&["sync", "--git-hook"], "");
    assert!(!hook.status.success());
    assert_eq!(
        case.read("AGENTS.md"),
        "Do not replace these hand-written instructions.\n"
    );
    assert!(!case.repo.join(".agents/rules/migrated-harness.md").exists());

    let manual = case.run(&["sync"], "n\n");
    assert!(manual.status.success(), "{}", stderr(&manual));
    assert!(stdout(&manual).contains("Migration proposed"));
    assert_eq!(
        case.read("AGENTS.md"),
        "Do not replace these hand-written instructions.\n"
    );
    assert!(!case.repo.join(".agents/rules/migrated-harness.md").exists());
}

#[test]
fn typed_resources_project_all_adapters_and_preserve_scope() {
    let case = Case::new(Some("typed-resources"));
    let result = case.run(&["sync"], "");
    assert!(result.status.success(), "{}", stderr(&result));
    for path in ["AGENTS.md", "CLAUDE.md", ".github/copilot-instructions.md"] {
        let body = case.read(path);
        assert!(body.contains("Shared repository instructions."));
        assert!(!body.contains("Scoped source instructions."));
        assert!(!body.contains("type: rule"));
    }
    for path in [
        "src/AGENTS.md",
        ".claude/rules/src.md",
        ".github/instructions/src.instructions.md",
    ] {
        assert!(case.read(path).contains("Scoped source instructions."));
    }
    assert!(case.read(".codex/config.toml").contains("reviewer"));
    assert!(case.repo.join(".claude/agents/reviewer.md").exists());
    assert!(case.repo.join(".github/agents/reviewer.agent.md").exists());
    let repeated = case.run(&["sync"], "");
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(stdout(&repeated).contains("Everything is synchronized. No files changed."));
}

#[test]
fn resource_type_must_match_its_directory_before_any_write() {
    for (path, header) in [
        ("rules/repository.md", "type: agent\npath: ."),
        ("rules/repository.md", "type: command\npath: ."),
        ("rules/repository.md", "type: unknown\npath: ."),
        ("rules/repository.md", "type: 42\npath: ."),
        ("rules/repository.md", "type: rule\ntype: agent\npath: ."),
        (
            "agents/reviewer.md",
            "type: rule\nname: reviewer\ndescription: Review",
        ),
        (
            "agents/reviewer.md",
            "type: command\nname: reviewer\ndescription: Review",
        ),
        ("agents/reviewer.md", "name: reviewer\ndescription: Review"),
    ] {
        let case = Case::new(Some("typed-resources"));
        let content = format!("---\n{header}\n---\n\nPreserve these instructions.\n");
        fs::write(case.repo.join(".agents").join(path), &content).unwrap();
        let result = case.run(&["sync"], "");
        assert!(!result.status.success(), "{path}: {header}");
        assert_eq!(case.read(&format!(".agents/{path}")), content);
        for output in ["AGENTS.md", "CLAUDE.md", ".codex/config.toml", ".gitignore"] {
            assert!(!case.repo.join(output).exists(), "{path}: {output}");
        }
    }
}

#[test]
fn legacy_subagents_are_supported_but_duplicate_agent_names_are_rejected() {
    let case = Case::new(Some("typed-resources"));
    fs::create_dir(case.repo.join(".agents/subagents")).unwrap();
    let legacy = "---\nname: reviewer\ndescription: Review changes\n---\n\nLegacy instructions.\n";
    fs::write(case.repo.join(".agents/subagents/reviewer.md"), legacy).unwrap();
    let conflict = case.run(&["sync"], "");
    assert!(!conflict.status.success());
    assert!(stderr(&conflict).contains("duplicate subagent name: reviewer"));
    assert!(!case.repo.join("AGENTS.md").exists());
    fs::remove_file(case.repo.join(".agents/agents/reviewer.md")).unwrap();
    let result = case.run(&["sync"], "");
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(
        case.read(".claude/agents/reviewer.md")
            .contains("Legacy instructions.")
    );
}

#[test]
fn commands_are_reported_as_unsupported_without_being_silently_ignored() {
    let case = Case::new(Some("typed-resources"));
    let content = "---\ntype: command\n---\n\nReview the current changes.\n";
    fs::write(case.repo.join(".agents/commands/review.md"), content).unwrap();
    let result = case.run(&["sync"], "");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("command projection is not supported yet"));
    assert!(stderr(&result).contains(".agents/commands/review.md"));
    assert_eq!(case.read(".agents/commands/review.md"), content);
    assert!(!case.repo.join("AGENTS.md").exists());
}

#[test]
fn migration_previews_gitignore_and_rollback_restores_it() {
    for existing in [None, Some("/user-cache/\n")] {
        let case = Case::new(Some("native"));
        assert!(
            case.command("git")
                .args(["init", "-q"])
                .current_dir(&case.repo)
                .status()
                .unwrap()
                .success()
        );
        if let Some(content) = existing {
            fs::write(case.repo.join(".gitignore"), content).unwrap();
        }
        for name in ["integrate-harness", "release-cli"] {
            let skill = case.repo.join(format!(".agents/skills/{name}"));
            fs::create_dir_all(&skill).unwrap();
            fs::write(
                skill.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Test skill\n---\nInstructions.\n"),
            )
            .unwrap();
        }
        let declined = case.run(&["migrate"], "n\n");
        assert!(declined.status.success(), "{}", stderr(&declined));
        let preview = stdout(&declined);
        let discovery = preview
            .find(if existing.is_some() {
                "Discovered outdated .gitignore"
            } else {
                "Discovered missing .gitignore"
            })
            .unwrap();
        let proposed = preview.find("Migration proposed").unwrap();
        let change = preview
            .find(if existing.is_some() {
                "Update .gitignore"
            } else {
                "Create .gitignore"
            })
            .unwrap();
        let confirmation = preview.find("Proceed with migration?").unwrap();
        assert!(discovery < proposed && proposed < change && change < confirmation);
        assert!(preview.contains("Add /AGENTS.md"));
        assert_eq!(preview.matches("    Add /.claude\n").count(), 1);
        assert!(!preview.contains("    Add /.claude/"));
        assert_eq!(preview.matches("    Add /.github\n").count(), 1);
        assert!(!preview.contains("    Add /.github/"));
        assert_eq!(preview.matches("    Add /.codex\n").count(), 1);
        assert!(!preview.contains("    Add /.codex/"));
        assert_eq!(
            fs::read_to_string(case.repo.join(".gitignore"))
                .ok()
                .as_deref(),
            existing
        );
        let approved = case.run(&["migrate"], "y\n");
        assert!(approved.status.success(), "{}", stderr(&approved));
        assert!(case.read(".gitignore").contains("/AGENTS.md"));
        if let Some(content) = existing {
            assert!(case.read(".gitignore").starts_with(content));
        }
        let sync = case.run(&["sync"], "");
        assert!(sync.status.success(), "{}", stderr(&sync));
        let rollback = case.run(&["rollback"], "");
        assert!(rollback.status.success(), "{}", stderr(&rollback));
        assert_eq!(
            fs::read_to_string(case.repo.join(".gitignore"))
                .ok()
                .as_deref(),
            existing
        );
    }
}

#[test]
fn non_git_migration_and_sync_do_not_create_gitignore() {
    let case = Case::new(Some("native"));
    let migration = case.run(&["migrate"], "y\n");
    assert!(migration.status.success(), "{}", stderr(&migration));
    assert!(!stdout(&migration).contains(".gitignore"));
    assert!(!case.repo.join(".gitignore").exists());
    let sync = case.run(&["sync"], "");
    assert!(sync.status.success(), "{}", stderr(&sync));
    assert!(!case.repo.join(".gitignore").exists());
    fs::write(case.repo.join(".gitignore"), "/user-cache/\n").unwrap();
    let repeated = case.run(&["sync"], "");
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert_eq!(case.read(".gitignore"), "/user-cache/\n");
}
