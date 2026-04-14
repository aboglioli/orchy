use std::path::Path;

use orchy_core::knowledge::{KnowledgeStore, KnowledgeKind, WriteKnowledge};
use orchy_core::knowledge::service::KnowledgeService;
use orchy_core::namespace::{Namespace, ProjectId};
use tracing::{info, warn};

pub async fn load_skills_from_dir<S: KnowledgeStore>(
    dir: &Path,
    service: &KnowledgeService<S, crate::embeddings::EmbeddingsBackend>,
) -> Result<usize, Box<dyn std::error::Error>> {
    if !dir.exists() {
        warn!(path = %dir.display(), "skills directory does not exist, skipping");
        return Ok(0);
    }

    let mut count = 0;
    load_recursive(dir, dir, service, &mut count).await?;
    info!(count, path = %dir.display(), "loaded skills from disk");
    Ok(count)
}

fn load_recursive<'a, S: KnowledgeStore>(
    base: &'a Path,
    current: &'a Path,
    service: &'a KnowledgeService<S, crate::embeddings::EmbeddingsBackend>,
    count: &'a mut usize,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + 'a>,
> {
    Box::pin(async move { load_recursive_inner(base, current, service, count).await })
}

async fn load_recursive_inner<S: KnowledgeStore>(
    base: &Path,
    current: &Path,
    service: &KnowledgeService<S, crate::embeddings::EmbeddingsBackend>,
    count: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: Vec<_> = std::fs::read_dir(current)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();

        if path.is_dir() {
            load_recursive(base, &path, service, count).await?;
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "md" {
            continue;
        }

        let rel = path.strip_prefix(base)?;
        let namespace_str = rel
            .parent()
            .map(|p| p.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
            .unwrap_or_default();

        if namespace_str.is_empty() {
            warn!(file = %path.display(), "skill file at root of skills dir (no project namespace), skipping");
            continue;
        }

        let (project_str, scope_str) = match namespace_str.split_once('/') {
            Some((p, s)) => (p.to_string(), Some(s.to_string())),
            None => (namespace_str.clone(), None),
        };

        let project = match ProjectId::try_from(project_str.clone()) {
            Ok(p) => p,
            Err(e) => {
                warn!(file = %path.display(), error = %e, "invalid project from path, skipping");
                continue;
            }
        };

        let namespace = match scope_str {
            Some(scope) => match Namespace::try_from(format!("/{scope}")) {
                Ok(ns) => ns,
                Err(e) => {
                    warn!(file = %path.display(), error = %e, "invalid namespace from path, skipping");
                    continue;
                }
            },
            None => Namespace::root(),
        };

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let raw = std::fs::read_to_string(&path)?;
        let (description, content) = parse_frontmatter(&raw, &name);

        let cmd = WriteKnowledge {
            project: project.clone(),
            namespace: namespace.clone(),
            path: format!("skills/{name}"),
            kind: KnowledgeKind::Skill,
            title: description,
            content,
            tags: vec![],
            expected_version: None,
            agent_id: None,
            metadata: std::collections::HashMap::new(),
        };

        service
            .write(cmd)
            .await
            .map_err(|e| format!("failed to load skill {} in {}: {}", name, namespace, e))?;

        *count += 1;
    }

    Ok(())
}

fn parse_frontmatter(raw: &str, default_name: &str) -> (String, String) {
    let trimmed = raw.trim_start();

    if !trimmed.starts_with("---") {
        return (default_name.to_string(), raw.to_string());
    }

    let after_first = &trimmed[3..];
    let Some(end) = after_first.find("---") else {
        return (default_name.to_string(), raw.to_string());
    };

    let frontmatter = &after_first[..end];
    let content = after_first[end + 3..].trim_start().to_string();

    let mut description = default_name.to_string();

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("description:") {
            description = val.trim().to_string();
        }
    }

    (description, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_without_frontmatter() {
        let (desc, content) = parse_frontmatter("just content", "my-skill");
        assert_eq!(desc, "my-skill");
        assert_eq!(content, "just content");
    }

    #[test]
    fn parse_frontmatter_with_description() {
        let raw = "---\ndescription: My cool skill\n---\nThe content here";
        let (desc, content) = parse_frontmatter(raw, "fallback");
        assert_eq!(desc, "My cool skill");
        assert_eq!(content, "The content here");
    }

    #[test]
    fn parse_frontmatter_no_closing() {
        let raw = "---\ndescription: broken\nno closing";
        let (desc, _content) = parse_frontmatter(raw, "fallback");
        assert_eq!(desc, "fallback");
    }
}
