use std::cell::RefCell;
use std::rc::Rc;

use micro_compiler::compile_source;
use micro_core::{Event, MicroUiTree, RenderError, RenderPatch, RenderPort, Runtime};
use micro_ir::{FunctionId, HostCallKind, HostRequest, UiKind};
use micro_vm::{HostAccess, Value, VmError};

/// A simulated host that serves the shell's `os.*` / `net.*` / `device.*`
/// reads and records every `os.launchIndex(i)` request.
struct SimHost {
    apps: Vec<(String, String)>,
    launches: Rc<RefCell<Vec<u32>>>,
    pending: Vec<(FunctionId, Value)>,
}

impl SimHost {
    fn new(launches: Rc<RefCell<Vec<u32>>>) -> Self {
        Self {
            apps: vec![("Counter".into(), "C".into())],
            launches,
            pending: Vec::new(),
        }
    }
}

impl HostAccess for SimHost {
    fn call(&mut self, request: &HostRequest, args: &[Value]) -> Result<Option<Value>, VmError> {
        Ok(match request.kind {
            HostCallKind::OsAppName => {
                let index = numeric(args, 0) as usize;
                Some(Value::String(
                    self.apps.get(index).map_or("", |app| &app.0).to_owned(),
                ))
            }
            HostCallKind::OsAppIcon => {
                let index = numeric(args, 0) as usize;
                Some(Value::String(
                    self.apps.get(index).map_or("", |app| &app.1).to_owned(),
                ))
            }
            HostCallKind::OsLaunchIndex => {
                self.launches.borrow_mut().push(numeric(args, 0) as u32);
                None
            }
            HostCallKind::OsGoBack | HostCallKind::NetWifiConnect | HostCallKind::NetWifiDisconnect
            | HostCallKind::DeviceSetBacklight => None,
            HostCallKind::OsDelay => {
                self.pending
                    .push((request.callback.unwrap(), Value::String(String::new())));
                None
            }
            HostCallKind::NetScanWifi => {
                self.pending.push((
                    request.callback.unwrap(),
                    Value::String("ap1\nap2".into()),
                ));
                None
            }
            HostCallKind::NetWifiState => Some(Value::String("off".into())),
            HostCallKind::NetWifiSsid => Some(Value::String(String::new())),
            HostCallKind::DeviceName => Some(Value::String("micro-os".into())),
            HostCallKind::DeviceChip => Some(Value::String("ESP32-S3 (sim)".into())),
            HostCallKind::DeviceFlashBytes | HostCallKind::DevicePsramBytes => {
                Some(Value::Number(8388608.0))
            }
            HostCallKind::DeviceResetReason => Some(Value::String("power-on (sim)".into())),
            HostCallKind::DeviceBacklight => Some(Value::Number(3.0)),
            HostCallKind::NetHttpGet => {
                self.pending.push((
                    request.callback.unwrap(),
                    Value::String("HTTP 200\nOK".into()),
                ));
                None
            }
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
    }
}

fn numeric(args: &[Value], index: usize) -> f64 {
    match args.get(index) {
        Some(Value::Number(value)) => *value,
        _ => 0.0,
    }
}

#[derive(Default)]
struct RecordingRenderer {
    created: Vec<MicroUiTree>,
    patches: Vec<RenderPatch>,
}

impl RenderPort for RecordingRenderer {
    fn create_tree(&mut self, tree: &MicroUiTree) -> Result<(), RenderError> {
        self.created.push(tree.clone());
        Ok(())
    }

    fn apply(&mut self, patches: &[RenderPatch]) -> Result<(), RenderError> {
        self.patches.extend_from_slice(patches);
        Ok(())
    }
}

#[test]
fn shell_launcher_renders_installed_app_icons() {
    let source = include_str!("../../../apps/shell/app.ts");
    let image = compile_source("apps/shell/app.ts", source).unwrap();
    assert_eq!(image.metadata.id, "shell");

    let launches = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost::new(launches.clone())),
    )
    .unwrap();

    // The launcher renders the installed app's icon (button) + name (text).
    let (launch_handler, name_text) = {
        let tree = &runtime.renderer().created[0];
        let icon_button = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Button && node.text == "C")
            .expect("installed app icon tile");
        let name = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Text && node.text == "Counter")
            .expect("installed app name label");
        (icon_button.on_click.unwrap(), name.text.clone())
    };

    // Tapping the icon tile launches installed app index 0.
    runtime.enqueue(Event::Activate(launch_handler));
    runtime.tick().unwrap();
    assert_eq!(*launches.borrow(), vec![0]);
    assert_eq!(runtime.renderer().created.len(), 1);
    assert_eq!(name_text, "Counter");
}
