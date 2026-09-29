use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

const MIN_VERSION: (u32, u32, u32) = (0, 152, 1);

pub fn parse_version(output: &str) -> Result<(u32, u32, u32), String> {
    let version = output
        .trim()
        .strip_prefix("codex-cli ")
        .ok_or("unrecognized Codex version output")?;
    let numeric = version.split(['-', '+']).next().unwrap();
    let parts = numeric
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid Codex version")?;
    if parts.len() != 3 {
        return Err("invalid Codex version".into());
    }
    Ok((parts[0], parts[1], parts[2]))
}

pub fn check_version() -> Result<(), String> {
    let output = Command::new("codex")
        .arg("--version")
        .output()
        .map_err(|_| "Codex target needs codex-cli >= 0.152.1 on PATH".to_string())?;
    if !output.status.success() {
        return Err("codex --version failed".into());
    }
    let version = parse_version(&String::from_utf8_lossy(&output.stdout))?;
    if version < MIN_VERSION {
        return Err(format!(
            "Codex CLI {version:?} is older than the validated minimum 0.152.1"
        ));
    }
    Ok(())
}

pub fn outputs(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut config =
        String::from("# Codex projection from .agents/ (tested with codex-cli 0.152.1)\n");
    let subagents = read_subagents(root)?;
    let mcp_path = root.join(".agents/mcp.json");
    if mcp_path.exists() {
        if mcp_path.is_symlink() {
            return Err("symlink source unsupported: .agents/mcp.json".into());
        }
        let manifest = read_json(&mcp_path)?;
        let top = manifest
            .as_object()
            .ok_or(".agents/mcp.json must be an object")?;
        if top.len() != 1 || !top.contains_key("servers") {
            return Err(".agents/mcp.json supports only servers".into());
        }
        let servers = top["servers"]
            .as_object()
            .ok_or("servers must be an object")?;
        for (name, server) in servers {
            valid_name(name)?;
            let fields = server.as_object().ok_or("MCP server must be an object")?;
            let transport = fields
                .get("transport")
                .and_then(Value::as_str)
                .ok_or("MCP transport is required")?;
            let allowed: &[&str] = if transport == "stdio" {
                &[
                    "transport",
                    "command",
                    "args",
                    "cwd",
                    "env_vars",
                    "default_tools_approval_mode",
                ]
            } else if transport == "http" {
                &[
                    "transport",
                    "url",
                    "bearer_token_env_var",
                    "default_tools_approval_mode",
                ]
            } else {
                return Err(format!("unsupported MCP transport for {name}: {transport}"));
            };
            for key in fields.keys() {
                if !allowed.contains(&key.as_str()) {
                    return Err(format!("unsupported MCP field {name}.{key}"));
                }
            }
            config.push_str(&format!("\n[mcp_servers.{name}]\n"));
            if transport == "stdio" {
                for field in ["command", "cwd"] {
                    if let Some(value) = fields.get(field) {
                        config.push_str(&string_field(field, value)?);
                    }
                }
                if !fields.contains_key("command") {
                    return Err(format!("MCP server {name} needs command"));
                }
                for field in ["args", "env_vars"] {
                    if let Some(value) = fields.get(field) {
                        config.push_str(&array_field(field, value)?);
                    }
                }
            } else {
                config.push_str(&string_field(
                    "url",
                    fields
                        .get("url")
                        .ok_or(format!("MCP server {name} needs url"))?,
                )?);
                if let Some(value) = fields.get("bearer_token_env_var") {
                    config.push_str(&string_field("bearer_token_env_var", value)?);
                }
            }
            if let Some(value) = fields.get("default_tools_approval_mode") {
                let mode = value
                    .as_str()
                    .ok_or("default_tools_approval_mode must be a string")?;
                if !["auto", "prompt", "writes", "approve"].contains(&mode) {
                    return Err(format!("unsupported MCP approval mode for {name}: {mode}"));
                }
                config.push_str(&string_field("default_tools_approval_mode", value)?);
            }
        }
    }
    if !subagents.is_empty() {
        config.push_str("\n[agents]\nenabled = true\n");
        for subagent in &subagents {
            let name = subagent["name"].as_str().unwrap();
            config.push_str(&format!("\n[agents.{name}]\n"));
            config.push_str(&format!("config_file = \"agents/{name}.toml\"\n"));
            config.push_str(&string_field("description", &subagent["description"])?);
        }
    }
    config.push_str("\n[[hooks.UserPromptSubmit]]\n[[hooks.UserPromptSubmit.hooks]]\ntype = \"command\"\ncommand = \"rai sync --codex-hook\"\ntimeout = 30\n");
    let mut result = vec![(".codex/config.toml".to_string(), config)];
    for subagent in subagents {
        let name = subagent["name"].as_str().unwrap();
        let mut body = String::new();
        for field in [
            "name",
            "description",
            "model",
            "model_reasoning_effort",
            "sandbox_mode",
            "developer_instructions",
        ] {
            if let Some(value) = subagent.get(field) {
                body.push_str(&string_field(field, value)?);
            }
        }
        if let Some(value) = subagent.get("nickname_candidates") {
            body.push_str(&array_field("nickname_candidates", value)?);
        }
        result.push((format!(".codex/agents/{name}.toml"), body));
    }
    validate_skills(root)?;
    Ok(result)
}

