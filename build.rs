use std::env;
use std::path::Path;
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn watch(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

fn main() {
    println!("cargo:rerun-if-env-changed=RAI_BUILD_SHA");
    let root = std::path::PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    for path in ["build.rs", "Cargo.toml", "Cargo.lock", "src"] {
        watch(&root.join(path));
    }

    // Archives and Docker contexts may omit .git. Do not pick up an enclosing
    // repository's unrelated HEAD when building such a source copy.
    let has_git = root.join(".git").exists();
    let commit = env::var("RAI_BUILD_SHA")
        .ok()
        .inspect(|sha| {
            assert!(
                matches!(sha.len(), 40 | 64) && sha.bytes().all(|b| b.is_ascii_hexdigit()),
                "RAI_BUILD_SHA must be a full Git commit SHA"
            );
        })
        .or_else(|| {
            has_git
                .then(|| git(&root, &["rev-parse", "HEAD"]))
                .flatten()
        })
        .unwrap_or_else(|| "unknown".into());
    let dirty = has_git
        && git(
            &root,
            &["status", "--porcelain", "--untracked-files=normal"],
        )
        .is_some_and(|status| !status.is_empty());

    if has_git {
        if root.join(".git").is_file() {
            watch(&root.join(".git"));
        }
        // Resolve through Git for both ordinary checkouts and linked worktrees.
        let mut metadata = vec!["HEAD".to_owned(), "index".into(), "packed-refs".into()];
        if let Some(reference) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
            metadata.push(reference);
        }
        for name in metadata {
            if let Some(path) = git(&root, &["rev-parse", "--git-path", &name]) {
                let path = root.join(path);
                // Watching a missing path would rerun the build on every invocation.
                if path.exists() {
                    watch(&path);
                } else if name != "packed-refs"
                    && let Some(parent) = path.parent()
                {
                    watch(parent);
                }
            }
        }
        if let Some(files) = git(
            &root,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        ) {
            for path in files.split('\0').filter(|path| !path.is_empty()) {
                watch(&root.join(path));
            }
        }
    }
    println!("cargo:rustc-env=RAI_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RAI_BUILD_DIRTY={dirty}");
}
