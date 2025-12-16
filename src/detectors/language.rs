use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Java,
    Unknown,
}

pub fn detect_language(root: &Path) -> Language {
    let mut counts: HashMap<Language, usize> = HashMap::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
            let lang = match ext {
                "py" => Language::Python,
                "js" => Language::JavaScript,
                "ts" => Language::TypeScript,
                "rs" => Language::Rust,
                "go" => Language::Go,
                "java" => Language::Java,
                _ => continue,
            };

            *counts.entry(lang).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(lang, _)| lang)
        .unwrap_or(Language::Unknown)
}
