use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Backup {
    pub path: PathBuf,
    original: String,
}

impl Backup {
    pub fn plan(root: &Path, source: &Path, original: String) -> Result<Self, String> {
        // Planning must not create files or migrate user data, including in dry runs.
        #[cfg(windows)]
        let base = PathBuf::from(env::var_os("APPDATA").ok_or("APPDATA is not set")?).join("rai");
        #[cfg(not(windows))]
        let base = match env::var_os("XDG_CONFIG_HOME") {
            Some(path) => PathBuf::from(path).join("rai"),
            None => PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?).join(".rai"),
        };
        // Resolve the caller-selected home/XDG directory (e.g. macOS /var
        // aliases), while rejecting links in the rai-owned backup subtree.
        let base_parent = base.parent().ok_or("invalid user-data directory")?;
        let base = base_parent
            .canonicalize()
            .unwrap_or_else(|_| base_parent.to_path_buf())
            .join(base.file_name().ok_or("invalid user-data directory")?);
        let project = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
        let digest = format!("{:x}", Sha256::digest(original.as_bytes()));
        let path = base
            .join("projection-backups")
            .join(project)
            .join(digest)
            .join(source.strip_prefix(root).map_err(|e| e.to_string())?);
        if !path.is_absolute() || path.starts_with(root) {
            return Err("projection backups must be outside the repository in an absolute user-data directory".into());
        }
        Ok(Self { path, original })
    }

    pub fn save(&self, source: &Path) -> Result<(), String> {
        regular_path(source)?;
        if fs::read(source).map_err(|e| e.to_string())? != self.original.as_bytes() {
            return Err(format!(
                "{} changed during sync; rerun rai sync",
                source.display()
            ));
        }
        regular_path(&self.path)?;
        fs::create_dir_all(self.path.parent().ok_or("invalid backup path")?).map_err(|e| {
            format!(
                "cannot create projection backup {}: {e}",
                self.path.display()
            )
        })?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&self.path) {
            Ok(mut file) => {
                file.write_all(self.original.as_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|e| {
                        format!("cannot save projection backup {}: {e}", self.path.display())
                    })?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                regular_path(&self.path)?;
                if fs::read(&self.path).map_err(|e| e.to_string())? != self.original.as_bytes() {
                    return Err(format!(
                        "projection backup conflict: {}",
                        self.path.display()
                    ));
                }
            }
            Err(error) => {
                return Err(format!(
                    "cannot save projection backup {}: {error}",
                    self.path.display()
                ));
            }
        }
        Ok(())
    }
}

fn regular_path(path: &Path) -> Result<(), String> {
    if path.ancestors().any(Path::is_symlink) {
        return Err(format!(
            "symlink projection backup conflict: {}",
            path.display()
        ));
    }
    Ok(())
}

pub fn has_marker(content: &str, kind: &str) -> bool {
    let valid_hash = |hash: &str| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit());
    match kind {
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .ok()
            .and_then(|v| v.get("_rai_generated_sha256")?.as_str().map(str::to_owned))
            .is_some_and(|hash| valid_hash(&hash)),
        "hash" | "jsonc" => content
            .strip_prefix(if kind == "hash" {
                "# rai-generated sha256:"
            } else {
                "// rai-generated sha256:"
            })
            .and_then(|s| s.split_once('\n'))
            .is_some_and(|(hash, _)| valid_hash(hash)),
        _ => content
            .strip_prefix("<!-- rai-generated sha256:")
            .and_then(|s| s.split_once(" -->\n"))
            .is_some_and(|(hash, _)| valid_hash(hash)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_preserves_bytes_refuses_collisions_and_source_changes() {
        let dir = tempfile::tempdir().unwrap();
        let directory = dir.path().canonicalize().unwrap();
        let source = directory.join("source");
        let backup = Backup {
            path: directory.join("saved/AGENTS.md"),
            original: "local\r\nedit\n".into(),
        };
        fs::write(&source, &backup.original).unwrap();
        backup.save(&source).unwrap();
        backup.save(&source).unwrap();
        assert_eq!(fs::read(&backup.path).unwrap(), backup.original.as_bytes());
        fs::write(&backup.path, "other").unwrap();
        assert!(
            backup
                .save(&source)
                .unwrap_err()
                .contains("backup conflict")
        );
        fs::write(&source, "new edit").unwrap();
        assert!(
            backup
                .save(&source)
                .unwrap_err()
                .contains("changed during sync")
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), "new edit");
    }

    #[test]
    fn malformed_markers_are_not_recoverable() {
        let hash = "a".repeat(64);
        for (kind, marked) in [
            (
                "markdown",
                format!("<!-- rai-generated sha256:{hash} -->\nchanged"),
            ),
            ("hash", format!("# rai-generated sha256:{hash}\nchanged")),
            ("jsonc", format!("// rai-generated sha256:{hash}\nchanged")),
            (
                "json",
                format!("{{\"_rai_generated_sha256\":\"{hash}\",\"edited\":true}}"),
            ),
        ] {
            assert!(has_marker(&marked, kind));
            assert!(!has_marker(&marked.replace(&hash, "invalid"), kind));
            assert!(!has_marker("plain user content", kind));
        }
        assert!(!has_marker("{\"_rai_generated_sha256\":\"", "json"));
    }

    #[cfg(unix)]
    #[test]
    fn backup_refuses_linked_sources_and_destinations() {
        let dir = tempfile::tempdir().unwrap();
        let directory = dir.path().canonicalize().unwrap();
        let source = directory.join("source");
        fs::write(&source, "keep").unwrap();
        let backup = Backup {
            path: directory.join("link/saved"),
            original: "keep".into(),
        };
        std::os::unix::fs::symlink(dir.path(), dir.path().join("link")).unwrap();
        assert!(backup.save(&source).unwrap_err().contains("symlink"));
        assert!(!dir.path().join("saved").exists());
    }
}
