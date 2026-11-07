//! Installed skill discovery across `~/.pi/agent/skills/**/SKILL.md`,
//! `~/.agents/skills/**/SKILL.md`, and `<cwd>/.pi/skills/**/SKILL.md`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::models::{resolve_pi_agent_dir, resolve_user_home_dir};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    pub name: String,
    pub description: String,
    /// Always `"skill"` for filesystem skills; merged with `"prompt"` / `"extension"` from Pi RPC.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `"workspace"` | `"user"` | `"agents"`
    pub scope: String,
}

fn unquote_yaml_scalar(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return trimmed[1..trimmed.len() - 1].trim().to_string();
    }
    trimmed.to_string()
}

/// Parses YAML frontmatter (`name` and `description`) from a `SKILL.md` file.
/// Falls back to `fallback_name` (parent folder name) and the first non-heading markdown line
/// if frontmatter fields are absent.
pub fn parse_skill_markdown(content: &str, fallback_name: &str) -> (String, String) {
    let stripped = content.trim_start_matches('\u{feff}');
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut body_start_line = 0usize;

    let lines: Vec<&str> = stripped.lines().collect();
    if lines.first().map(|l| l.trim()) == Some("---") {
        let mut end_idx = None;
        for (idx, line) in lines.iter().enumerate().skip(1) {
            let t = line.trim();
            if t == "---" || t == "..." {
                end_idx = Some(idx);
                break;
            }
        }

        if let Some(end) = end_idx {
            let fm_lines = &lines[1..end];
            let mut i = 0;
            while i < fm_lines.len() {
                let raw_line = fm_lines[i];
                let trimmed = raw_line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    i += 1;
                    continue;
                }

                if let Some((raw_key, raw_val)) = trimmed.split_once(':') {
                    let key = raw_key.trim();
                    let val = raw_val.trim();

                    if key == "name" {
                        let parsed = unquote_yaml_scalar(val);
                        if !parsed.is_empty() {
                            name = Some(parsed);
                        }
                        i += 1;
                        continue;
                    }

                    if key == "description" {
                        if val == "|" || val == "|-" || val == ">" || val == ">-" || val.is_empty()
                        {
                            let folded = val.starts_with('>');
                            let mut block_parts: Vec<String> = Vec::new();
                            i += 1;
                            while i < fm_lines.len() {
                                let next_raw = fm_lines[i];
                                let is_indented =
                                    next_raw.starts_with(' ') || next_raw.starts_with('\t');
                                if !is_indented && !next_raw.trim().is_empty() {
                                    break;
                                }
                                let next_trimmed = next_raw.trim();
                                if !next_trimmed.is_empty() {
                                    block_parts.push(next_trimmed.to_string());
                                }
                                i += 1;
                            }
                            let joined = if folded {
                                block_parts.join(" ")
                            } else {
                                block_parts.join("\n")
                            };
                            if !joined.is_empty() {
                                description = Some(joined);
                            }
                            continue;
                        } else {
                            let parsed = unquote_yaml_scalar(val);
                            if !parsed.is_empty() {
                                description = Some(parsed);
                            }
                        }
                    }
                }
                i += 1;
            }

            body_start_line = end + 1;
        }
    }

    let final_name = name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| fallback_name.trim().to_string());

    let final_description = description
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| extract_first_body_paragraph(&lines[body_start_line..]));

    (final_name, final_description)
}

fn extract_first_body_paragraph(body_lines: &[&str]) -> String {
    for line in body_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("```") {
            continue;
        }
        return trimmed.chars().take(240).collect();
    }
    String::new()
}

fn collect_skill_files_recursive(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name_os) = path.file_name() else {
            continue;
        };
        let name = name_os.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if path.is_file() && name.eq_ignore_ascii_case("SKILL.md") {
            out.push(path);
        } else if path.is_dir() {
            subdirs.push(path);
        }
    }

    subdirs.sort();
    for sub in subdirs {
        collect_skill_files_recursive(&sub, depth + 1, max_depth, out);
    }
}

