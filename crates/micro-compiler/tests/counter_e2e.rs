use micro_compiler::compile_source;
use micro_core::{Event, MicroUiTree, RenderError, RenderPatch, RenderPort, Runtime};
use micro_ir::{
    FunctionId, HostCallKind, HostRequest, TextStyle, UiKind,
};
use micro_vm::{HostAccess, Value, VmError};

/// A simulated host so the counter's `device.*` / `net.*` reads succeed when
/// the App runs without a real platform.
#[derive(Default)]
struct SimHost {
    pending: Vec<(FunctionId, Value)>,
}

impl HostAccess for SimHost {
    fn call(&mut self, request: &HostRequest, _args: &[Value]) -> Result<Option<Value>, VmError> {
        Ok(match request.kind {
            HostCallKind::DeviceName => Some(Value::String("micro-os".into())),
            HostCallKind::DeviceChip => Some(Value::String("ESP32-S3 (sim)".into())),
            HostCallKind::DeviceFlashBytes | HostCallKind::DevicePsramBytes => {
                Some(Value::Number(8388608.0))
            }
            HostCallKind::DeviceResetReason => Some(Value::String("power-on (sim)".into())),
            HostCallKind::DeviceBacklight => Some(Value::Number(3.0)),
            HostCallKind::DeviceSetBacklight
            | HostCallKind::NetWifiConnect
            | HostCallKind::NetWifiDisconnect => None,
            HostCallKind::NetWifiState => Some(Value::String("connected".into())),
            HostCallKind::NetWifiSsid => Some(Value::String("micro-demo".into())),
            HostCallKind::NetScanWifi | HostCallKind::NetHttpGet => {
                self.pending.push((
                    request.callback.unwrap(),
                    Value::String("HTTP 200\nOK".into()),
                ));
                None
            }
            HostCallKind::OsAppName | HostCallKind::OsAppIcon => Some(Value::String(String::new())),
            HostCallKind::OsLaunchIndex | HostCallKind::OsGoBack | HostCallKind::OsDelay => None,
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
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
fn compiles_loads_and_clicks_the_real_counter() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("apps/counter/app.ts", source).unwrap();
    let mut runtime =
        Runtime::new_with_host(image, RecordingRenderer::default(), 10_000, Box::new(SimHost::default()))
            .unwrap();
    let (count_id, count_style, add_style, handler) = {
        let tree = &runtime.renderer().created[0];
        let count_text = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Text && node.text.starts_with("Count: "))
            .expect("count display text");
        let add_button = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Button && node.text == "Add")
            .expect("Add button");
        assert_eq!(count_text.text, "Count: 0");
        (
            count_text.id,
            count_text.text_style.clone(),
            add_button.text_style.clone(),
            add_button.on_click.unwrap(),
        )
    };
    assert_eq!(count_style, Some(TextStyle::DEFAULT_TEXT));
    assert_eq!(add_style, Some(TextStyle::DEFAULT_BUTTON));

    runtime.enqueue(Event::Activate(handler));
    runtime.tick().unwrap();
    runtime.enqueue(Event::Activate(handler));
    runtime.tick().unwrap();

    let count_patches: Vec<_> = runtime
        .renderer()
        .patches
        .iter()
        .filter_map(|patch| match patch {
            RenderPatch::SetText { node, text } if *node == count_id => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(count_patches, ["Count: 1", "Count: 2"]);
    assert_eq!(runtime.renderer().created.len(), 1);
}
