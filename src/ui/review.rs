//! Review contiguous diff hunks without overwriting later changes.
use crate::state::{AgentState, DiffSnapshot};
use similar::{DiffTag, TextDiff};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug)]
pub struct Hunk {
    pub id: String,
    pub path: String,
    pub after: std::sync::Arc<str>,
    pub old_start: usize,
    pub old_end: usize,
    pub new_start: usize,
    pub new_end: usize,
    pub old_text: String,
    pub new_text: String,
}
pub fn hunks(changes: &[DiffSnapshot]) -> Vec<Hunk> {
    let mut files = BTreeMap::<String, (String, String)>::new();
    for change in changes
        .iter()
        .filter(|c| !c.tool.starts_with("preview:") && !c.tool.starts_with("live:"))
    {
        let file = files
            .entry(change.target.clone())
            .or_insert_with(|| (change.before.clone(), change.after.clone()));
        file.1 = change.after.clone();
    }
    let mut hunks = Vec::new();
    for (path, (before, after)) in files {
        let after: std::sync::Arc<str> = after.into();
        let old = before.split_inclusive('\n').collect::<Vec<_>>();
        let new = after.split_inclusive('\n').collect::<Vec<_>>();
        for op in TextDiff::from_lines(before.as_str(), after.as_ref())
            .ops()
            .iter()
            .filter(|op| op.tag() != DiffTag::Equal)
        {
            let a = op.old_range();
            let b = op.new_range();
            let id = crate::file_guard::digest(
                format!(
                    "{path}\0{}\0{}\0{}",
                    a.start,
                    old[a.clone()].concat(),
                    new[b.clone()].concat()
                )
                .as_bytes(),
            );
            hunks.push(Hunk {
                id,
                path: path.clone(),
                after: after.clone(),
                old_start: a.start,
                old_end: a.end,
                new_start: b.start,
                new_end: b.end,
                old_text: old[a].concat(),
                new_text: new[b].concat(),
            });
        }
    }
    hunks
}
pub fn preview(hunk: &Hunk) -> String {
    format!(
        "{}\n@@ -{},{} +{},{} @@\n{}{}",
        hunk.path,
        hunk.old_start + 1,
        hunk.old_end - hunk.old_start,
        hunk.new_start + 1,
        hunk.new_end - hunk.new_start,
        hunk.old_text
            .lines()
            .map(|l| format!("- {l}\n"))
            .collect::<String>(),
        hunk.new_text
            .lines()
            .map(|l| format!("+ {l}\n"))
            .collect::<String>()
    )
}
fn restored(hunk: &Hunk, current: &str) -> Result<String, String> {
    if current != hunk.after.as_ref() {
        return Err("This file changed after the displayed diff. Refresh review; later edits were preserved.".into());
    }
    let lines = hunk.after.split_inclusive('\n').collect::<Vec<_>>();
    let restored = format!(
        "{}{}{}",
        lines[..hunk.new_start].concat(),
        hunk.old_text,
        lines[hunk.new_end..].concat()
    );
    Ok(restored)
}
pub fn revert(state: &mut AgentState, hunk: &Hunk) -> Result<(), String> {
    if state.ui.agent_running {
        return Err("Pause or finish the active turn before reverting a hunk.".into());
    }
    if state.permission_profile == crate::state::PermissionProfile::ReadOnly
        || crate::sandbox::mode() == "read-only"
    {
        return Err("Read-only mode prevents reverting files.".into());
    }
    let path = crate::file_guard::workspace_path(&state.repo_root, Path::new(&hunk.path), true)?;
    let peer = crate::team::for_root(&state.repo_root);
    let _lease = peer
        .as_ref()
        .map(|p| p.claim(&[hunk.path.clone()], false))
        .transpose()?;
    let current = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && hunk.after.is_empty() => {
            String::new()
        }
        Err(e) => return Err(e.to_string()),
    };
    let restored = restored(hunk, &current)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut checkpoint = crate::checkpoints::Turn::start(
        &state.repo_root,
        &id,
        state.conversation.messages.clone(),
    )?;
    let args = serde_json::json!({"path":hunk.path});
    checkpoint.capture(&state.repo_root, "write_file", &args)?;
    let expected = path.exists().then_some(current.as_bytes());
    crate::file_guard::replace(&path, expected, restored.as_bytes())?;
    checkpoint.complete(
        &state.repo_root,
        "write_file",
        &args,
        &serde_json::json!({"after":restored}),
    )?;
    state.session_changes.push(DiffSnapshot {
        tool: "review".into(),
        target: hunk.path.clone(),
        before: hunk.after.to_string(),
        after: restored,
    });
    state.ui.diff_snapshot.clear();
    state.ui.diff_active = false;
    state.ui.reviewed_hunks.remove(&hunk.id);
    crate::persistence::save(state)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hunk_revert_preserves_other_hunks_and_rejects_intervening_edits() {
        let root = std::env::temp_dir().join(format!("review-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let before = "a\nb\nc\nd\ne\n";
        let after = "A\nb\nc\nd\nE\n";
        let changes = vec![DiffSnapshot {
            tool: "edit_file".into(),
            target: "a".into(),
            before: before.into(),
            after: after.into(),
        }];
        let pieces = hunks(&changes);
        assert_eq!(pieces.len(), 2);
        std::fs::write(root.join("a"), restored(&pieces[0], after).unwrap()).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a")).unwrap(),
            "a\nb\nc\nd\nE\n"
        );
        let h = hunks(&changes).pop().unwrap();
        std::fs::write(root.join("a"), "user edit").unwrap();
        assert!(restored(&h, "user edit").is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("a")).unwrap(),
            "user edit"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