pub fn scan_skill_roots(roots: &[(PathBuf, &str)]) -> Vec<InstalledSkill> {
    let mut skills: Vec<InstalledSkill> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    for (root_dir, scope) in roots {
        if !root_dir.exists() || !root_dir.is_dir() {
            continue;
        }
        let mut skill_files = Vec::new();
        collect_skill_files_recursive(root_dir, 0, 6, &mut skill_files);

        for skill_md_path in skill_files {
            let Ok(content) = std::fs::read_to_string(&skill_md_path) else {
                continue;
            };
            let fallback_name = skill_md_path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "skill".to_string());

            let (name, description) = parse_skill_markdown(&content, &fallback_name);
            if name.is_empty() {
                continue;
            }
            let dedup_key = name.to_lowercase();
            if !seen_names.insert(dedup_key) {
                continue;
            }

            skills.push(InstalledSkill {
                name,
                description,
                source: "skill".to_string(),
                path: Some(skill_md_path.to_string_lossy().to_string()),
                scope: (*scope).to_string(),
            });
        }
    }

    skills.sort_by_key(|a| a.name.to_lowercase());
    skills
}

#[tauri::command]
pub async fn list_installed_skills(cwd: Option<String>) -> Result<Vec<InstalledSkill>, String> {
    tokio::task::spawn_blocking(move || {
        let mut roots: Vec<(PathBuf, &str)> = Vec::new();

        // 1. Workspace skills (`<cwd>/.pi/skills` and `<cwd>/.agents/skills`) take precedence
        if let Some(cwd_str) = cwd.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let cwd_path = PathBuf::from(cwd_str);
            roots.push((cwd_path.join(".pi").join("skills"), "workspace"));
            roots.push((cwd_path.join(".agents").join("skills"), "workspace"));
        }

        // 2. `~/.pi/agent/skills`
        if let Some(agent_dir) = resolve_pi_agent_dir() {
            roots.push((agent_dir.join("skills"), "user"));
        }

        // 3. `~/.agents/skills`
        if let Some(home_dir) = resolve_user_home_dir() {
            roots.push((home_dir.join(".agents").join("skills"), "agents"));
        }

        Ok(scan_skill_roots(&roots))
    })
    .await
    .map_err(|e| format!("Failed to join list_installed_skills task: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pipan-skills-test-{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn test_parse_skill_markdown_frontmatter_and_multiline_description() {
        let md = "---\nname: web-browser\ndescription: >-\n  Browse pages and inspect DOM\n  using headless Chrome.\n---\n# Web Browser\nBody text\n";
        let (name, desc) = parse_skill_markdown(md, "fallback");
        assert_eq!(name, "web-browser");
        assert_eq!(desc, "Browse pages and inspect DOM using headless Chrome.");
    }

    #[test]
    fn test_parse_skill_markdown_fallback_without_frontmatter() {
        let md = "# My Skill Heading\n\nRuns automated database migrations safely.\n";
        let (name, desc) = parse_skill_markdown(md, "db-migrate");
        assert_eq!(name, "db-migrate");
        assert_eq!(desc, "Runs automated database migrations safely.");
    }

    #[test]
    fn test_scan_skill_roots_discovers_nested_skills_and_deduplicates() {
        let root = make_temp_dir("scan");
        let ws_skills = root.join("workspace").join(".pi").join("skills");
        let user_skills = root.join("user").join(".pi").join("agent").join("skills");
        let agents_skills = root.join("user").join(".agents").join("skills");

        let ws_skill_dir = ws_skills.join("pr-review");
        let user_dup_dir = user_skills.join("pr-review");
        let agents_skill_dir = agents_skills.join("nested").join("security-audit");

        std::fs::create_dir_all(&ws_skill_dir).unwrap();
        std::fs::create_dir_all(&user_dup_dir).unwrap();
        std::fs::create_dir_all(&agents_skill_dir).unwrap();

        std::fs::write(
            ws_skill_dir.join("SKILL.md"),
            "---\nname: pr-review\ndescription: Workspace PR review checklist\n---\n",
        )
        .unwrap();
        std::fs::write(
            user_dup_dir.join("SKILL.md"),
            "---\nname: pr-review\ndescription: Global PR review checklist\n---\n",
        )
        .unwrap();
        std::fs::write(
            agents_skill_dir.join("SKILL.md"),
            "---\nname: \"security-audit\"\ndescription: \"Deep dependency and OWASP check\"\n---\n",
        )
        .unwrap();

        let scanned = scan_skill_roots(&[
            (ws_skills, "workspace"),
            (user_skills, "user"),
            (agents_skills, "agents"),
        ]);

        assert_eq!(scanned.len(), 2);
        assert_eq!(scanned[0].name, "pr-review");
        assert_eq!(scanned[0].scope, "workspace");
        assert_eq!(scanned[0].description, "Workspace PR review checklist");

        assert_eq!(scanned[1].name, "security-audit");
        assert_eq!(scanned[1].scope, "agents");
        assert_eq!(scanned[1].description, "Deep dependency and OWASP check");

        let _ = std::fs::remove_dir_all(&root);
    }
}
