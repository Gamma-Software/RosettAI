use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Global rule.\n",
    )
    .unwrap();
    repo
}

fn run(repo: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(args)
        .arg("--repo")
        .arg(repo)
        .output()
        .unwrap()
}

fn ok(repo: &Path, args: &[&str]) -> Output {
    let output = run(repo, args);
    assert!(
        output.status.success(),
        "{}: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn failed(repo: &Path, args: &[&str], expected: &str) {
    let output = run(repo, args);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture_files(root: &Path) -> Vec<PathBuf> {
    fn visit(root: &Path, current: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    let mut files = Vec::new();
    visit(root, root, &mut files);
    files.sort();
    files
}

#[test]
fn example_project_matches_expected_tree_byte_for_byte() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/integration/fixtures/codex-sync");
    let input = fixture.join("input");
    let expected = fixture.join("expected");
    assert_eq!(
        fs::read_dir(&input).unwrap().count(),
        1,
        "the input example must contain only .agents/"
    );
    assert!(input.join(".agents").is_dir());
    let repo = tempfile::tempdir().unwrap();
    for relative in fixture_files(&input) {
        let target = repo.path().join(&relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(input.join(relative), target).unwrap();
    }
    ok(repo.path(), &["sync"]);
    let actual_files = fixture_files(repo.path());
    let expected_files = fixture_files(&expected);
    let expected_project_files = expected_files
        .iter()
        .map(|path| {
            if path == Path::new(".gitignore.expected") {
                PathBuf::from(".gitignore")
            } else {
                path.clone()
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual_files, expected_project_files,
        "generated project tree differs"
    );
    for relative in expected_files {
        let projected = if relative == Path::new(".gitignore.expected") {
            Path::new(".gitignore")
        } else {
            &relative
        };
        let actual = fs::read(repo.path().join(projected)).unwrap();
        let wanted = fs::read(expected.join(&relative)).unwrap();
        assert_eq!(
            actual,
            wanted,
            "content differs for {}\nactual:\n{}\nexpected:\n{}",
            relative.display(),
            String::from_utf8_lossy(&actual),
            String::from_utf8_lossy(&wanted)
        );
    }
}

#[test]
fn full_codex_projection_and_idempotence() {
    let repo = repo();
    fs::create_dir_all(repo.path().join(".agents/skills/review")).unwrap();
    fs::write(
        repo.path().join(".agents/skills/review/SKILL.md"),
        "---\nname: review\ndescription: Review code.\n---\nReview carefully.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".agents/mcp.json"),
        r#"{"servers":{"docs":{"transport":"http","url":"https://example.invalid/mcp","bearer_token_env_var":"DOCS_TOKEN"},"local":{"transport":"stdio","command":"npx","args":["-y","example"],"cwd":"tools","env_vars":["LOCAL_TOKEN"],"default_tools_approval_mode":"writes"}}}"#,
    )
    .unwrap();

    let preview = ok(repo.path(), &["sync", "--dry-run", "--json"]);
    let preview: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["ok"], true);
    assert!(!repo.path().join("AGENTS.md").exists());
    assert!(!repo.path().join(".gitignore").exists());

    ok(repo.path(), &["sync"]);
    let instructions = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    assert!(instructions.contains("Global rule."));
    let config = fs::read_to_string(repo.path().join(".codex/config.toml")).unwrap();
    let config: toml::Value = config.parse().unwrap();
    assert_eq!(
        config["mcp_servers"]["docs"]["url"].as_str(),
        Some("https://example.invalid/mcp")
    );
    assert_eq!(
        config["mcp_servers"]["local"]["command"].as_str(),
        Some("npx")
    );
    assert_eq!(
        config["mcp_servers"]["local"]["default_tools_approval_mode"].as_str(),
        Some("writes")
    );
    assert_eq!(
        config["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"].as_str(),
        Some("rai sync --codex-hook")
    );
    assert!(!repo.path().join(".codex/skills/review").exists());
    assert!(repo.path().join(".agents/skills/review/SKILL.md").exists());

    let before = fs::read(repo.path().join("AGENTS.md")).unwrap();
    ok(repo.path(), &["sync"]);
    assert_eq!(before, fs::read(repo.path().join("AGENTS.md")).unwrap());
    let status = ok(repo.path(), &["status", "--json"]);
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["inSync"], true);
}

#[test]
fn flat_rules_group_by_explicit_directory_and_strip_frontmatter() {
    let repo = repo();
    fs::create_dir_all(repo.path().join("frontend/components")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/root-extra.md"),
        "---\npath: .\n---\nExtra global rule.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".agents/rules/frontend.md"),
        "---\npath: frontend\n---\nFrontend rule.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".agents/rules/frontend-extra.md"),
        "---\npath: frontend\n---\nExtra frontend rule.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".agents/rules/components.md"),
        "---\npath: frontend/components\n---\nComponent rule.\n",
    )
    .unwrap();

    ok(repo.path(), &["sync"]);
    let root = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    let frontend = fs::read_to_string(repo.path().join("frontend/AGENTS.md")).unwrap();
    let components = fs::read_to_string(repo.path().join("frontend/components/AGENTS.md")).unwrap();
    assert!(root.contains("Global rule.") && root.contains("Extra global rule."));
    assert!(!root.contains("Frontend rule."));
    assert!(frontend.contains("Frontend rule.") && frontend.contains("Extra frontend rule."));
    assert!(!frontend.contains("Component rule."));
    assert!(components.contains("Component rule."));
    assert!(!frontend.contains("path: frontend"));
    let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    for path in [
        "/AGENTS.md",
        "/frontend/AGENTS.md",
        "/frontend/components/AGENTS.md",
    ] {
        assert!(ignore.contains(path), "{path}");
    }
}

#[test]
fn scoped_only_repository_creates_no_root_instructions() {
    let repo = repo();
    fs::remove_file(repo.path().join(".agents/rules/general.md")).unwrap();
    fs::create_dir(repo.path().join("frontend")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/frontend.md"),
        "---\npath: frontend\n---\nOnly frontend.\n",
    )
    .unwrap();
    ok(repo.path(), &["sync"]);
    assert!(!repo.path().join("AGENTS.md").exists());
    assert!(
        fs::read_to_string(repo.path().join("frontend/AGENTS.md"))
            .unwrap()
            .contains("Only frontend.")
    );
}

#[test]
fn rule_move_cleans_only_owned_outputs() {
    let repo = repo();
    fs::create_dir_all(repo.path().join("frontend")).unwrap();
    fs::write(
        repo.path().join(".gitignore"),
        "# user entry\n/local.cache\n",
    )
    .unwrap();
    let rule = repo.path().join(".agents/rules/feature.md");
    fs::write(&rule, "---\npath: frontend\n---\nFeature rule.\n").unwrap();
    ok(repo.path(), &["sync"]);

    fs::write(&rule, "Moved into global guidance.\n").unwrap();
    let preview = ok(repo.path(), &["sync", "--dry-run"]);
    let preview = String::from_utf8_lossy(&preview.stdout);
    assert!(preview.contains("Delete frontend/AGENTS.md"));
    assert!(repo.path().join("frontend/AGENTS.md").exists());
    ok(repo.path(), &["sync"]);
    assert!(!repo.path().join("frontend/AGENTS.md").exists());
    assert!(
        fs::read_to_string(repo.path().join("AGENTS.md"))
            .unwrap()
            .contains("Moved into global guidance.")
    );
    let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(ignore.starts_with("# user entry\n/local.cache\n"));
    assert!(!ignore.contains("/frontend/AGENTS.md"));
}

#[test]
fn collisions_fail_before_any_projection_is_written() {
    for path in ["AGENTS.md", "frontend/AGENTS.md", ".codex/config.toml"] {
        let repo = repo();
        fs::create_dir_all(repo.path().join("frontend")).unwrap();
        fs::write(
            repo.path().join(".agents/rules/frontend.md"),
            "---\npath: frontend\n---\nFrontend rule.\n",
        )
        .unwrap();
        let output = repo.path().join(path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&output, "User-owned content\n").unwrap();
        failed(
            repo.path(),
            &["sync"],
            "unowned or modified output conflict",
        );
        assert_eq!(fs::read_to_string(&output).unwrap(), "User-owned content\n");
        assert!(!repo.path().join(".gitignore").exists());
    }
}

#[test]
fn invalid_canonical_resources_fail_without_partial_writes() {
    let cases = [
        (
            ".agents/rules/bad.md",
            "---\npath: ../outside\n---\nUnsafe.\n",
            "invalid rule path",
        ),
        (
            ".agents/rules/bad.md",
            "---\npath: missing\n---\nRule.\n",
            "scoped rule target",
        ),
        (
            ".agents/mcp.json",
            r#"{"servers":{"x":{"transport":"http","url":"https://example.invalid","secret":"inline"}}}"#,
            "unsupported MCP field",
        ),
        (
            ".agents/subagents/other.yaml",
            "name: wrong\ndescription: Agent\ndeveloper_instructions: Work\n",
            "subagent name must match filename",
        ),
        (
            ".agents/subagents/other.json",
            r#"{"name":"other","description":"Agent","developer_instructions":"Work"}"#,
            "only regular .yaml subagents",
        ),
        (
            ".agents/skills/bad/SKILL.md",
            "No frontmatter\n",
            "invalid skill frontmatter",
        ),
    ];
    for (path, content, error) in cases {
        let repo = repo();
        let source = repo.path().join(path);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(source, content).unwrap();
        failed(repo.path(), &["sync"], error);
        assert!(!repo.path().join("AGENTS.md").exists(), "{path}");
        assert!(!repo.path().join(".codex/config.toml").exists(), "{path}");
        assert!(!repo.path().join(".gitignore").exists(), "{path}");
    }
}

#[test]
fn prompt_guard_blocks_on_changes_and_allows_clean_prompt() {
    let repo = repo();
    let first = ok(repo.path(), &["sync", "--codex-hook"]);
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["decision"], "block");
    assert!(
        first["reason"]
            .as_str()
            .unwrap()
            .contains("new Codex session")
    );
    let second = ok(repo.path(), &["sync", "--codex-hook"]);
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["continue"], true);
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Changed global rule.\n",
    )
    .unwrap();
    let third = ok(repo.path(), &["sync", "--codex-hook"]);
    let third: Value = serde_json::from_slice(&third.stdout).unwrap();
    assert_eq!(third["decision"], "block");
    assert!(
        fs::read_to_string(repo.path().join("AGENTS.md"))
            .unwrap()
            .contains("Changed global rule.")
    );
}

