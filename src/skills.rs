//! Explicit local skills; discovery never executes repository content.
use crate::preferences::Preferences;
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub source: String,
}
const FILE_LIMIT: usize = 128 * 1024;
const CONTEXT_LIMIT: usize = 32 * 1024;

pub fn discover(repo: &Path) -> Vec<Skill> {
    let mut roots = vec![
        repo.join("skills"),
        repo.join(".agents/skills"),
        repo.join(".osmogrep/skills"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.extend([
            home.join(".osmogrep/skills"),
            home.join(".agents/skills"),
            home.join(".codex/skills"),
        ]);
    }
    discover_roots(&roots, repo)
}
fn read_bounded(path: &Path, limit: usize) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Skill exceeds context budget".into());
    }
    String::from_utf8(bytes).map_err(|_| "Skill is not UTF-8".into())
}
fn discover_roots(roots: &[PathBuf], repo: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots {
        if fs::symlink_metadata(root)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(4)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .take(4096)
        {
            if skills.len() >= 512 {
                break;
            }
            if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
                continue;
            }
            let Ok(path) = entry.path().canonicalize() else {
                continue;
            };
            if !path.starts_with(&root) {
                continue;
            }
            let Ok(text) = read_bounded(&path, FILE_LIMIT) else {
                continue;
            };
            let fallback = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str())
                .unwrap_or("skill");
            let (name, description) = metadata(&text, fallback);
            if !seen.insert(name.clone()) {
                continue;
            }
            let source = if path.starts_with(repo) {
                "repository"
            } else {
                "user"
            }
            .into();
            skills.push(Skill {
                name,
                description,
                path,
                source,
            });
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}
fn metadata(text: &str, fallback: &str) -> (String, String) {
    let mut name = fallback.to_string();
    let mut description = String::new();
    let mut folded = false;
    if text.lines().next().map(str::trim) == Some("---") {
        for line in text.lines().skip(1).take_while(|l| l.trim() != "---") {
            if let Some(value) = line.strip_prefix("name:") {
                name = value.trim().trim_matches(['\'', '"']).to_string();
                folded = false;
            } else if let Some(value) = line.strip_prefix("description:") {
                let value = value.trim();
                folded = matches!(value, ">" | ">-" | "|" | "|-");
                if !folded {
                    description = value.trim_matches(['\'', '"']).to_string()
                }
            } else if folded && line.starts_with([' ', '\t']) {
                description.push(' ');
                description.push_str(line.trim());
            } else if !line.trim().is_empty() {
                folded = false;
            }
        }
    }
    if name.is_empty()
        || name.len() > 96
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c))
    {
        name = fallback.to_string();
    }
    description = description
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect();
    name = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.:".contains(c) {
                c
            } else {
                '-'
            }
        })
        .take(96)
        .collect();
    if name.is_empty() {
        name = "skill".into();
    }
    (name, description.trim().to_string())
}

pub fn explicit_context(
    repo: &Path,
    text: &str,
    preferences: &Preferences,
) -> Result<String, String> {
    if !text.contains('$') {
        return Ok(String::new());
    }
    let names: BTreeSet<_> = text
        .split_whitespace()
        .filter_map(|s| s.strip_prefix('$'))
        .map(|s| s.trim_end_matches([',', ';', ')', ']', '.']))
        .collect();
    let mut context = String::new();
    for skill in discover(repo).into_iter().filter(|s| {
        names.contains(s.name.as_str()) && !preferences.disabled_skills.contains(&s.name)
    }) {
        let remaining = CONTEXT_LIMIT.saturating_sub(context.len());
        let body = read_bounded(&skill.path, remaining.min(FILE_LIMIT))?;
        let block=format!("\n<user_selected_skill name=\"{}\">\nSource: {}\nResolve referenced files relative to this skill's directory. Existing tool permissions and budgets still apply.\n{}\n</user_selected_skill>\n",skill.name,skill.path.display(),body);
        if context.len() + block.len() > CONTEXT_LIMIT {
            return Err(
                "Selected skills exceed the 32 KiB instruction budget; select fewer skills".into(),
            );
        }
        context.push_str(&block);
    }
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_metadata_and_respects_explicit_opt_in_and_budget() {
        let root = std::env::temp_dir().join(format!("osmogrep-skills-{}", uuid::Uuid::new_v4()));
        let dir = root.join("skills/probe");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"),"---\nname: probe\ndescription: >\n  Inspect a runtime\n  condition.\n---\nOnly inspect local facts.").unwrap();
        let skills = discover_roots(&[root.join("skills")], &root);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Inspect a runtime condition.");
        let mut prefs = Preferences::default();
        assert!(explicit_context(&root, "hello", &prefs).unwrap().is_empty());
        assert!(explicit_context(&root, "use $probe", &prefs)
            .unwrap()
            .contains("Only inspect local facts."));
        prefs.disabled_skills.insert("probe".into());
        assert!(explicit_context(&root, "$probe", &prefs)
            .unwrap()
            .is_empty());
        prefs.disabled_skills.clear();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: probe\n---\n{}", "x".repeat(CONTEXT_LIMIT)),
        )
        .unwrap();
        assert!(explicit_context(&root, "$probe", &prefs).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn discovery_does_not_follow_external_skill_symlinks() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-skill-link-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("skills")).unwrap();
        fs::create_dir_all(root.join("external")).unwrap();
        fs::write(root.join("external/SKILL.md"), "secret").unwrap();
        std::os::unix::fs::symlink(root.join("external"), root.join("skills/link")).unwrap();
        assert!(discover_roots(&[root.join("skills")], &root).is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
