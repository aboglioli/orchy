use std::path::Path;

use orchy_core::namespace::Namespace;
use orchy_core::skill::SkillStore;
use orchy_core::skill::WriteSkill;
use orchy_core::skill::service::SkillService;
use tracing::{info, warn};

/// Loads skill files from a directory into the store.
///
/// Directory structure maps to namespaces:
///   skills/my-project/commit-conventions.md  → namespace="my-project", name="commit-conventions"
///   skills/my-project/backend/coding-style.md → namespace="my-project/backend", name="coding-style"
///
/// Files support optional frontmatter:
///   ---
///   description: Short description
///   ---
///   Skill content here...
///
/// If no frontmatter, the filename (without extension) is the name and
/// description defaults to the name.
pub async fn load_skills_from_dir<S: SkillStore>(
    dir: &Path,
    service: &SkillService<S>,
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

fn load_recursive<'a, S: SkillStore>(
    base: &'a Path,
    current: &'a Path,
    service: &'a SkillService<S>,
    count: &'a mut usize,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + 'a>,
> {
    Box::pin(async move { load_recursive_inner(base, current, service, count).await })
}

async fn load_recursive_inner<S: SkillStore>(
    base: &Path,
    current: &Path,
    service: &SkillService<S>,
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

        let namespace = match Namespace::try_from(namespace_str.clone()) {
            Ok(ns) => ns,
            Err(e) => {
                warn!(file = %path.display(), error = %e, "invalid namespace from path, skipping");
                continue;
            }
        };

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let raw = std::fs::read_to_string(&path)?;
        let (description, content) = parse_frontmatter(&raw, &name);

        let cmd = WriteSkill {
            namespace: namespace.clone(),
            name: name.clone(),
            description,
            content,
            written_by: None,
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
