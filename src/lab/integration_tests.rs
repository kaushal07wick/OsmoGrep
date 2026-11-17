use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{cell::Cell, path::PathBuf};

struct TestRoot(PathBuf);
impl std::ops::Deref for TestRoot {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Fixture {
    store: Store,
    capsule: replay::Capsule,
    root: TestRoot,
}
impl Fixture {
    fn new() -> Self {
        let (root, capsule) = runtime::replay_tests::fixture();
        let store = Store::at(&root.join("lab.sqlite3"), &root).unwrap();
        Self {
            store,
            capsule,
            root: TestRoot(root),
        }
    }
}
struct Phone {
    calls: Vec<(String, Value, bool)>,
    pixel: u8,
    physical: bool,
    fail: bool,
}
impl behavior::Device for Phone {
    fn call(&mut self, name: &str, args: Value, closing: bool) -> Result<Value> {
        if name == "phone_act" || name == "phone_observe" {
            assert_eq!(args["sessionId"], "lease");
        }
        self.calls.push((name.into(), args, closing));
        match name {
            "phone_devices" => Ok(
                json!({"structuredContent":{"registered":[{"name":"test","kind":if self.physical{"device"}else{"simulator"}}]}}),
            ),
            "phone_session" => {
                Ok(json!({"structuredContent":{"ok":true,"session":{"id":"lease","name":"test"}}}))
            }
            "phone_act" if self.fail => Err("controlled uncertain input outcome".into()),
            "phone_observe" | "phone_act" => {
                let image =
                    image::RgbaImage::from_pixel(2, 2, image::Rgba([self.pixel, 0, 0, 255]));
                let mut bytes = std::io::Cursor::new(vec![]);
                image::DynamicImage::ImageRgba8(image)
                    .write_to(&mut bytes, image::ImageOutputFormat::Png)
                    .unwrap();
                Ok(
                    json!({"structuredContent":{"ok":true,"action":{"verified":true}},"content":[{"type":"image","mimeType":"image/png","data":STANDARD.encode(bytes.into_inner())}]}),
                )
            }
            _ => panic!("Unexpected device method {name}"),
        }
    }
}
fn phone() -> Phone {
    Phone {
        calls: vec![],
        pixel: 0,
        physical: false,
        fail: false,
    }
}
fn flow(store: &Store) -> Record {
    behavior::plan(store,"Save flow",&json!({"platform":"iphone","target":"test","sources":["app.py"],"steps":[{"tool":"phone_act","arguments":{"action":{"kind":"tap","target":{"identifier":"save"}}},"expect":{"/action/verified":true}}]})).unwrap()
}
#[test]
fn ui_runs_bind_fresh_sessions_preserve_real_images_and_export_before_after() {
    let f = Fixture::new();
    let flow = flow(&f.store);
    let mut device = phone();
    let before = behavior::run(
        &f.root,
        &f.store,
        &flow,
        "before",
        "build-1",
        &mut device,
        &|| false,
    )
    .unwrap();
    assert_eq!(before.data["state"], "finished");
    assert!(before.data["console_errors"].is_null());
    device.pixel = 255;
    let after = behavior::run(
        &f.root,
        &f.store,
        &flow,
        "after",
        "build-2",
        &mut device,
        &|| false,
    )
    .unwrap();
    assert_eq!(
        device
            .calls
            .iter()
            .filter(|(m, _, closing)| m == "phone_session" && *closing)
            .count(),
        2
    );
    let actions = device
        .calls
        .iter()
        .enumerate()
        .filter(|(_, v)| v.0 == "phone_act")
        .collect::<Vec<_>>();
    assert_eq!(actions.len(), 2);
    for (index, _) in &actions {
        assert_eq!(device.calls[index - 1].0, "phone_observe");
    }
    assert_ne!(actions[0].1 .1["requestId"], actions[1].1 .1["requestId"]);
    let comparison =
        behavior::compare(&f.store, "Save before / after", &before.id, &after.id).unwrap();
    assert_eq!(
        comparison.data["steps"][0]["visual_difference"]["changed_pixels"],
        4
    );
    behavior::export(&f.root, &f.store, &comparison.id, "comparison.html").unwrap();
    let html = fs::read_to_string(f.root.join("comparison.html")).unwrap();
    assert_eq!(html.matches("data:image/png;base64,").count(), 2);
    assert!(html.contains("Content-Security-Policy"));
    assert!(behavior::export(&f.root, &f.store, &comparison.id, "comparison.html").is_err());
    assert_eq!(
        fs::read_to_string(f.root.join("app.py")).unwrap(),
        "VALUE = 1\n"
    );
}
#[test]
fn device_failure_releases_control_and_physical_targets_are_refused() {
    let f = Fixture::new();
    let flow = flow(&f.store);
    let mut device = phone();
    device.fail = true;
    let run = behavior::run(
        &f.root,
        &f.store,
        &flow,
        "before",
        "test",
        &mut device,
        &|| false,
    )
    .unwrap();
    assert_eq!(run.data["state"], "failed");
    assert!(device
        .calls
        .last()
        .is_some_and(|(method, args, closing)| method == "phone_session"
            && args["operation"] == "release"
            && *closing));
    device.calls.clear();
    device.fail = false;
    device.physical = true;
    let run = behavior::run(
        &f.root,
        &f.store,
        &flow,
        "before",
        "test",
        &mut device,
        &|| false,
    )
    .unwrap();
    assert_eq!(run.data["state"], "failed");
    assert_eq!(device.calls.len(), 1);
}
struct Vm {
    calls: Vec<String>,
    starts: Cell<usize>,
    identity: bool,
    base: String,
    tree: String,
    reproducer: String,
}
impl crate::intelligence::verify::Sandbox for Vm {
    fn call(&mut self, name: &str, args: Value) -> crate::intelligence::Result<Value> {
        self.calls.push(name.into());
        Ok(match name {
            "start" => {
                assert!(Path::new(args["repo"].as_str().unwrap())
                    .join("app.py")
                    .exists());
                self.base = args["base"].as_str().unwrap().into();
                self.tree = crate::intelligence::git::text(
                    Path::new(args["repo"].as_str().unwrap()),
                    &["write-tree"],
                )
                .unwrap();
                self.starts.set(self.starts.get() + 1);
                json!({"session":format!("vm-{}",self.starts.get())})
            }
            "reproducer" => {
                self.reproducer =
                    crate::file_guard::digest(args["content"].as_str().unwrap().as_bytes());
                json!({})
            }
            "compare" => {
                json!({"command":args["command"],"reproducer_sha256":self.reproducer,"oracle_sources":["CONTRACT.md:1"],"base":{"revision":self.base,"exit_code":1,"failure_kind":"assertion","timed_out":false,"cancelled":false},"candidate":{"tree":self.tree,"exit_code":0,"timed_out":false,"cancelled":false},"environment_equivalent":true,"oracle_validated":self.identity,"reproducer_identical":true,"environment_fingerprint":"fixture-v1"})
            }
            "check" => json!({"integrity_preserved":true}),
            _ => json!({}),
        })
    }
}
#[test]
fn comparison_and_fault_campaigns_keep_independent_runs_and_host_source() {
    let f = Fixture::new();
    let left = f.store.put("replay", "Left", json!(f.capsule)).unwrap();
    let mut right = f.capsule.clone();
    right.files.get_mut("app.py").unwrap().content = "VALUE = 3\n".into();
    right.files.get_mut("app.py").unwrap().sha256 = crate::file_guard::digest(b"VALUE = 3\n");
    let right = f.store.put("replay", "Right", json!(right)).unwrap();
    let comparison = compare::plan(&f.store, "Compare", &left.id, &right.id).unwrap();
    let mut vm = Vm {
        calls: vec![],
        starts: Cell::new(0),
        identity: false,
        base: String::new(),
        tree: String::new(),
        reproducer: String::new(),
    };
    let result = compare::run(&f.root, &f.store, &comparison, &mut vm, &|| false).unwrap();
    assert_eq!(result.data["left_result"]["state"], "inconclusive");
    assert!(result.data["left_result"]["command_duration_ms"].is_null());
    vm.identity = true;
    let scenario=scenarios::plan(&f.store,"Timezone cases",&json!({"replay":left.id,"cases":[{"name":"UTC","environment":{"TZ":"UTC"},"repetitions":2},{"name":"Locale","environment":{"LC_ALL":"C"}}]})).unwrap();
    let result = scenarios::run(&f.root, &f.store, &scenario, &mut vm, &|| false).unwrap();
    assert_eq!(result.data["results"].as_array().unwrap().len(), 3);
    assert_eq!(
        result.data["results"][0]["outcome"]["state"],
        "check_passed"
    );
    assert_eq!(vm.calls.iter().filter(|m| *m == "start").count(), 5);
    assert_eq!(vm.calls.iter().filter(|m| *m == "close").count(), 5);
    assert_eq!(
        fs::read_to_string(f.root.join("app.py")).unwrap(),
        "VALUE = 1\n"
    );
    let checkouts = f.root.join(".context/osmogrep-lab/checkouts");
    assert_eq!(fs::read_dir(checkouts).unwrap().count(), 0);
    assert!(scenarios::plan(&f.store,"Too many",&json!({"replay":left.id,"cases":[{"name":"Over budget","environment":{},"repetitions":33}]})).is_err());
}
#[test]
fn corrections_show_source_drift_and_do_not_touch_the_source() {
    let f = Fixture::new();
    let record = context::correction(
        &f.root,
        &f.store,
        "Value contract",
        &json!({"text":"The documented value is one.","sources":["app.py"]}),
    )
    .unwrap();
    assert_eq!(
        context::inspect(&f.root, &f.store, &record.id).unwrap()["stale_sources"],
        json!([])
    );
    fs::write(f.root.join("app.py"), "VALUE = 7\n").unwrap();
    assert_eq!(
        context::inspect(&f.root, &f.store, &record.id).unwrap()["stale_sources"],
        json!(["app.py"])
    );
    context::configure(
        &f.root,
        &f.store,
        &json!({"enabled":false,"clear_corrections":true}),
    )
    .unwrap();
    assert_eq!(
        context::inspect(&f.root, &f.store, &record.id).unwrap()["active"],
        false
    );
}

#[test]
fn cancelling_a_campaign_closes_the_allocated_vm_and_keeps_partial_evidence() {
    let f = Fixture::new();
    let replay = f.store.put("replay", "Source", json!(f.capsule)).unwrap();
    let plan = scenarios::plan(
        &f.store,
        "Cancellation",
        &json!({"replay":replay.id,"cases":[{"name":"Repeated","environment":{},"repetitions":3}]}),
    )
    .unwrap();
    let stopped = std::rc::Rc::new(Cell::new(false));
    struct CancelVm {
        stop: std::rc::Rc<Cell<bool>>,
        closed: bool,
    }
    impl crate::intelligence::verify::Sandbox for CancelVm {
        fn call(&mut self, name: &str, _args: Value) -> crate::intelligence::Result<Value> {
            if name == "start" {
                self.stop.set(true);
                return Ok(json!({"session":"cancelled-fixture"}));
            }
            assert_eq!(name, "close");
            self.closed = true;
            Ok(json!({}))
        }
    }
    let mut vm = CancelVm {
        stop: stopped.clone(),
        closed: false,
    };
    let record = scenarios::run(&f.root, &f.store, &plan, &mut vm, &|| stopped.get()).unwrap();
    assert!(vm.closed);
    assert_eq!(record.data["state"], "cancelled");
    assert_eq!(record.data["results"].as_array().unwrap().len(), 1);
    assert_eq!(
        f.store.get(&plan.id).unwrap().data["results"],
        record.data["results"]
    );
}
