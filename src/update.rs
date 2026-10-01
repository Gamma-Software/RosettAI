use serde_json::Value;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const API_URL: &str = "https://api.github.com/repos/Gamma-Software/RosettAI/releases/latest";
const CACHE_AGE: Duration = Duration::from_secs(60 * 60);
const DOWNLOAD_ROOT: &str = "https://github.com/Gamma-Software/RosettAI/releases/download";

fn version(tag: &str) -> Result<[u64; 3], String> {
    let value = tag.strip_prefix('v').unwrap_or(tag);
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("invalid release version: {tag}"));
    }
    let numbers = parts
        .iter()
        .map(|part| {
            part.parse::<u64>()
                .map_err(|_| format!("invalid release version: {tag}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok([numbers[0], numbers[1], numbers[2]])
}

fn latest_tag(body: &[u8]) -> Result<String, String> {
    let release: Value = serde_json::from_slice(body)
        .map_err(|e| format!("invalid GitHub release response: {e}"))?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or("GitHub release response has no tag_name")?;
    version(tag)?;
    Ok(tag.to_owned())
}

fn fetch_latest(timeout: u8) -> Result<String, String> {
    let response = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--max-time",
            &timeout.to_string(),
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "User-Agent: rai",
            API_URL,
        ])
        .output()
        .map_err(|e| format!("cannot run curl to check for updates: {e}"))?;
    if !response.status.success() {
        return Err(format!(
            "cannot check GitHub Releases (curl exit status {}). Check your connection or try again later",
            response.status
        ));
    }
    latest_tag(&response.stdout)
}

fn cache_path() -> Option<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(xdg).join("rai/latest-release"));
    }
    #[cfg(windows)]
    {
        env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("rai/latest-release"))
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(|home| PathBuf::from(home).join(".rai/cache/latest-release"))
    }
}

fn cached_latest() -> Option<String> {
    let path = cache_path()?;
    let age = fs::metadata(&path).ok()?.modified().ok()?.elapsed().ok()?;
    if age > CACHE_AGE {
        return None;
    }
    let tag = fs::read_to_string(path).ok()?;
    version(tag.trim()).ok()?;
    Some(tag.trim().to_owned())
}

fn store_latest(tag: &str) {
    let Some(path) = cache_path() else { return };
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension(format!("{}", std::process::id()));
    if fs::write(&temporary, tag).is_ok() {
        let _ = fs::rename(&temporary, path);
    }
}

pub fn automatic_warning() {
    let latest = cached_latest().or_else(|| {
        let tag = fetch_latest(2).ok()?;
        store_latest(&tag);
        Some(tag)
    });
    let Some(latest) = latest else { return };
    let current = env!("CARGO_PKG_VERSION");
    if version(&latest).is_ok_and(|release| release > version(current).unwrap()) {
        let warning = format!(
            "Warning: rai {latest} is available (installed: {current}). Run `rai update` to install it."
        );
        if io::stderr().is_terminal() {
            eprintln!("\x1b[33m{warning}\x1b[0m");
        } else {
            eprintln!("{warning}");
        }
    }
}

fn release_target() -> Result<&'static str, String> {
    if (cfg!(target_os = "linux") && !cfg!(target_env = "gnu"))
        || (cfg!(target_os = "windows") && !cfg!(target_env = "msvc"))
    {
        return Err("no rai release archive for this binary's target environment".into());
    }
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        (os, arch) => Err(format!("no rai release archive for {os}/{arch}")),
    }
}

fn expected_checksum(contents: &str, filename: &str) -> Result<String, String> {
    let mut matches = contents.lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == filename && fields.next().is_none()).then_some(hash)
    });
    let hash = matches
        .next()
        .ok_or_else(|| format!("SHA256SUMS has no entry for {filename}"))?;
    if matches.next().is_some() || hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(format!("invalid SHA256SUMS entry for {filename}"));
    }
    Ok(hash.to_ascii_lowercase())
}

fn verify_archive(path: &Path, checksum: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != checksum {
        return Err("release archive SHA-256 does not match SHA256SUMS".into());
    }
    Ok(())
}

