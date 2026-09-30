use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Global Copilot rule.\n",
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
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn failed(repo: &Path, expected: &str) {
    let output = run(repo, &["sync"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn body_after_marker(content: &str) -> &str {
    content
        .split_once('\n')
        .map(|(_, body)| body)
        .expect("generated file has an ownership marker")
}

#[test]
fn projects_rules_agents_mcp_and_preserves_canonical_skills() {
    let repo = repo();
    fs::create_dir_all(repo.path().join(".agents/subagents")).unwrap();
    fs::write(
        repo.path().join(".agents/subagents/reviewer.yaml"),
        "name: reviewer\ndescription: Review changes.\nmodel: gpt-6-luna\nmodel_reasoning_effort: high\nsandbox_mode: read-only\nnickname_candidates: [Atlas]\ndeveloper_instructions: |\n  Find regressions.\n  Cite affected files.\n",
    )
    .unwrap();
    fs::create_dir_all(repo.path().join(".agents/skills/review")).unwrap();
    fs::write(
        repo.path().join(".agents/skills/review/SKILL.md"),
        "---\nname: review\ndescription: Review code.\n---\nReview carefully.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".agents/mcp.json"),
        r#"{"servers":{"docs":{"transport":"http","url":"https://example.invalid/mcp","bearer_token_env_var":"DOCS_TOKEN"},"local":{"transport":"stdio","command":"npx","args":["-y","example"],"cwd":"tools","env_vars":["LOCAL_TOKEN"]}}}"#,
    )
    .unwrap();

    ok(repo.path(), &["sync"]);

    let instructions =
        fs::read_to_string(repo.path().join(".github/copilot-instructions.md")).unwrap();
    assert!(instructions.contains("# Shared project rules"));
    assert!(instructions.contains("Global Copilot rule."));

    let agent = fs::read_to_string(repo.path().join(".github/agents/reviewer.agent.md")).unwrap();
    assert!(agent.contains("name: reviewer"));
    assert!(agent.contains("description: Review changes."));
    assert!(agent.contains("target: vscode"));
    assert!(agent.contains("Find regressions.\nCite affected files."));
    for codex_only in [
        "gpt-6-luna",
        "model_reasoning_effort",
        "sandbox_mode",
        "nickname_candidates",
    ] {
        assert!(!agent.contains(codex_only), "{codex_only}");
    }

    let mcp = fs::read_to_string(repo.path().join(".vscode/mcp.json")).unwrap();
    let mcp: Value = serde_json::from_str(body_after_marker(&mcp)).unwrap();
    assert_eq!(mcp["servers"]["docs"]["type"], "http");
    assert_eq!(
        mcp["servers"]["docs"]["headers"]["Authorization"],
        "Bearer ${env:DOCS_TOKEN}"
    );
    assert_eq!(mcp["servers"]["local"]["type"], "stdio");
    assert_eq!(
        mcp["servers"]["local"]["env"]["LOCAL_TOKEN"],
        "${env:LOCAL_TOKEN}"
    );
    assert!(repo.path().join(".agents/skills/review/SKILL.md").exists());
    assert!(!repo.path().join(".github/skills/review").exists());

    let before = [
        ".github/copilot-instructions.md",
        ".github/agents/reviewer.agent.md",
        ".vscode/mcp.json",
    ]
    .map(|path| fs::read(repo.path().join(path)).unwrap());
    ok(repo.path(), &["sync"]);
    let after = [
        ".github/copilot-instructions.md",
        ".github/agents/reviewer.agent.md",
        ".vscode/mcp.json",
    ]
    .map(|path| fs::read(repo.path().join(path)).unwrap());
    assert_eq!(before, after);
    let status: Value =
        serde_json::from_slice(&ok(repo.path(), &["status", "--json"]).stdout).unwrap();
    assert_eq!(status["inSync"], true);
}

#[test]
fn scoped_rules_use_copilot_apply_to_without_leaking_into_global_rules() {
    let repo = repo();
    fs::create_dir_all(repo.path().join("frontend/components")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/frontend.md"),
        "---\npath: frontend\n---\nFrontend only.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".agents/rules/components.md"),
        "---\npath: frontend/components\n---\nComponents only.\n",
    )
    .unwrap();

    ok(repo.path(), &["sync"]);

    let global = fs::read_to_string(repo.path().join(".github/copilot-instructions.md")).unwrap();
    assert!(global.contains("Global Copilot rule."));
    assert!(!global.contains("Frontend only."));
    let frontend = fs::read_to_string(
        repo.path()
            .join(".github/instructions/frontend.instructions.md"),
    )
    .unwrap();
    assert!(frontend.contains("applyTo: frontend/**"));
    assert!(frontend.contains("Frontend only."));
    assert!(!frontend.contains("path: frontend"));
    let components = fs::read_to_string(
        repo.path()
            .join(".github/instructions/frontend__components.instructions.md"),
    )
    .unwrap();
    assert!(components.contains("applyTo: frontend/components/**"));
    assert!(components.contains("Components only."));
}

#[test]
fn removing_canonical_resources_cleans_only_owned_copilot_outputs() {
    let repo = repo();
    fs::create_dir_all(repo.path().join("frontend")).unwrap();
    let scoped = repo.path().join(".agents/rules/frontend.md");
    fs::write(&scoped, "---\npath: frontend\n---\nFrontend only.\n").unwrap();
    fs::create_dir_all(repo.path().join(".agents/subagents")).unwrap();
    let agent = repo.path().join(".agents/subagents/reviewer.yaml");
    fs::write(
        &agent,
        "name: reviewer\ndescription: Review.\ndeveloper_instructions: Review.\n",
    )
    .unwrap();
    let mcp = repo.path().join(".agents/mcp.json");
    fs::write(
        &mcp,
        r#"{"servers":{"docs":{"transport":"http","url":"https://example.invalid/mcp"}}}"#,
    )
    .unwrap();
    ok(repo.path(), &["sync"]);

    fs::remove_file(repo.path().join(".agents/rules/general.md")).unwrap();
    fs::remove_file(scoped).unwrap();
    fs::create_dir_all(repo.path().join("backend")).unwrap();
    fs::write(
        repo.path().join(".agents/rules/backend.md"),
        "---\npath: backend\n---\nBackend only.\n",
    )
    .unwrap();
    fs::remove_file(agent).unwrap();
    fs::remove_file(mcp).unwrap();
    fs::write(
        repo.path().join(".github/agents/user.agent.md"),
        "User-owned agent.\n",
    )
    .unwrap();

    ok(repo.path(), &["sync"]);
    for path in [
        ".github/copilot-instructions.md",
        ".github/instructions/frontend.instructions.md",
        ".github/agents/reviewer.agent.md",
        ".vscode/mcp.json",
    ] {
        assert!(!repo.path().join(path).exists(), "stale {path}");
    }
    assert_eq!(
        fs::read_to_string(repo.path().join(".github/agents/user.agent.md")).unwrap(),
        "User-owned agent.\n"
    );
    let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    for path in [
        ".github/copilot-instructions.md",
        ".github/instructions/frontend.instructions.md",
        ".github/agents/reviewer.agent.md",
        ".vscode/mcp.json",
    ] {
        assert!(!ignore.contains(path), "stale ignore entry for {path}");
    }
}

#[test]
fn copilot_collisions_abort_before_any_projection_is_written() {
    for path in [
        ".github/copilot-instructions.md",
        ".github/agents/reviewer.agent.md",
        ".vscode/mcp.json",
    ] {
        let repo = repo();
        if path.contains("reviewer") {
            fs::create_dir_all(repo.path().join(".agents/subagents")).unwrap();
            fs::write(
                repo.path().join(".agents/subagents/reviewer.yaml"),
                "name: reviewer\ndescription: Review.\ndeveloper_instructions: Review.\n",
            )
            .unwrap();
        }
        if path.ends_with("mcp.json") {
            fs::write(
                repo.path().join(".agents/mcp.json"),
                r#"{"servers":{"docs":{"transport":"http","url":"https://example.invalid/mcp"}}}"#,
            )
            .unwrap();
        }
        let collision = repo.path().join(path);
        fs::create_dir_all(collision.parent().unwrap()).unwrap();
        fs::write(&collision, "User-owned content.\n").unwrap();

        failed(repo.path(), "unowned or modified output conflict");
        assert_eq!(
            fs::read_to_string(collision).unwrap(),
            "User-owned content.\n"
        );
        assert!(!repo.path().join("AGENTS.md").exists());
        assert!(!repo.path().join(".codex/config.toml").exists());
        assert!(!repo.path().join(".gitignore").exists());
    }
}

#[test]
fn modified_copilot_projection_is_never_overwritten() {
    let repo = repo();
    ok(repo.path(), &["sync"]);
    let path = repo.path().join(".github/copilot-instructions.md");
    let edited = format!("{}Local edit.\n", fs::read_to_string(&path).unwrap());
    fs::write(&path, &edited).unwrap();
    fs::write(
        repo.path().join(".agents/rules/general.md"),
        "Changed canonical rule.\n",
    )
    .unwrap();

    failed(repo.path(), "unowned or modified output conflict");
    assert_eq!(fs::read_to_string(path).unwrap(), edited);
}
