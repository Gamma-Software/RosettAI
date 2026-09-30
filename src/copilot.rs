use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn outputs(
    root: &Path,
    rules: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<(String, String, String)>, String> {
    let mut outputs = Vec::new();

    for (scope, sections) in rules {
        let body = format!("# Shared project rules\n\n{}\n", sections.join("\n\n"));
        if scope.is_empty() {
            outputs.push((
                ".github/copilot-instructions.md".into(),
                body,
                "markdown".into(),
            ));
        } else {
            let filename = format!("{}.instructions.md", scope.replace('/', "__"));
            let apply_to = format!("{scope}/**");
            let frontmatter = serde_yaml::to_string(&json!({ "applyTo": apply_to }))
                .map_err(|e| e.to_string())?;
            outputs.push((
                format!(".github/instructions/{filename}"),
                format!("---\n{frontmatter}---\n\n{body}"),
                "markdown".into(),
            ));
        }
    }

    for agent in crate::codex::read_subagents(root)? {
        let name = agent["name"].as_str().unwrap();
        let header = json!({
            "name": name,
            "description": agent["description"],
            "target": "vscode"
        });
        let yaml = serde_yaml::to_string(&header).map_err(|e| e.to_string())?;
        let instructions = agent["developer_instructions"].as_str().unwrap().trim();
        outputs.push((
            format!(".github/agents/{name}.agent.md"),
            format!("---\n{yaml}---\n\n{instructions}\n"),
            "markdown".into(),
        ));
    }

    crate::codex::validate_skills(root)?;
    if let Some(mcp) = mcp_output(root)? {
        outputs.push((".vscode/mcp.json".into(), mcp, "jsonc".into()));
    }
    Ok(outputs)
}

fn mcp_output(root: &Path) -> Result<Option<String>, String> {
    let path = root.join(".agents/mcp.json");
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let canonical: Value =
        serde_json::from_str(&source).map_err(|e| format!("{}: {e}", path.display()))?;
    let servers = canonical["servers"]
        .as_object()
        .ok_or(".agents/mcp.json servers must be an object")?;
    let mut projected = Map::new();
    for (name, value) in servers {
        let server = value.as_object().ok_or("MCP server must be an object")?;
        let transport = server["transport"]
            .as_str()
            .ok_or("MCP transport is required")?;
        let mut out = Map::new();
        match transport {
            "stdio" => {
                out.insert("type".into(), json!("stdio"));
                for field in ["command", "args", "cwd"] {
                    if let Some(value) = server.get(field) {
                        out.insert(field.into(), value.clone());
                    }
                }
                if let Some(names) = server.get("env_vars").and_then(Value::as_array) {
                    let mut env = Map::new();
                    for variable in names {
                        let variable = variable.as_str().ok_or("env_vars must contain strings")?;
                        env.insert(variable.into(), json!(format!("${{env:{variable}}}")));
                    }
                    out.insert("env".into(), Value::Object(env));
                }
            }
            "http" => {
                out.insert("type".into(), json!("http"));
                out.insert("url".into(), server["url"].clone());
                if let Some(variable) = server.get("bearer_token_env_var").and_then(Value::as_str) {
                    out.insert(
                        "headers".into(),
                        json!({"Authorization": format!("Bearer ${{env:{variable}}}")}),
                    );
                }
            }
            _ => return Err(format!("unsupported MCP transport for {name}: {transport}")),
        }
        projected.insert(name.clone(), Value::Object(out));
    }
    let body = serde_json::to_string_pretty(&json!({ "servers": projected }))
        .map_err(|e| e.to_string())?;
    Ok(Some(format!("{body}\n")))
}
