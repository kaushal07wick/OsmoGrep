//! Versioned JSONL control channel, shared by integrations and explicitly requested workers.
use crate::{
    agent::{Agent, AgentEvent, RunControl},
    state::{DiffSnapshot, PermissionProfile},
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, Read},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

pub fn serve(
    root: PathBuf,
    resume: Option<String>,
    profile: PermissionProfile,
    auto_approve: bool,
) -> Result<(), String> {
    let mut state = crate::init_state();
    state.repo_root = root.canonicalize().map_err(|e| e.to_string())?;
    state.session.update_latest = false;
    if let Some(id) = resume {
        crate::persistence::load_id(&mut state, &id, false)?;
    }
    state.permission_profile = profile;
    state.ui.auto_approve = auto_approve;
    crate::persistence::save(&state)?;
    let agent = Agent::new();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        loop {
            let mut bytes = Vec::new();
            match input
                .by_ref()
                .take(1024 * 1024 + 1)
                .read_until(b'\n', &mut bytes)
            {
                Ok(0) => break,
                Ok(_) if bytes.len() <= 1024 * 1024 => {
                    if tx
                        .send(serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string()))
                        .is_err()
                    {
                        return;
                    }
                }
                _ => {
                    let _ = tx.send(Err("RPC record exceeds 1 MiB or input failed".into()));
                    break;
                }
            }
        }
    });
    let mut emitter = crate::HeadlessEmitter::new(true, auto_approve);
    emitter.emit_json_value(json!({"type":"ready","protocol_version":1,"session_id":state.session.id,"methods":["prompt","steer","cancel","permission","session","compact","recover","checkpoints","rewind","shutdown"]}));
    let mut queue = VecDeque::<String>::new();
    let mut active: Option<(RunControl, mpsc::Receiver<AgentEvent>)> = None;
    let mut permissions = HashMap::<String, mpsc::Sender<bool>>::new();
    let mut input_closed = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Ok(request)) => {
                let id = request.get("id").cloned();
                let result = (|| -> Result<Value, String> {
                    match request["method"].as_str().ok_or("Missing RPC method")? {
                        "prompt" => {
                            let prompt = request["params"]["prompt"]
                                .as_str()
                                .filter(|s| !s.trim().is_empty())
                                .ok_or("Missing prompt")?;
                            if queue.len() >= 64 {
                                return Err("Follow-up queue is full".into());
                            }
                            queue.push_back(prompt.into());
                            Ok(json!({"queued":active.is_some(),"session_id":state.session.id}))
                        }
                        "steer" => {
                            let text = request["params"]["text"]
                                .as_str()
                                .ok_or("Missing steer text")?;
                            active
                                .as_ref()
                                .ok_or("No active run")?
                                .0
                                .steer_tx
                                .send(text.into())
                                .map_err(|e| e.to_string())?;
                            Ok(json!({"steered":true}))
                        }
                        "cancel" => {
                            if let Some((control, _)) = &active {
                                control.cancel.cancel();
                            }
                            for (_, reply) in permissions.drain() {
                                let _ = reply.send(false);
                            }
                            queue.clear();
                            Ok(json!({"cancel_requested":true}))
                        }
                        "permission" => {
                            let key = request["params"]["request_id"]
                                .as_str()
                                .ok_or("Missing permission request ID")?;
                            let allow = request["params"]["allow"]
                                .as_bool()
                                .ok_or("Missing allow boolean")?;
                            permissions
                                .remove(key)
                                .ok_or("Unknown or expired permission request")?
                                .send(allow)
                                .map_err(|e| e.to_string())?;
                            Ok(json!({"delivered":true}))
                        }
                        "compact" => {
                            if active.is_some() {
                                return Err(
                                    "Finish or cancel the active run before compaction".into()
                                );
                            }
                            let (tx, events) = mpsc::channel();
                            let control = agent.compact(
                                state.repo_root.clone(),
                                state.conversation.messages.clone(),
                                tx,
                            );
                            active = Some((control, events));
                            Ok(json!({"compacting":true}))
                        }
                        "recover" => {
                            if active.is_some() {
                                return Err(
                                    "Finish or cancel the active run before recovery".into()
                                );
                            }
                            let id = request["params"]["run_id"]
                                .as_str()
                                .ok_or("Missing run_id")?;
                            crate::persistence::recover(&mut state, id)?;
                            Ok(
                                json!({"session_id":state.session.id,"messages":state.conversation.messages}),
                            )
                        }
                        "checkpoints" => Ok(json!(crate::checkpoints::list(&state.repo_root)?)),
                        "rewind" => {
                            if active.is_some() {
                                return Err(
                                    "Finish or cancel the active run before rewinding".into()
                                );
                            }
                            let id = request["params"]["checkpoint_id"]
                                .as_str()
                                .ok_or("Missing checkpoint_id")?;
                            let mode = request["params"]["mode"]
                                .as_str()
                                .ok_or("Choose code, conversation, or both")?;
                            if !matches!(mode, "code" | "conversation" | "both") {
                                return Err("Choose code, conversation, or both".into());
                            }
                            if mode != "conversation" && profile == PermissionProfile::ReadOnly {
                                return Err(
                                    "Read-only profile cannot restore workspace files".into()
                                );
                            }
                            let turn = crate::checkpoints::load(&state.repo_root, id)?;
                            let count = if mode != "conversation" {
                                crate::checkpoints::restore_files(&state.repo_root, &turn)?
                            } else {
                                0
                            };
                            if mode != "code" {
                                crate::persistence::new_session(&mut state)?;
                                state.conversation.set_messages(turn.conversation);
                            }
                            crate::persistence::save(&state)?;
                            Ok(json!({"restored_files":count,"session_id":state.session.id}))
                        }
                        "session" => Ok(
                            json!({"id":state.session.id,"name":state.session_name,"messages":state.conversation.messages,"running":active.is_some(),"queued":queue.len()}),
                        ),
                        "shutdown" => {
                            input_closed = true;
                            queue.clear();
                            if let Some((control, _)) = &active {
                                control.cancel.cancel();
                            }
                            for (_, reply) in permissions.drain() {
                                let _ = reply.send(false);
                            }
                            Ok(json!({"shutting_down":true}))
                        }
                        _ => Err("Unknown RPC method".into()),
                    }
                })();
                if let Some(id) = id {
                    emitter.emit_json_value(match result {
                        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                        Err(error) => {
                            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":error}})
                        }
                    });
                }
            }
            Ok(Err(error)) => {
                emitter.emit_json_value(json!({"type":"protocol_error","message":error}))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => input_closed = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let mut done = false;
        if let Some((_, events)) = &active {
            loop {
                let event = match events.try_recv() {
                    Ok(event) => event,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !done {
                            emitter.emit_json_value(json!({"type":"error","message":"Agent worker disconnected unexpectedly; recover the saved run before retrying mutations"}));
                        }
                        done = true;
                        break;
                    }
                };
                match &event {
                    AgentEvent::ConversationUpdate(messages) => {
                        state.conversation.set_messages(messages.clone());
                        crate::persistence::save(&state)?;
                    }
                    AgentEvent::ToolDiff {
                        tool,
                        target,
                        before,
                        after,
                    } => {
                        state.session_changes.push(DiffSnapshot {
                            tool: tool.clone(),
                            target: target.clone(),
                            before: before.clone(),
                            after: after.clone(),
                        });
                    }
                    _ => {}
                }
                if let AgentEvent::PermissionRequest {
                    tool_name,
                    args_summary,
                    reply_tx,
                } = event
                {
                    if auto_approve {
                        let _ = reply_tx.send(true);
                    } else {
                        let id = uuid::Uuid::new_v4().to_string();
                        permissions.insert(id.clone(), reply_tx);
                        emitter.emit_json_value(json!({"type":"permission_request","request_id":id,"tool_name":tool_name,"args_summary":args_summary}));
                    }
                    continue;
                }
                done |= matches!(event, AgentEvent::Done);
                if matches!(event, AgentEvent::Error(_) | AgentEvent::Cancelled) {
                    queue.clear();
                }
                emitter.emit_event(event);
            }
        }
        if done {
            active = None;
            permissions.clear();
            crate::persistence::save(&state)?;
        }
        if input_closed {
            if let Some((control, _)) = &active {
                control.cancel.cancel();
            }
            for (_, reply) in permissions.drain() {
                let _ = reply.send(false);
            }
            if active.is_none() {
                break;
            }
        }
        if active.is_none() && !input_closed {
            if let Some(prompt) = queue.pop_front() {
                crate::logger::log_user_input(&mut state, &prompt);
                let (tx, events) = mpsc::channel();
                let control = agent.spawn(
                    state.repo_root.clone(),
                    prompt,
                    state.conversation.messages.clone(),
                    state.steer.clone(),
                    profile,
                    auto_approve,
                    tx,
                );
                active = Some((control, events));
            }
        }
    }
    crate::persistence::save(&state)
}
