use std::fs;
use std::path::Path;
use std::process::Command;

fn run(repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rai"))
        .args(args)
        .arg("--repo")
        .arg(repo)
        .output()
        .unwrap()
}

#[test]
fn projects_scoped_rules_and_cleans_stale_owned_files() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::create_dir_all(repo.path().join("frontend")).unwrap();
    fs::write(repo.path().join(".agents/rules/general.md"), "Global.\n").unwrap();
    let scoped = repo.path().join(".agents/rules/front.md");
    fs::write(&scoped, "---\npath: frontend\n---\nFrontend.\n").unwrap();
    assert!(run(repo.path(), &["sync", "--dry-run"]).status.success());
    assert!(!repo.path().join("CLAUDE.md").exists());
    assert!(run(repo.path(), &["sync"]).status.success());
    let rule = repo.path().join(".claude/rules/frontend.md");
    assert!(fs::read_to_string(&rule).unwrap().contains("frontend/**"));
    assert!(
        !fs::read_to_string(repo.path().join("CLAUDE.md"))
            .unwrap()
            .contains("Frontend.")
    );
    fs::remove_file(scoped).unwrap();
    assert!(run(repo.path(), &["sync"]).status.success());
    assert!(!rule.exists());
}

#[test]
fn unowned_claude_file_aborts_without_partial_writes() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(repo.path().join(".agents/rules/general.md"), "Global.\n").unwrap();
    fs::write(repo.path().join("CLAUDE.md"), "My instructions\n").unwrap();
    let result = run(repo.path(), &["sync"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CLAUDE.md"));
    assert!(!repo.path().join("AGENTS.md").exists());
    assert_eq!(
        fs::read_to_string(repo.path().join("CLAUDE.md")).unwrap(),
        "My instructions\n"
    );
}

#[test]
fn rejects_skill_attachments_before_writing() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".agents/rules")).unwrap();
    fs::write(repo.path().join(".agents/rules/general.md"), "Global.\n").unwrap();
    let skill = repo.path().join(".agents/skills/example");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: example\ndescription: Example.\n---\nBody\n",
    )
    .unwrap();
    fs::write(skill.join("data.txt"), "attachment").unwrap();
    let result = run(repo.path(), &["sync"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("skill attachment"));
    assert!(!repo.path().join("AGENTS.md").exists());
}