fn download(url: &str, destination: &Path) -> Result<(), String> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "10",
            "--max-time",
            "120",
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .output()
        .map_err(|e| format!("cannot run curl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "download failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

struct StagingDir {
    path: PathBuf,
    keep: bool,
}

impl StagingDir {
    fn create(next_to: &Path) -> Result<Self, String> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = next_to.join(format!(".rai-update-{}-{timestamp}", std::process::id()));
        fs::create_dir(&path)
            .map_err(|e| format!("cannot stage update beside installed rai: {e}"))?;
        Ok(Self { path, keep: false })
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn extract_archive(archive: &Path, staging: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    let output = Command::new("tar")
        .args(["-xf"])
        .arg(archive)
        .args(["-C"])
        .arg(staging)
        .arg("rai.exe")
        .output();
    #[cfg(not(windows))]
    let output = Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .args(["-C"])
        .arg(staging)
        .arg("rai")
        .output();
    let output = output.map_err(|e| format!("cannot extract release archive: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot extract release archive: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let binary = staging.join(if cfg!(windows) { "rai.exe" } else { "rai" });
    if !fs::symlink_metadata(&binary)
        .map_err(|e| format!("release archive has no rai binary: {e}"))?
        .file_type()
        .is_file()
    {
        return Err("release archive rai binary is not a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&binary)
            .map_err(|e| format!("cannot inspect release binary: {e}"))?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err("release archive rai binary is not executable".into());
        }
    }
    Ok(binary)
}

#[cfg(windows)]
fn replace_binary(staging: &mut StagingDir, binary: &Path, installed: &Path) -> Result<(), String> {
    use std::process::Stdio;

    let script = staging.path.join("finish-update.ps1");
    fs::write(&script, r#"param($Source, $Target, $ParentPid)
Wait-Process -Id $ParentPid -ErrorAction SilentlyContinue
$Backup = Join-Path $PSScriptRoot 'old-rai.exe'
for ($Attempt = 0; $Attempt -lt 30; $Attempt++) {
  try {
    Move-Item -LiteralPath $Target -Destination $Backup -ErrorAction Stop
    try { Move-Item -LiteralPath $Source -Destination $Target -ErrorAction Stop }
    catch { Move-Item -LiteralPath $Backup -Destination $Target -ErrorAction SilentlyContinue; throw }
    Remove-Item -LiteralPath $Backup -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $PSScriptRoot -Recurse -Force -ErrorAction SilentlyContinue
    exit 0
  } catch { Start-Sleep -Seconds 1 }
}
exit 1
"#).map_err(|e| format!("cannot prepare Windows updater: {e}"))?;
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(&script)
        .arg(binary)
        .arg(installed)
        .arg(std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start Windows updater: {e}"))?;
    staging.keep = true;
    Ok(())
}

#[cfg(not(windows))]
fn replace_binary(
    _staging: &mut StagingDir,
    binary: &Path,
    installed: &Path,
) -> Result<(), String> {
    fs::rename(binary, installed)
        .map_err(|e| format!("cannot replace {}: {e}", installed.display()))
}

pub fn install() -> Result<(), String> {
    let latest = fetch_latest(10)?;
    let current = env!("CARGO_PKG_VERSION");
    if version(&latest)? <= version(current)? {
        println!("rai {current} is already up to date (latest release: {latest}).");
        return Ok(());
    }
    let target = release_target()?;
    let suffix = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive_name = format!("rai-{latest}-{target}.{suffix}");
    let installed = env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| format!("cannot locate installed rai: {e}"))?;
    let parent = installed
        .parent()
        .ok_or("installed rai has no parent directory")?;
    let mut staging = StagingDir::create(parent)?;
    let sums = staging.path.join("SHA256SUMS");
    let archive = staging.path.join(&archive_name);
    download(&format!("{DOWNLOAD_ROOT}/{latest}/SHA256SUMS"), &sums)?;
    let checksum = expected_checksum(
        &fs::read_to_string(&sums).map_err(|e| format!("cannot read SHA256SUMS: {e}"))?,
        &archive_name,
    )?;
    download(
        &format!("{DOWNLOAD_ROOT}/{latest}/{archive_name}"),
        &archive,
    )?;
    verify_archive(&archive, &checksum)?;
    let binary = extract_archive(&archive, &staging.path)?;
    replace_binary(&mut staging, &binary, &installed)?;
    store_latest(&latest);
    if cfg!(windows) {
        println!(
            "rai {latest} downloaded and verified. Installation will finish when this process exits."
        );
    } else {
        println!("rai updated from {current} to {latest}.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_versions() {
        assert_eq!(version("v1.2.10").unwrap(), [1, 2, 10]);
        assert!(version("v1.2.3-beta.1").is_err());
        assert!(version("v1.2").is_err());
        assert_eq!(latest_tag(br#"{"tag_name":"v1.2.3"}"#).unwrap(), "v1.2.3");
        assert!(latest_tag(br#"{"message":"Not Found"}"#).is_err());
    }

    #[test]
    fn compares_numeric_versions() {
        assert!(version("v0.10.0").unwrap() > version("0.9.9").unwrap());
    }

    #[test]
    fn finds_exact_checksum_and_rejects_bad_entries() {
        let hash = "a".repeat(64);
        let sums = format!("{hash}  rai-v1.0.0-x86_64-apple-darwin.tar.gz\n");
        assert_eq!(
            expected_checksum(&sums, "rai-v1.0.0-x86_64-apple-darwin.tar.gz").unwrap(),
            hash
        );
        assert!(expected_checksum(&sums, "rai-v1.0.0-aarch64-apple-darwin.tar.gz").is_err());
        assert!(
            expected_checksum(
                &(sums.clone() + &sums),
                "rai-v1.0.0-x86_64-apple-darwin.tar.gz"
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn verifies_extracts_and_replaces_binary() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let extracted = dir.path().join("extracted");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&extracted).unwrap();
        let source_binary = source.join("rai");
        fs::write(&source_binary, b"new rai binary").unwrap();
        fs::set_permissions(&source_binary, fs::Permissions::from_mode(0o755)).unwrap();
        let archive = dir.path().join("rai.tar.gz");
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
        let hash = format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()));
        verify_archive(&archive, &hash).unwrap();
        assert!(verify_archive(&archive, &"0".repeat(64)).is_err());
        let binary = extract_archive(&archive, &extracted).unwrap();
        let installed = dir.path().join("installed-rai");
        fs::write(&installed, b"old rai binary").unwrap();
        let mut staging = StagingDir {
            path: extracted,
            keep: false,
        };
        replace_binary(&mut staging, &binary, &installed).unwrap();
        assert_eq!(fs::read(&installed).unwrap(), b"new rai binary");
    }
}
