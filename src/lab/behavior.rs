//! Bounded UI flows against existing, explicitly named companion targets.
use super::{error, Record, Result, Store};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub tool: String,
    #[serde(default = "empty")]
    pub arguments: Value,
    /// A JSON pointer and exact expected value in the companion observation.
    pub expect: Option<BTreeMap<String, Value>>,
}
fn empty() -> Value {
    json!({})
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    pub platform: String,
    pub target: String,
    pub sources: Vec<String>,
    pub steps: Vec<Step>,
}
impl Flow {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.platform.as_str(), "mac" | "iphone")
            || self.target.is_empty()
            || self.target.len() > 100
            || self
                .target
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_'))
        {
            return Err("Choose mac or iphone and an explicit named companion target".into());
        }
        if self.steps.is_empty() || self.steps.len() > 32 || self.sources.len() > 64 {
            return Err("A flow needs 1–32 steps and at most 64 source references".into());
        }
        for source in &self.sources {
            super::safe_path(source)?;
        }
        for step in &self.steps {
            let allowed = if self.platform == "mac" {
                matches!(
                    step.tool.as_str(),
                    "screenshot" | "click" | "move" | "drag" | "scroll" | "type" | "key" | "status"
                )
            } else {
                matches!(
                    step.tool.as_str(),
                    "phone_observe" | "phone_act" | "phone_wait" | "phone_capabilities"
                )
            };
            if !allowed || !step.arguments.is_object() || step.arguments.to_string().len() > 16384 {
                return Err("Unsupported flow operation or oversized arguments; lifecycle, installation and shell commands are excluded".into());
            }
            if step.arguments.get("phone").is_some()
                || step.arguments.get("sessionId").is_some()
                || step.arguments.get("requestId").is_some()
                || step.arguments["action"]["target"].get("ref").is_some()
            {
                return Err("Flows bind their own target and request IDs; use stable accessibility identifiers, not saved observation refs".into());
            }
            if step.tool == "phone_act"
                && !matches!(
                    step.arguments["action"]["kind"].as_str(),
                    Some("tap" | "fill" | "type" | "swipe" | "scroll" | "press" | "launch")
                )
            {
                return Err(
                    "Unsupported phone action; use a dedicated simulator and stable selectors"
                        .into(),
                );
            }
            if let Some(expect) = &step.expect {
                if expect.len() > 16
                    || expect
                        .keys()
                        .any(|key| !key.starts_with('/') || key.len() > 512)
                {
                    return Err(
                        "Assertions need at most 16 JSON pointer / expected value pairs".into(),
                    );
                }
            }
            if crate::intelligence::cli::redact(&step.arguments.to_string())
                != step.arguments.to_string()
            {
                return Err("Do not save credentials in a device flow".into());
            }
        }
        Ok(())
    }
    pub fn from_record(record: &Record) -> Result<Self> {
        if record.kind != "behavior" || record.data["type"] != "flow" {
            return Err("Select a saved UI flow".into());
        }
        let flow: Self = serde_json::from_value(record.data["flow"].clone()).map_err(error)?;
        flow.validate()?;
        Ok(flow)
    }
}
pub fn plan(store: &Store, title: &str, data: &Value) -> Result<Record> {
    let flow: Flow = serde_json::from_value(data.clone()).map_err(error)?;
    flow.validate()?;
    store.put(
        "behavior",
        title,
        json!({"type":"flow","flow":flow,"state":"planned"}),
    )
}
pub trait Device {
    fn call(&mut self, tool: &str, arguments: Value, closing: bool) -> Result<Value>;
}
pub struct Companion<'a> {
    pub server: &'a str,
    pub deadline: Instant,
    pub cancel: &'a dyn Fn() -> bool,
}
impl Device for Companion<'_> {
    fn call(&mut self, tool: &str, args: Value, closing: bool) -> Result<Value> {
        let timeout = if closing {
            std::time::Duration::from_secs(10)
        } else {
            self.deadline.saturating_duration_since(Instant::now())
        };
        if timeout.is_zero() {
            return Err("Device flow deadline reached".into());
        }
        crate::mcp::call_tool(self.server, tool, &args, timeout, &|| {
            !closing && (self.cancel)()
        })
    }
}
pub fn connect(root: &Path, flow: &Flow) -> Result<String> {
    flow.validate()?;
    let device = if flow.platform == "mac" {
        crate::workspace::Device::Mac
    } else {
        crate::workspace::Device::Phone
    };
    let program = crate::workspace::resolve(device, root)?;
    let server = format!(
        "lab-{}-{}",
        device.program(),
        &crate::file_guard::digest(flow.target.as_bytes())[..16]
    );
    crate::workspace::register_named_controls(device, root, &flow.target, &program, &server)?;
    Ok(server)
}
pub fn unpack(value: Value) -> Result<Value> {
    if value["isError"] == true || value.get("error").is_some_and(|v| !v.is_null()) {
        return Err(format!(
            "Companion operation failed: {}",
            crate::intelligence::cli::redact(&value.to_string())
                .chars()
                .take(1200)
                .collect::<String>()
        ));
    }
    let content = value["content"].as_array().cloned().unwrap_or_default();
    let data=value.get("structuredContent").cloned().or_else(||content.iter().filter_map(|v|v["text"].as_str()).find_map(|s|serde_json::from_str::<Value>(s).ok()))
        .unwrap_or_else(|| json!({"text":content.iter().filter_map(|v|v["text"].as_str()).collect::<Vec<_>>().join("\n")}));
    if data["ok"] == false {
        return Err("Companion reported an unsuccessful operation; observe before retrying".into());
    }
    let image = content
        .iter()
        .find(|v| v["type"] == "image" && v["mimeType"] == "image/png")
        .cloned();
    if let Some(image) = &image {
        decode(image)?;
    }
    Ok(json!({"observation":data,"image":image}))
}
fn decode(value: &Value) -> Result<image::RgbaImage> {
    let encoded = value["data"]
        .as_str()
        .ok_or("Screenshot has no encoded PNG")?;
    if encoded.len() > 4 * 1024 * 1024 {
        return Err("Screenshot exceeds the 4 MiB capture budget".into());
    }
    let bytes = STANDARD.decode(encoded).map_err(error)?;
    let mut reader =
        image::io::Reader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode().map_err(error)?.to_rgba8())
}
fn hashes(root: &Path, paths: &[String]) -> Result<Value> {
    let mut values = serde_json::Map::new();
    for path in paths {
        let absolute = crate::file_guard::workspace_path(root, Path::new(path), false)?;
        values.insert(
            path.clone(),
            json!(crate::file_guard::digest(&super::bounded_file(
                &absolute,
                4 * 1024 * 1024
            )?)),
        );
    }
    Ok(Value::Object(values))
}
pub fn run(
    root: &Path,
    store: &Store,
    source: &Record,
    phase: &str,
    build: &str,
    device: &mut dyn Device,
    cancel: &dyn Fn() -> bool,
) -> Result<Record> {
    let flow = Flow::from_record(source)?;
    if !matches!(phase, "before" | "after")
        || build.trim().is_empty()
        || build.len() > 200
        || build.chars().any(char::is_control)
    {
        return Err("Choose before/after and identify the installed app build".into());
    }
    let _lock = store.job_lock(&source.id)?;
    let _device_lock = store
        .resource_lock(&format!("device:{}:{}", flow.platform, flow.target))
        .map_err(|error| format!("Another flow may own {}: {error}", flow.target))?;
    let record=store.put("execution",&format!("{} · {phase}",super::short_title(&source.title)),json!({"type":"behavior_run","flow":source.id,"phase":phase,"build":build,"target":flow.target,"platform":flow.platform,"source_hashes":hashes(root,&flow.sources)?,"steps":[],"state":"running","build_identity":"User-declared; installed app provenance is not attested"}))?;
    let mut data = record.data.clone();
    let mut acquired = false;
    let mut session = Value::Null;
    let result = (|| -> Result<()> {
        if flow.platform == "iphone" {
            let devices = unpack(device.call(
                "phone_devices",
                json!({"includeConnectedDevices":false}),
                false,
            )?)?;
            let found = devices["observation"]["registered"]
                .as_array()
                .and_then(|a| a.iter().find(|v| v["name"] == flow.target));
            if !found.is_some_and(|v| v["kind"] == "simulator") {
                return Err("The named target must be an already registered dedicated simulator; no physical-phone fallback or runtime download".into());
            }
            // Attempt release even when a start reply is lost after acquisition.
            acquired = true;
            let opened = unpack(device.call(
                "phone_session",
                json!({"phone":flow.target,"operation":"start"}),
                false,
            )?)?;
            session = opened["observation"]["session"]["id"].clone();
            if session.as_str().is_none() || opened["observation"]["session"]["name"] != flow.target
            {
                return Err("Companion did not attest the named simulator session".into());
            }
        } else {
            let status = unpack(device.call("status", json!({}), false)?)?;
            if !status["observation"]["text"]
                .as_str()
                .is_some_and(|s| s.contains(&format!("VM '{}' is running.", flow.target)))
            {
                return Err("The named Mac VM did not confirm readiness".into());
            }
        }
        for (index, step) in flow.steps.iter().enumerate() {
            if cancel() {
                return Err("Device flow cancelled".into());
            }
            let mut args = step.arguments.clone();
            if flow.platform == "iphone" {
                args["phone"] = json!(flow.target);
                args["sessionId"] = session.clone();
                args["requestId"] = json!(uuid::Uuid::new_v4().to_string());
                if step.tool == "phone_act" {
                    unpack(device.call(
                        "phone_observe",
                        json!({"phone":flow.target,"sessionId":session,"includeImage":false}),
                        false,
                    )?)?;
                    args["observeAfter"] = json!(true);
                    args["includeImage"] = json!(true);
                }
            } else if step.tool != "status" && step.tool != "screenshot" {
                args["screenshot"] = json!(true);
            }
            let started = Instant::now();
            let observation = unpack(device.call(&step.tool, args, false)?)?;
            let passed = step.expect.as_ref().map(|checks| {
                checks.iter().all(|(pointer, expected)| {
                    observation["observation"].pointer(pointer) == Some(expected)
                })
            });
            let saved=store.put("execution","UI flow observation",super::clean_value(json!({"type":"behavior_step","run":record.id,"index":index,"tool":step.tool,"elapsed_ms":started.elapsed().as_millis() as u64,"assertions_passed":passed,"evidence":observation}))?)?;
            data["steps"].as_array_mut().unwrap().push(json!(saved.id));
            let current = store.get(&record.id)?;
            store.update(&record.id, current.version, data.clone())?;
            if passed == Some(false) {
                return Err(format!("Assertion failed at step {}", index + 1));
            }
        }
        Ok(())
    })();
    if acquired {
        let mut release = json!({"phone":flow.target,"operation":"release"});
        if !session.is_null() {
            release["sessionId"] = session;
        }
        data["cleanup_error"] = device
            .call("phone_session", release, true)
            .and_then(unpack)
            .err()
            .map(Value::String)
            .unwrap_or(Value::Null);
    }
    data["state"] = json!(if result.is_ok() {
        "finished"
    } else if cancel() {
        "cancelled"
    } else {
        "failed"
    });
    data["error"] = result.err().map(Value::String).unwrap_or(Value::Null);
    data["console_errors"] = Value::Null;
    data["network_requests"] = Value::Null;
    let current = store.get(&record.id)?;
    store.update(&record.id, current.version, super::clean_value(data)?)
}
fn run_record(store: &Store, id: &str) -> Result<Record> {
    let record = store.get(id)?;
    if record.kind != "execution" || record.data["type"] != "behavior_run" {
        return Err("Choose a completed UI flow run".into());
    }
    Ok(record)
}
pub fn compare(store: &Store, title: &str, before: &str, after: &str) -> Result<Record> {
    let a = run_record(store, before)?;
    let b = run_record(store, after)?;
    if a.data["flow"] != b.data["flow"]
        || a.data["target"] != b.data["target"]
        || a.data["phase"] != "before"
        || b.data["phase"] != "after"
        || a.data["state"] != "finished"
        || b.data["state"] != "finished"
    {
        return Err(
            "Compare successful before/after runs of the same saved flow and target".into(),
        );
    }
    let left = a.data["steps"]
        .as_array()
        .ok_or("Missing flow observations")?;
    let right = b.data["steps"]
        .as_array()
        .ok_or("Missing flow observations")?;
    if left.len() != right.len() {
        return Err("Flow runs have different step counts".into());
    }
    let mut changes = vec![];
    for (a, b) in left.iter().zip(right) {
        let a = store.get(a.as_str().ok_or("Invalid observation ID")?)?;
        let b = store.get(b.as_str().ok_or("Invalid observation ID")?)?;
        let x = &a.data["evidence"]["image"];
        let y = &b.data["evidence"]["image"];
        let difference = if x.is_null() || y.is_null() {
            Value::Null
        } else {
            pixel_difference(&decode(x)?, &decode(y)?)
        };
        changes.push(json!({"before":a.id,"after":b.id,"visual_difference":difference,"before_assertions":a.data["assertions_passed"],"after_assertions":b.data["assertions_passed"]}));
    }
    store.put("behavior",title,json!({"type":"comparison","before":before,"after":after,"steps":changes,"source_before":a.data["source_hashes"],"source_after":b.data["source_hashes"],"note":"Pixel differences are observations, not proof of a regression. App build identity is user-declared; console/network metrics are unavailable from these companions."}))
}
fn pixel_difference(a: &image::RgbaImage, b: &image::RgbaImage) -> Value {
    if a.dimensions() != b.dimensions() {
        return json!({"dimensions_changed":true,"before":a.dimensions(),"after":b.dimensions(),"changed_pixels":null});
    }
    let changed = a.pixels().zip(b.pixels()).filter(|(x, y)| x != y).count();
    json!({"dimensions_changed":false,"width":a.width(),"height":a.height(),"changed_pixels":changed,"total_pixels":a.width() as u64*a.height() as u64})
}
fn html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn export(root: &Path, store: &Store, id: &str, path: &str) -> Result<Value> {
    let record = store.get(id)?;
    if record.kind != "behavior" || record.data["type"] != "comparison" {
        return Err("Choose a UI behavior comparison".into());
    }
    let mut output=format!("<!doctype html><meta charset=utf-8><meta http-equiv=Content-Security-Policy content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\"><title>{}</title><style>body{{background:#000;color:#fff;font:15px monospace;margin:32px}}section{{display:grid;grid-template-columns:1fr 1fr;gap:20px}}img{{max-width:100%;border:1px solid #333}}h1{{color:#ef984f}}pre{{white-space:pre-wrap}}</style><h1>{}</h1><p>Before / After · exact pixel comparison · declared build identity</p>",html(&record.title),html(&record.title));
    for step in record.data["steps"]
        .as_array()
        .ok_or("Missing observations")?
    {
        output.push_str("<section>");
        for side in ["before", "after"] {
            let evidence = store.get(step[side].as_str().ok_or("Missing observation ID")?)?;
            output.push_str(&format!("<div><h2>{side}</h2>"));
            if let Some(encoded) = evidence.data["evidence"]["image"]["data"].as_str() {
                decode(&evidence.data["evidence"]["image"])?;
                output.push_str(&format!(
                    "<img alt='{side} screenshot' src='data:image/png;base64,{}'>",
                    html(encoded)
                ));
            }
            output.push_str(&format!(
                "<pre>{}</pre></div>",
                html(&evidence.data["evidence"]["observation"].to_string())
            ));
        }
        output.push_str(&format!(
            "</section><pre>{}</pre>",
            html(&step["visual_difference"].to_string())
        ));
        if output.len() > super::MAX_BYTES {
            return Err("Visual export exceeds 8 MiB; export a smaller flow".into());
        }
    }
    super::write_export_bytes(root, path, output.as_bytes())?;
    Ok(json!({"path":path,"bytes":output.len()}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flow_rejects_stale_refs_and_lifecycle_and_measures_pixels() {
        let mut flow = Flow {
            platform: "iphone".into(),
            target: "test".into(),
            sources: vec![],
            steps: vec![Step {
                tool: "phone_act".into(),
                arguments: json!({"action":{"kind":"tap","target":{"ref":"old"}}}),
                expect: None,
            }],
        };
        assert!(flow.validate().is_err());
        flow.steps[0].arguments = json!({"action":{"kind":"tap","target":{"identifier":"save"}}});
        assert!(flow.validate().is_ok());
        flow.steps[0].tool = "phone_boot".into();
        assert!(flow.validate().is_err());
        let a = image::RgbaImage::new(2, 2);
        let mut b = a.clone();
        b.put_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        assert_eq!(pixel_difference(&a, &b)["changed_pixels"], 1);
        assert!(unpack(json!({"isError":true,"structuredContent":{"ok":true}})).is_err());
    }
}
