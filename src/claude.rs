use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn outputs(
    root: &Path,
    rules: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<(String, String, String)>, String> {
    let settings = serde_json::to_string_pretty(&json!({"hooks": {"PreToolUse": [{
        "matcher": "Edit|Write|MultiEdit",
        "hooks": [{"type": "command", "command": "rai guard", "timeout": 10}]
    }]}}))
    .map_err(|e| e.to_string())?;
    let mut result = vec![(
        ".claude/settings.json".into(),
        format!("{settings}\n"),
        "json".into(),
    )];
    for (scope, sections) in rules {
        let body = format!("# Shared project rules\n\n{}\n", sections.join("\n\n"));
        if scope.is_empty() {
            result.push(("CLAUDE.md".into(), body, "markdown".into()));
        } else {
            let name = scope.replace('/', "__");
            let front = serde_yaml::to_string(&json!({"paths": [format!("{scope}/**")]}))
                .map_err(|e| e.to_string())?;
            result.push((
                format!(".claude/rules/{name}.md"),
                format!("---\n{front}---\n\n{body}"),
                "markdown".into(),
            ));
        }
    }
    for agent in crate::codex::read_subagents(root)? {
        let name = agent["name"].as_str().unwrap();
        let front =
            serde_yaml::to_string(&json!({"name": name, "description": agent["description"]}))
                .map_err(|e| e.to_string())?;
        let instructions = agent["developer_instructions"].as_str().unwrap().trim();
        result.push((
            format!(".claude/agents/{name}.md"),
            format!("---\n{front}---\n\n{instructions}\n"),
            "markdown".into(),
        ));
    }
    let skills = root.join(".agents/skills");
    if skills.is_dir() {
        let mut entries = fs::read_dir(&skills)
            .map_err(|e| e.to_string())?
            .map(|e| e.map(|v| v.path()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        for skill in entries {
            if skill.file_name().is_some_and(|name| name == ".keep")
                && skill.is_file()
                && !skill.is_symlink()
            {
                continue;
            }
            let name = skill.file_name().unwrap().to_string_lossy();
            for entry in fs::read_dir(&skill).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.file_name().is_some_and(|name| name == ".keep")
                    && path.is_file()
                    && !path.is_symlink()
                {
                    continue;
                }
                if path.file_name().unwrap() != "SKILL.md" {
                    return Err(format!(
                        "Claude projection does not support skill attachment: {}",
                        path.display()
                    ));
                }
            }
            let body = fs::read_to_string(skill.join("SKILL.md")).map_err(|e| e.to_string())?;
            result.push((
                format!(".claude/skills/{name}/SKILL.md"),
                body,
                "markdown".into(),
            ));
        }
    }
    if let Some(source) = crate::codex::read_mcp(root)? {
        let servers = source["servers"]
            .as_object()
            .ok_or("MCP servers must be an object")?;
        let mut projected = Map::new();
        for (name, server) in servers {
            let transport = server["transport"]
                .as_str()
                .ok_or("MCP transport is required")?;
            let mut native = Map::new();
            native.insert("type".into(), json!(transport));
            match transport {
                "stdio" => {
                    for field in ["command", "args"] {
                        if let Some(value) = server.get(field) {
                            native.insert(field.into(), value.clone());
                        }
                    }
                    if let Some(vars) = server.get("env_vars") {
                        let vars = vars.as_array().ok_or("env_vars must be an array")?;
                        let mut env = Map::new();
                        for var in vars {
                            let var = var.as_str().ok_or("env_vars must contain strings")?;
                            env.insert(var.into(), json!(format!("${{{var}}}")));
                        }
                        native.insert("env".into(), Value::Object(env));
                    }
                }
                "http" => {
                    native.insert("url".into(), server["url"].clone());
                    if let Some(var) = server.get("bearer_token_env_var") {
                        let var = var
                            .as_str()
                            .ok_or("bearer_token_env_var must be a string")?;
                        native.insert(
                            "headers".into(),
                            json!({"Authorization": format!("Bearer ${{{var}}}")}),
                        );
                    }
                }
                _ => return Err(format!("unsupported Claude MCP transport: {transport}")),
            }
            projected.insert(name.clone(), Value::Object(native));
        }
        let body = serde_json::to_string_pretty(&json!({"mcpServers": projected}))
            .map_err(|e| e.to_string())?;
        result.push((".mcp.json".into(), format!("{body}\n"), "json".into()));
    }
    Ok(result)
}