#[test]
fn tracked_native_output_is_never_overwritten() {
    let repo = repo();
    fs::write(repo.path().join("AGENTS.md"), "Tracked user guidance.\n").unwrap();
    let git = Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(git.status.success());
    let add = Command::new("git")
        .args(["add", "AGENTS.md"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(add.status.success());
    failed(repo.path(), &["sync"], "tracked output conflict: AGENTS.md");
    assert_eq!(
        fs::read_to_string(repo.path().join("AGENTS.md")).unwrap(),
        "Tracked user guidance.\n"
    );
    assert!(!repo.path().join(".codex/config.toml").exists());
}

#[test]
fn modified_owned_output_is_preserved() {
    let repo = repo();
    ok(repo.path(), &["sync"]);
    let path = repo.path().join("AGENTS.md");
    let edited = format!("{}Local addition.\n", fs::read_to_string(&path).unwrap());
    fs::write(&path, &edited).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Changed canonical rule.\n",
    )
    .unwrap();
    failed(
        repo.path(),
        &["sync"],
        "unowned or modified output conflict",
    );
    assert_eq!(fs::read_to_string(path).unwrap(), edited);
}

#[cfg(unix)]
#[test]
fn symlinked_scope_is_rejected_without_writing_outside_repo() {
    use std::os::unix::fs::symlink;

    let repo = repo();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), repo.path().join("frontend")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/frontend.md"),
        "---\npath: frontend\n---\nFrontend rule.\n",
    )
    .unwrap();
    failed(repo.path(), &["sync"], "scoped rule target");
    assert!(!outside.path().join("AGENTS.md").exists());
    assert!(!repo.path().join("AGENTS.md").exists());
}

#[cfg(unix)]
#[test]
fn codex_version_gate_prevents_projection() {
    use std::os::unix::fs::PermissionsExt;

    let repo = repo();
    let bin = repo.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let stub = bin.join("codex");
    fs::write(&stub, "#!/bin/sh\necho 'codex-cli 0.151.0'\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let old = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--repo"])
        .arg(repo.path())
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(!old.status.success());
    assert!(String::from_utf8_lossy(&old.stderr).contains("older than the validated minimum"));
    assert!(!repo.path().join("AGENTS.md").exists());
    fs::write(&stub, "#!/bin/sh\necho 'codex-cli 0.152.1'\n").unwrap();
    let supported = Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(["sync", "--repo"])
        .arg(repo.path())
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        supported.status.success(),
        "{}",
        String::from_utf8_lossy(&supported.stderr)
    );
    assert!(repo.path().join("AGENTS.md").exists());
}
