use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn registry_path() -> Result<PathBuf, String> {
    Ok(crate::user_data::config_dir()?.join("installed-hooks.json"))
}

fn read(path: &Path) -> Result<Vec<Value>, String> {
    if path.is_symlink() {
        return Err(format!(
            "symlink hook registry conflict: {}",
            path.display()
        ));
    }
    if !path.exists() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    value
        .as_array()
        .cloned()
        .ok_or_else(|| format!("invalid hook registry: {}", path.display()))
}

fn write(path: &Path, entries: &[Value]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("json.tmp");
    if temp.is_symlink() {
        return Err(format!(
            "symlink hook registry conflict: {}",
            temp.display()
        ));
    }
    fs::write(
        &temp,
        serde_json::to_vec_pretty(entries).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

pub fn remember_git_template(path: &Path, event: &str) -> Result<(), String> {
    let registry = registry_path()?;
    let mut entries = read(&registry)?;
    let path = path.to_string_lossy();
    if !entries.iter().any(|entry| entry["path"] == path.as_ref()) {
        entries.push(json!({"kind": "git-template", "event": event, "path": path}));
        write(&registry, &entries)?;
    }
    Ok(())
}

pub fn forget(path: &Path) -> Result<(), String> {
    let registry = registry_path()?;
    let mut entries = read(&registry)?;
    let before = entries.len();
    entries.retain(|entry| entry["path"] != path.to_string_lossy().as_ref());
    if entries.len() != before {
        write(&registry, &entries)?;
    }
    Ok(())
}
