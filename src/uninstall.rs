use crate::command_log::CommandExt;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const HOOKS: [&str; 3] = ["post-checkout", "post-merge", "post-rewrite"];

pub fn uninstall() -> Result<(), String> {
    remove_watcher()?;
    remove_global_hooks()?;
    remove_template()?;
    println!("RosettAI watcher and Git hooks removed.");
    Ok(())
}

fn remove_global_hooks() -> Result<(), String> {
    let config = config_dir()?;
    let hooks = config.join("global-hooks");
    let record = config.join("global-hooks-previous.json");
    if hooks.is_symlink() || record.is_symlink() {
        return Err("symlink global hook configuration conflict".into());
    }
    if !record.exists() {
        return Ok(());
    }
    let previous: Option<String> =
        serde_json::from_slice(&fs::read(&record).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", record.display()))?;
    let current = Command::new("git")
        .args(["config", "--global", "--get", "core.hooksPath"])
        .output()
        .map_err(|e| e.to_string())?;
    if current.status.success()
        && String::from_utf8_lossy(&current.stdout).trim() == hooks.to_string_lossy()
    {
        let mut command = Command::new("git");
        command.args(["config", "--global"]);
        if let Some(old) = previous {
            command.arg("core.hooksPath").arg(old);
        } else {
            command.args(["--unset", "core.hooksPath"]);
        }
        if !command
            .logged_status()
            .map_err(|e| e.to_string())?
            .success()
        {
            return Err("could not restore global core.hooksPath".into());
        }
    }
    for hook in crate::setup::GIT_HOOK_EVENTS {
        remove_hook(&hooks.join(hook))?;
    }
    remove_empty_dir(&hooks)?;
    if !hooks.exists() {
        fs::remove_file(record).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn config_dir() -> Result<PathBuf, String> {
    crate::user_data::config_dir()
}

fn owned_file(path: &Path, marker: &str) -> Result<bool, String> {
    if path.is_symlink() {
        return Err(format!("symlink conflict: {}", path.display()));
    }
    if !path.exists() {
        return Ok(false);
    }
    let body = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !body.contains(marker) {
        return Err(format!("unowned file conflict: {}", path.display()));
    }
    Ok(true)
}

fn remove_hook(path: &Path) -> Result<(), String> {
    if path.is_symlink() {
        eprintln!("rai: preserved symlink hook {}", path.display());
        return Ok(());
    }
    if !path.exists() {
        crate::hook_registry::forget(path)?;
        return Ok(());
    }
    let body = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if body.starts_with("#!/bin/sh\n# rai-managed-hook\n") {
        fs::remove_file(path).map_err(|e| e.to_string())?;
        crate::hook_registry::forget(path)?;
    } else {
        eprintln!("rai: preserved unowned hook {}", path.display());
    }
    Ok(())
}

fn remove_template() -> Result<(), String> {
    let template = config_dir()?.join("git-template");
    let configured = Command::new("git")
        .args(["config", "--global", "--get", "init.templateDir"])
        .output()
        .map_err(|e| e.to_string())?;
    if configured.status.success()
        && String::from_utf8_lossy(&configured.stdout).trim() == template.to_string_lossy()
    {
        let status = Command::new("git")
            .args(["config", "--global", "--unset", "init.templateDir"])
            .logged_status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("could not unset global init.templateDir".into());
        }
    }
    if template.is_symlink() {
        return Err(format!("symlink conflict: {}", template.display()));
    }
    for hook in HOOKS {
        let path = template.join("hooks").join(hook);
        remove_hook(&path)?;
    }
    remove_empty_dir(&template.join("hooks"))?;
    remove_empty_dir(&template)?;
    Ok(())
}

fn remove_empty_dir(path: &Path) -> Result<(), String> {
    if path.is_dir()
        && fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .next()
            .is_none()
    {
        fs::remove_dir(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_watcher() -> Result<(), String> {
    let plist = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?)
        .join("Library/LaunchAgents/ai.rosettai.rai.plist");
    if owned_file(&plist, "<!-- rai-managed-launch-agent -->")? {
        let uid = Command::new("id")
            .arg("-u")
            .output()
            .map_err(|e| e.to_string())?;
        let domain = format!("gui/{}", String::from_utf8_lossy(&uid.stdout).trim());
        let result = Command::new("launchctl")
            .args(["bootout", &domain])
            .arg(&plist)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            eprintln!(
                "rai: launchctl bootout: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            );
        }
        fs::remove_file(&plist).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn remove_watcher() -> Result<(), String> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?).join(".config"));
    let service = base.join("systemd/user/rai-watch.service");
    if owned_file(&service, "# rai-managed-watcher")? {
        let status = Command::new("systemctl")
            .args(["--user", "disable", "--now", "rai-watch.service"])
            .logged_status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("could not stop rai-watch.service".into());
        }
        fs::remove_file(&service).map_err(|e| e.to_string())?;
        let status = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .logged_status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("systemd daemon-reload failed".into());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn remove_watcher() -> Result<(), String> {
    let script = "$task = Get-ScheduledTask -TaskName 'RosettAI Watcher' -ErrorAction SilentlyContinue; if ($task) { if ($task.Description -ne 'rai-managed-watcher') { throw 'unowned watcher task conflict' }; Stop-ScheduledTask -TaskName 'RosettAI Watcher' -ErrorAction SilentlyContinue; Unregister-ScheduledTask -TaskName 'RosettAI Watcher' -Confirm:$false }";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "could not remove watcher task: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}
