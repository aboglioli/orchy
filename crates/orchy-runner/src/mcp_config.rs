use std::path::Path;

/// Agents that read `.mcp.json` from the working directory.
pub fn supports_mcp_json(agent_type: &str) -> bool {
    matches!(agent_type, "claude" | "cursor")
}

/// Write/merge the orchy server entry into `.mcp.json` in `dir` using raw JSON
/// so all existing fields are preserved exactly.
///
/// Returns `true` if we added the entry (caller must call `remove` on cleanup).
/// Returns `false` if orchy was already present (caller must not call `remove`).
/// Returns `Err` if the file exists but cannot be read or parsed — we refuse to
/// touch a config we don't understand rather than risk corrupting it.
pub fn inject(dir: &Path, orchy_url: &str) -> std::io::Result<bool> {
    let path = dir.join(".mcp.json");

    let mut root: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(".mcp.json exists but is not valid JSON: {e}"),
            )
        })?
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    let obj = root.as_object_mut().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            ".mcp.json root is not a JSON object",
        )
    })?;

    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    let servers_map = servers.as_object_mut().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            ".mcp.json mcpServers is not a JSON object",
        )
    })?;

    if servers_map.contains_key("orchy") {
        return Ok(false);
    }

    servers_map.insert(
        "orchy".to_string(),
        serde_json::json!({ "type": "http", "url": orchy_url }),
    );

    let content = serde_json::to_string_pretty(&root)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, content)?;
    Ok(true)
}

/// Remove the orchy entry we previously injected. If `mcpServers` becomes empty
/// the key is dropped. If the whole object becomes empty the file is deleted.
/// All other fields are preserved. Silently skips on any error.
pub fn remove(dir: &Path) {
    let path = dir.join(".mcp.json");
    if !path.exists() {
        return;
    }

    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return;
    };
    let Some(obj) = root.as_object_mut() else {
        return;
    };

    if let Some(servers) = obj.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove("orchy");
        if servers.is_empty() {
            obj.remove("mcpServers");
        }
    }

    if obj.is_empty() {
        let _ = std::fs::remove_file(&path);
    } else if let Ok(new_content) = serde_json::to_string_pretty(&root) {
        let _ = std::fs::write(&path, new_content);
    }
}
