use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SCAN_DIRS: usize = 20_000;
const MAX_DEPTH: usize = 9;

pub fn remember(config: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let mut identities = read_identities(config)?;
    for root in roots {
        if let Some(id) = identity(root) {
            identities.insert(root.to_string_lossy().into_owned(), id);
        }
    }
    write_identities(config, &identities)
}

pub fn change_root(config: &Path, old: &Path, replacement: Option<&Path>) -> Result<(), String> {
    let roots_file = config.join("roots.txt");
    if roots_file.is_symlink() {
        return Err(format!(
            "symlink configuration conflict: {}",
            roots_file.display()
        ));
    }
    let body = fs::read_to_string(&roots_file).map_err(|e| e.to_string())?;
    let mut roots = body
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if !roots.iter().any(|root| root == old) {
        return Err(format!(
            "workspace is no longer configured: {}",
            old.display()
        ));
    }
    let mut identities = read_identities(config)?;
    identities.remove(&old.to_string_lossy().into_owned());
    roots.retain(|root| root != old);
    if let Some(path) = replacement {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        let id = identity(&path).ok_or(format!("not a regular directory: {}", path.display()))?;
        identities.insert(path.to_string_lossy().into_owned(), id);
        roots.push(path);
    }
    roots.sort();
    roots.dedup();
    write_roots(config, &roots, &[])?;
    write_identities(config, &identities)
}

pub fn resolve(config: &Path, roots: &[PathBuf]) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    resolve_with_search(config, roots, true)
}

pub fn resolve_with_search(
    config: &Path,
    roots: &[PathBuf],
    search: bool,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let mut identities = read_identities(config)?;
    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    let mut changed = false;
    for root in roots {
        let key = root.to_string_lossy().into_owned();
        let expected = identities.get(&key).cloned();
        if root.is_dir()
            && !root.is_symlink()
            && expected
                .as_ref()
                .is_none_or(|id| identity(root).as_ref() == Some(id))
        {
            if expected.is_none()
                && let Some(id) = identity(root)
            {
                identities.insert(key, id);
                changed = true;
            }
            resolved.push(root.clone());
            continue;
        }
        let found = expected
            .as_ref()
            .filter(|_| search)
            .and_then(|id| find_moved(root, id));
        if let Some(found) = found {
            let new_key = found.to_string_lossy().into_owned();
            identities.remove(&key);
            identities.insert(new_key, expected.unwrap());
            resolved.push(found);
            changed = true;
        } else {
            missing.push(root.clone());
        }
    }
    resolved.sort();
    resolved.dedup();
    if changed {
        write_identities(config, &identities)?;
        write_roots(config, &resolved, &missing)?;
    }
    Ok((resolved, missing))
}

fn read_identities(config: &Path) -> Result<BTreeMap<String, String>, String> {
    let path = config.join("root-identities.json");
    if path.is_symlink() {
        return Err(format!(
            "symlink configuration conflict: {}",
            path.display()
        ));
    }
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let value: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let object = value
        .as_object()
        .ok_or("root-identities.json must be an object")?;
    object
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|id| (key.clone(), id.to_owned()))
                .ok_or("root-identities.json contains a non-string identity".to_owned())
        })
        .collect()
}

fn write_identities(config: &Path, ids: &BTreeMap<String, String>) -> Result<(), String> {
    let path = config.join("root-identities.json");
    if path.is_symlink() {
        return Err(format!(
            "symlink configuration conflict: {}",
            path.display()
        ));
    }
    let body = serde_json::to_vec_pretty(ids).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| e.to_string())
}

fn write_roots(config: &Path, resolved: &[PathBuf], missing: &[PathBuf]) -> Result<(), String> {
    let path = config.join("roots.txt");
    if path.is_symlink() {
        return Err(format!(
            "symlink configuration conflict: {}",
            path.display()
        ));
    }
    let mut all = resolved
        .iter()
        .chain(missing)
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    all.sort();
    fs::write(path, format!("{}\n", all.join("\n"))).map_err(|e| e.to_string())
}

