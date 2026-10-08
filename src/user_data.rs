#[cfg(not(windows))]
use crate::command_log::CommandExt;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn config_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        return Ok(PathBuf::from(env::var_os("APPDATA").ok_or("APPDATA is not set")?).join("rai"));
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg).join("rai"));
        }
        let home = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?);
        let current = home.join(".rai");
        let previous = home.join(".config/rai");
        migrate(&previous, &current)?;
        Ok(current)
    }
}

/// Resolve the transcript location without migrating configuration or running
/// Git (help and version must remain usable independently of machine setup).
pub fn logs_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        Ok(PathBuf::from(env::var_os("APPDATA").ok_or("APPDATA is not set")?).join("rai/logs"))
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg).join("rai/logs"));
        }
        Ok(PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?).join(".rai/logs"))
    }
}

#[cfg(not(windows))]
fn migrate(previous: &Path, current: &Path) -> Result<(), String> {
    if previous.is_symlink() || current.is_symlink() {
        return Err("symlink RosettAI configuration conflict".into());
    }
    if previous.exists() {
        if current.exists() {
            if !current.is_dir() || !previous.is_dir() {
                return Err("configuration migration directory conflict".into());
            }
            let entries = fs::read_dir(previous)
                .map_err(|e| e.to_string())?
                .map(|entry| entry.map(|entry| entry.path()).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            for path in &entries {
                let target = current.join(path.file_name().ok_or("invalid configuration entry")?);
                if target.exists() || target.is_symlink() {
                    return Err(format!(
                        "configuration migration conflict: {}",
                        target.display()
                    ));
                }
            }
            for path in entries {
                let target = current.join(path.file_name().ok_or("invalid configuration entry")?);
                fs::rename(path, &target).map_err(|e| e.to_string())?;
            }
            fs::remove_dir(previous).map_err(|e| e.to_string())?;
        } else {
            fs::rename(previous, current).map_err(|e| {
                format!(
                    "cannot move {} to {}: {e}",
                    previous.display(),
                    current.display()
                )
            })?;
        }
    }
    update_hook_paths(current, previous)?;
    update_git_template(previous, current)
}

#[cfg(not(windows))]
fn update_hook_paths(current: &Path, previous: &Path) -> Result<(), String> {
    let registry = current.join("installed-hooks.json");
    if !registry.exists() {
        return Ok(());
    }
    if registry.is_symlink() {
        return Err(format!(
            "symlink hook registry conflict: {}",
            registry.display()
        ));
    }
    let mut hooks: serde_json::Value =
        serde_json::from_slice(&fs::read(&registry).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let entries = hooks.as_array_mut().ok_or("invalid hook registry")?;
    let mut changed = false;
    for entry in entries {
        let Some(path) = entry.get("path").and_then(|value| value.as_str()) else {
            continue;
        };
        if let Ok(relative) = Path::new(path).strip_prefix(previous) {
            entry["path"] = current.join(relative).to_string_lossy().into_owned().into();
            changed = true;
        }
    }
    if changed {
        fs::write(
            &registry,
            serde_json::to_vec_pretty(&hooks).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn update_git_template(previous: &Path, current: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["config", "--global", "--get", "init.templateDir"])
        .output()
        .map_err(|e| e.to_string())?;
    match output.status.code() {
        Some(1) => return Ok(()),
        Some(0) => {}
        _ => return Err("could not read global init.templateDir".into()),
    }
    if String::from_utf8_lossy(&output.stdout).trim()
        == previous.join("git-template").to_string_lossy()
    {
        let status = Command::new("git")
            .args(["config", "--global", "init.templateDir"])
            .arg(current.join("git-template"))
            .logged_status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("could not update global init.templateDir".into());
        }
    }
    Ok(())
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_conflicting_files() {
        let sandbox = tempfile::tempdir().unwrap();
        let previous = sandbox.path().join(".config/rai");
        let current = sandbox.path().join(".rai");
        fs::create_dir_all(&previous).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(previous.join("roots.txt"), "old\n").unwrap();
        fs::write(current.join("roots.txt"), "new\n").unwrap();
        assert!(
            migrate(&previous, &current)
                .unwrap_err()
                .contains("conflict")
        );
        assert_eq!(
            fs::read_to_string(previous.join("roots.txt")).unwrap(),
            "old\n"
        );
        assert_eq!(
            fs::read_to_string(current.join("roots.txt")).unwrap(),
            "new\n"
        );
    }
}