fn read_subagents(root: &Path) -> Result<Vec<Value>, String> {
    let mut subagents = Vec::new();
    let agents_dir = root.join(".agents/subagents");
    if agents_dir.exists() {
        if agents_dir.is_symlink() {
            return Err("symlink source unsupported: .agents/subagents".into());
        }
        let mut entries = fs::read_dir(&agents_dir)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        for path in entries {
            if path.is_symlink()
                || !path.is_file()
                || path.extension().is_none_or(|ext| ext != "yaml")
            {
                return Err(format!(
                    "only regular .yaml subagents are supported: {}",
                    path.display()
                ));
            }
            let name = path.file_stem().unwrap().to_string_lossy();
            valid_name(&name)?;
            let source =
                fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let value: Value =
                serde_yaml::from_str(&source).map_err(|e| format!("{}: {e}", path.display()))?;
            let fields = value.as_object().ok_or("subagent must be an object")?;
            for key in fields.keys() {
                if ![
                    "name",
                    "description",
                    "developer_instructions",
                    "model",
                    "model_reasoning_effort",
                    "sandbox_mode",
                    "nickname_candidates",
                ]
                .contains(&key.as_str())
                {
                    return Err(format!("unsupported subagent field {name}.{key}"));
                }
            }
            let field = |field| {
                fields
                    .get(field)
                    .and_then(Value::as_str)
                    .ok_or(format!("subagent {name} needs string {field}"))
            };
            if fields["name"].as_str() != Some(name.as_ref()) {
                return Err(format!("subagent name must match filename: {name}"));
            }
            field("description")?;
            field("developer_instructions")?;
            for optional in ["model", "model_reasoning_effort", "sandbox_mode"] {
                if fields.contains_key(optional) {
                    field(optional)?;
                }
            }
            if let Some(value) = fields.get("nickname_candidates") {
                let candidates = value.as_array().ok_or(format!(
                    "subagent {name} nickname_candidates must be an array"
                ))?;
                if candidates.is_empty() || candidates.iter().any(|value| !value.is_string()) {
                    return Err(format!(
                        "subagent {name} nickname_candidates must contain strings"
                    ));
                }
            }
            subagents.push(value);
        }
    }
    Ok(subagents)
}

fn validate_skills(root: &Path) -> Result<(), String> {
    let dir = root.join(".agents/skills");
    if !dir.exists() {
        return Ok(());
    }
    if dir.is_symlink() {
        return Err("symlink source unsupported: .agents/skills".into());
    }
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_symlink() || !path.is_dir() {
            return Err(format!(
                "skill must be a regular directory: {}",
                path.display()
            ));
        }
        let name = path.file_name().unwrap().to_string_lossy();
        valid_name(&name)?;
        let skill = path.join("SKILL.md");
        if skill.is_symlink() {
            return Err(format!("symlink skill unsupported: {}", skill.display()));
        }
        let source = fs::read_to_string(&skill).map_err(|e| format!("{}: {e}", skill.display()))?;
        if !source.starts_with("---\n")
            || !source.contains("\nname:")
            || !source.contains("\ndescription:")
            || !source[4..].contains("\n---\n")
        {
            return Err(format!("invalid skill frontmatter: {}", skill.display()));
        }
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&source).map_err(|e| format!("{}: {e}", path.display()))
}

fn valid_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(format!("invalid Codex resource name: {value}"));
    }
    Ok(())
}

fn string_field(name: &str, value: &Value) -> Result<String, String> {
    let value = value.as_str().ok_or(format!("{name} must be a string"))?;
    Ok(format!("{name} = {}\n", toml::Value::String(value.into())))
}

fn array_field(name: &str, value: &Value) -> Result<String, String> {
    let items = value.as_array().ok_or(format!("{name} must be an array"))?;
    let rendered = items
        .iter()
        .map(|item| {
            item.as_str()
                .map(|s| toml::Value::String(s.into()).to_string())
                .ok_or(format!("{name} must contain only strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("{name} = [{}]\n", rendered.join(", ")))
}