fn find_moved(old: &Path, expected: &str) -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Some(parent) = old.parent() {
        bases.push(parent.to_path_buf());
    }
    if let Some(grandparent) = old.parent().and_then(Path::parent) {
        bases.push(grandparent.to_path_buf());
    }
    let home = if cfg!(windows) {
        env::var_os("USERPROFILE")
    } else {
        env::var_os("HOME")
    };
    if let Some(home) = home {
        bases.push(PathBuf::from(home));
    }
    bases.retain(|base| base.is_dir() && !base.is_symlink() && base.components().count() > 1);
    bases.sort_by_key(|base| std::cmp::Reverse(base.components().count()));
    bases.dedup();
    // Search each scope only once, and accept only one matching directory.
    let mut visited = HashSet::new();
    let mut scanned = 0;
    for base in bases {
        let mut matches = Vec::new();
        let mut stack = vec![(base, 0)];
        while let Some((path, depth)) = stack.pop() {
            if !visited.insert(path.clone()) || scanned >= MAX_SCAN_DIRS {
                continue;
            }
            scanned += 1;
            if identity(&path).as_deref() == Some(expected) {
                matches.push(path.clone());
            }
            if depth >= MAX_DEPTH {
                continue;
            }
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let child = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.')
                    || matches!(name.as_ref(), "node_modules" | "target" | "vendor")
                {
                    continue;
                }
                if entry
                    .file_type()
                    .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
                {
                    stack.push((child, depth + 1));
                }
            }
        }
        if matches.len() == 1 {
            return matches.pop();
        }
        if matches.len() > 1 {
            return None;
        }
    }
    None
}

#[cfg(unix)]
fn identity(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return None;
    }
    Some(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn identity(path: &Path) -> Option<String> {
    use std::os::windows::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return None;
    }
    let volume = path
        .components()
        .next()?
        .as_os_str()
        .to_string_lossy()
        .to_ascii_lowercase();
    // Windows' stable directory creation timestamp distinguishes candidates on the same volume.
    // A collision is rejected by find_moved rather than selecting an arbitrary directory.
    Some(format!("windows:{volume}:{}", metadata.creation_time()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_a_moved_workspace_and_keeps_unresolved_roots() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        fs::create_dir(&config).unwrap();
        let first = dir.path().join("first");
        let moved = dir.path().join("moved");
        let absent = dir.path().join("absent");
        fs::create_dir(&first).unwrap();
        remember(&config, std::slice::from_ref(&first)).unwrap();
        write_roots(
            &config,
            std::slice::from_ref(&first),
            std::slice::from_ref(&absent),
        )
        .unwrap();
        fs::rename(&first, &moved).unwrap();
        let (resolved, missing) = resolve(&config, &[first.clone(), absent.clone()]).unwrap();
        assert_eq!(resolved, vec![moved.clone()]);
        assert_eq!(missing, vec![absent.clone()]);
        let saved = fs::read_to_string(config.join("roots.txt")).unwrap();
        assert!(saved.contains(&moved.to_string_lossy().to_string()));
        assert!(!saved.contains(&first.to_string_lossy().to_string()));
        assert!(saved.contains(&absent.to_string_lossy().to_string()));
        assert!(resolve(&config, &[moved, absent]).unwrap().1.len() == 1);
    }

    #[test]
    fn replaces_and_removes_a_missing_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        fs::create_dir(&config).unwrap();
        let old = dir.path().join("old");
        let new = dir.path().join("new");
        fs::create_dir(&old).unwrap();
        fs::create_dir(&new).unwrap();
        let new = new.canonicalize().unwrap();
        remember(&config, std::slice::from_ref(&old)).unwrap();
        write_roots(&config, std::slice::from_ref(&old), &[]).unwrap();
        fs::remove_dir(&old).unwrap();
        change_root(&config, &old, Some(&new)).unwrap();
        assert_eq!(
            fs::read_to_string(config.join("roots.txt")).unwrap(),
            format!("{}\n", new.display())
        );
        assert!(
            read_identities(&config)
                .unwrap()
                .contains_key(&new.to_string_lossy().into_owned())
        );
        assert!(
            !read_identities(&config)
                .unwrap()
                .contains_key(&old.to_string_lossy().into_owned())
        );
        change_root(&config, &new, None).unwrap();
        assert_eq!(fs::read_to_string(config.join("roots.txt")).unwrap(), "\n");
        assert!(read_identities(&config).unwrap().is_empty());
    }
}
