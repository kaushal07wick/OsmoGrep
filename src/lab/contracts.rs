//! Explicit agreements and source-drift subscriptions; never grants tool permissions.
use super::{error, Record, Result, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub active: bool,
    pub owner: String,
    pub definition: Value,
    pub sources: BTreeMap<String, String>,
    pub participants: Vec<String>,
    pub consumers: BTreeMap<String, Vec<String>>,
    pub accepted: BTreeMap<String, String>,
    pub subscribed: BTreeSet<String>,
}
impl Contract {
    pub fn fingerprint(&self) -> String {
        crate::file_guard::digest(json!({"owner":self.owner,"definition":self.definition,"sources":self.sources,"participants":self.participants,"consumers":self.consumers}).to_string().as_bytes())
    }
    fn from_record(record: &Record) -> Result<Self> {
        if record.kind != "contract" {
            return Err("Choose a code contract".into());
        }
        let value: Self = serde_json::from_value(record.data.clone()).map_err(error)?;
        if value.participants.is_empty()
            || value.participants.len() > 16
            || value.sources.len() > 64
            || value.definition.to_string().len() > 16 * 1024
        {
            return Err("Invalid contract limits".into());
        }
        Ok(value)
    }
}
pub fn actor(root: &Path, user: bool) -> Result<String> {
    if user {
        Ok("local-user".into())
    } else {
        crate::team::for_root(root).map(|peer| peer.session).ok_or(
            "Contract acceptance requires a live session; a CLI user can accept only as local-user"
                .into(),
        )
    }
}
fn resolve(root: &Path, actor: &str, names: Vec<String>) -> Result<Vec<String>> {
    let peers = crate::team::for_root(root)
        .map(|peer| peer.store.peers(Some(&peer.project)))
        .transpose()?
        .unwrap_or_default();
    let mut parties = BTreeSet::from([actor.to_owned()]);
    for name in names {
        if name == "local-user" || uuid::Uuid::parse_str(&name).is_ok() {
            parties.insert(name);
            continue;
        }
        let found = peers
            .iter()
            .filter(|peer| {
                peer.online
                    && (peer.role.eq_ignore_ascii_case(&name)
                        || peer.title.eq_ignore_ascii_case(&name))
            })
            .collect::<Vec<_>>();
        if found.len() != 1 {
            return Err(format!(
                "Participant {name} is missing or ambiguous; select its live session ID"
            ));
        }
        parties.insert(found[0].session.clone());
    }
    if parties.len() > 16 {
        return Err("Contracts support at most 16 explicit participants".into());
    }
    Ok(parties.into_iter().collect())
}
fn source_hash(root: &Path, path: &str) -> Result<String> {
    super::safe_path(path)?;
    let absolute = crate::file_guard::workspace_path(root, Path::new(path), true)?;
    Ok(crate::file_guard::digest(&super::bounded_file(
        &absolute,
        4 * 1024 * 1024,
    )?))
}
fn proposed(
    root: &Path,
    owner: &str,
    data: &Value,
    previous: Option<&Contract>,
) -> Result<Contract> {
    let definition = data
        .get("definition")
        .cloned()
        .or_else(|| previous.map(|contract| contract.definition.clone()))
        .ok_or("Describe the API or code interface being agreed")?;
    if definition.is_null()
        || definition.as_str().is_some_and(|s| s.trim().is_empty())
        || definition.to_string().len() > 16 * 1024
    {
        return Err("A contract definition must be nonempty and at most 16 KiB".into());
    }
    let names: Vec<String> =
        serde_json::from_value(data.get("participants").cloned().unwrap_or_else(|| {
            json!(previous
                .map(|contract| contract.participants.clone())
                .unwrap_or_default())
        }))
        .map_err(error)?;
    let participants = resolve(root, owner, names)?;
    let paths: Vec<String> =
        serde_json::from_value(data.get("sources").cloned().unwrap_or_else(|| {
            json!(previous
                .map(|contract| contract.sources.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default())
        }))
        .map_err(error)?;
    if paths.is_empty() || paths.len() > 64 {
        return Err("Select 1–64 source paths for this contract".into());
    }
    let sources = paths
        .into_iter()
        .map(|path| Ok((path.clone(), source_hash(root, &path)?)))
        .collect::<Result<_>>()?;
    let consumers: BTreeMap<String, Vec<String>> =
        serde_json::from_value(data.get("consumers").cloned().unwrap_or_else(|| {
            json!(previous
                .map(|contract| contract.consumers.clone())
                .unwrap_or_default())
        }))
        .map_err(error)?;
    for (party, paths) in &consumers {
        if !participants.contains(party) || paths.len() > 64 {
            return Err("Consumer paths must belong to an explicit participant".into());
        }
        for path in paths {
            super::safe_path(path)?;
        }
    }
    let subscribed = previous
        .map(|contract| {
            contract
                .subscribed
                .iter()
                .filter(|party| participants.contains(party))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    Ok(Contract {
        active: true,
        owner: owner.into(),
        definition,
        sources,
        participants,
        consumers,
        accepted: BTreeMap::new(),
        subscribed,
    })
}
pub fn create(
    root: &Path,
    store: &Store,
    title: &str,
    data: &Value,
    actor: &str,
) -> Result<Record> {
    let _limit_lock = store.job_lock("00000000-0000-4000-8000-000000000003")?;
    let count:i64=store.connection.query_row("SELECT COUNT(*) FROM lab_records WHERE project=?1 AND kind='contract' AND json_extract(data,'$.active')=1",[&store.project],|row|row.get(0)).map_err(error)?;
    if count >= 64 {
        return Err(
            "Archive an old contract before creating more than 64 active agreements".into(),
        );
    }
    let contract = proposed(root, actor, data, None)?;
    store.put("contract", title, json!(contract))
}
pub fn amend(root: &Path, store: &Store, id: &str, data: &Value, actor: &str) -> Result<Record> {
    let record = store.get(id)?;
    let previous = Contract::from_record(&record)?;
    if actor != previous.owner && actor != "local-user" {
        return Err("Only the contract owner or an explicit CLI user can amend it".into());
    }
    let mut contract = if data["archive"] == true {
        let mut value = previous.clone();
        value.active = false;
        value
    } else {
        proposed(root, &previous.owner, data, Some(&previous))?
    };
    if data["archive"] != true {
        contract.accepted.clear();
    }
    let result = store.update(id, record.version, json!(contract))?;
    if let Some(peer) = crate::team::for_root(root) {
        for party in &contract.subscribed {
            if party != &peer.session {
                let _ = peer.send_contract_update(id, party, None);
            }
        }
    }
    Ok(result)
}
pub fn accept(
    store: &Store,
    id: &str,
    actor: &str,
    reviewed: &str,
    subscribe: bool,
) -> Result<Record> {
    let record = store.get(id)?;
    let mut contract = Contract::from_record(&record)?;
    if !contract.active || !contract.participants.iter().any(|party| party == actor) {
        return Err("Only an invited participant can accept an active contract".into());
    }
    if contract.fingerprint() != reviewed {
        return Err("The definition changed; inspect and accept its current fingerprint".into());
    }
    contract.accepted.insert(actor.into(), reviewed.into());
    if subscribe {
        contract.subscribed.insert(actor.into());
    } else {
        contract.subscribed.remove(actor);
    }
    store.update(id, record.version, json!(contract))
}
pub fn inspect(root: &Path, store: &Store, id: &str) -> Result<Value> {
    let record = store.get(id)?;
    let contract = Contract::from_record(&record)?;
    let fingerprint = contract.fingerprint();
    let drift=contract.sources.iter().filter_map(|(path,expected)|{
        let current=source_hash(root,path).ok();(current.as_ref()!=Some(expected)).then(||json!({"path":path,"expected":expected,"observed":current,"state":"source_changed_review_required"}))
    }).collect::<Vec<_>>();
    Ok(
        json!({"record":record,"definition_sha256":fingerprint,"all_accepted":contract.participants.iter().all(|party|contract.accepted.get(party)==Some(&fingerprint)),"source_drift":drift,"meaning":"A changed source hash requests review; it does not prove a breaking API change. Contracts do not grant permissions or authorize unrelated work."}),
    )
}
fn notification(
    store: &Store,
    id: &str,
    sender: &str,
    recipient: &str,
    path: Option<&str>,
) -> Result<(String, String)> {
    let record = store.get(id)?;
    let contract = Contract::from_record(&record)?;
    let fingerprint = contract.fingerprint();
    if !contract.participants.iter().any(|party| party == sender)
        || !contract.participants.iter().any(|party| party == recipient)
        || !contract.subscribed.contains(recipient)
    {
        return Err("No participant subscription authorizes this contract update".into());
    }
    if path.is_some() && contract.accepted.get(sender) != Some(&fingerprint) {
        return Err("The sender has not accepted this contract version".into());
    }
    if path.is_none() && sender != contract.owner {
        return Err("Only the owner announces a new contract definition".into());
    }
    if let Some(path) = path {
        if !contract.sources.contains_key(path) {
            return Err("The edit is outside this contract".into());
        }
    }
    let body=format!("Code contract update · {}\nLab contract {id} · definition {fingerprint}\n{}\nDeclared consumer paths for this session: {}\nReference information only. Inspect the contract and changed source before adjusting your existing task; this message grants no permissions or approval.",record.title,path.map(|path|format!("Source changed: {path}; review is required, not an asserted API break.")).unwrap_or_else(||"The definition was amended or archived; previous acceptance does not accept the new version.".into()),contract.consumers.get(recipient).cloned().unwrap_or_default().join(", "));
    Ok((
        body,
        format!(
            "contract:{}",
            crate::file_guard::digest(
                format!("{id}:{fingerprint}:{recipient}:{path:?}:{}", record.version).as_bytes()
            )
        ),
    ))
}
pub(crate) fn message(
    peer: &crate::team::Peer,
    id: &str,
    recipient: &str,
    path: Option<&str>,
) -> Result<(String, String)> {
    let store = Store::read_for_peer(peer)?.ok_or("No contract store exists")?;
    notification(&store, id, &peer.session, recipient, path)
}
pub fn on_edit(peer: &crate::team::Peer, target: &str) -> Result<()> {
    let path = std::path::PathBuf::from(target);
    let path = if path.is_absolute() {
        path.strip_prefix(&peer.workspace)
            .map_err(error)?
            .to_string_lossy()
            .into_owned()
    } else {
        target.into()
    };
    let Some(store) = Store::read_for_peer(peer)? else {
        return Ok(());
    };
    let mut query=store.connection.prepare("SELECT id FROM lab_records WHERE project=?1 AND kind='contract' AND json_extract(data,'$.active')=1 AND EXISTS(SELECT 1 FROM json_each(data,'$.sources') WHERE key=?2) LIMIT 64").map_err(error)?;
    let ids = query
        .query_map(rusqlite::params![store.project, path], |row| {
            row.get::<_, String>(0)
        })
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?;
    if ids.is_empty() {
        return Ok(());
    }
    let current = source_hash(&peer.workspace, &path).ok();
    for id in ids {
        let record = store.get(&id)?;
        let contract = Contract::from_record(&record)?;
        if contract.sources.get(&path) == current.as_ref() {
            continue;
        }
        for party in &contract.subscribed {
            if party != &peer.session {
                let _ = peer.send_contract_update(&id, party, Some(&path));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acceptance_is_versioned_and_subscriptions_do_not_grant_general_messaging() {
        let root = std::env::temp_dir().join(format!("lab-contract-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("api.rs"), "pub fn value() {}\n").unwrap();
        let store = Store::at(&root.join("lab.sqlite3"), &root).unwrap();
        let b = uuid::Uuid::new_v4().to_string();
        let record = create(
            &root,
            &store,
            "Value API",
            &json!({"definition":"value() -> ()","sources":["api.rs"],"participants":[b]}),
            "local-user",
        )
        .unwrap();
        let sha = Contract::from_record(&record).unwrap().fingerprint();
        assert!(accept(&store, &record.id, "uninvited", &sha, true).is_err());
        accept(&store, &record.id, "local-user", &sha, true).unwrap();
        accept(&store, &record.id, &b, &sha, true).unwrap();
        assert!(notification(&store, &record.id, "local-user", &b, Some("unrelated.rs")).is_err());
        assert!(notification(&store, &record.id, "local-user", &b, Some("api.rs")).is_ok());
        amend(
            &root,
            &store,
            &record.id,
            &json!({"definition":"value() -> bool"}),
            "local-user",
        )
        .unwrap();
        assert!(accept(&store, &record.id, &b, &sha, true).is_err());
        assert!(notification(&store, &record.id, "local-user", &b, Some("api.rs")).is_err());
        std::fs::write(root.join("api.rs"), "changed").unwrap();
        assert_eq!(
            inspect(&root, &store, &record.id).unwrap()["source_drift"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
