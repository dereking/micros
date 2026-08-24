use micro_compiler::compile_source;
use micro_ir::{FunctionKind, Instruction, ScalarType, TextSource, UiKind, ValueSource};

#[test]
fn lowers_counter_studio_states_ui_bindings_and_handlers() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("app.ts", source).unwrap();

    assert_eq!(image.states.len(), 16);
    assert_eq!(
        image
            .states
            .iter()
            .filter(|slot| slot.ty == ScalarType::Number)
            .count(),
        13
    );
    assert_eq!(
        image
            .states
            .iter()
            .filter(|slot| slot.ty == ScalarType::String)
            .count(),
        3
    );

    let bindings: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Binding(_)))
        .collect();
    assert_eq!(bindings.len(), 26);

    let handlers: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .collect();
    assert_eq!(handlers.len(), 32);

    // The Add button's handler increments count (state 0) and presses (state 1).
    let add = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button)
        .unwrap();
    let add_handler = &image.functions[add.on_click.unwrap().0 as usize];
    let stores: Vec<_> = add_handler
        .code
        .iter()
        .filter_map(|op| match op {
            Instruction::StoreState(id) => Some(id.0),
            _ => None,
        })
        .collect();
    assert_eq!(stores, [0, 1]);
    // The Double handler doubles count and increments presses.
    let double = image
        .nodes
        .iter()
        .find(|node| {
            matches!(node.kind, UiKind::Button)
                && matches!(
                    image.nodes[node.id.0 as usize].text,
                    Some(TextSource::Constant(_))
                )
                && image.functions[node.on_click.unwrap().0 as usize]
                    .code
                    .iter()
                    .any(|op| matches!(op, Instruction::Mul))
        })
        .unwrap();
    let loads: Vec<_> = image.functions[double.on_click.unwrap().0 as usize]
        .code
        .iter()
        .filter_map(|op| match op {
            Instruction::LoadState(id) => Some(id.0),
            _ => None,
        })
        .collect();
    assert!(loads.contains(&0) && loads.contains(&1));

    let root = &image.nodes[image.root.0 as usize];
    assert_eq!(root.kind, UiKind::Tabview);
    assert_eq!(root.children.len(), 4);
    // Tab titles are interned in the tabview's options.
    assert_eq!(root.options.len(), 4);
    // The first tab content is a column holding the styled title and the Add
    // button (the counter family).
    let first_tab = &image.nodes[root.children[0].0 as usize];
    assert_eq!(first_tab.kind, UiKind::Column);
    assert!(
        first_tab
            .children
            .iter()
            .any(|child| matches!(
                image.nodes[child.0 as usize].text,
                Some(TextSource::Constant(_))
            ))
    );
    // The Add button is the first button and has a handler.
    let add = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button)
        .unwrap();
    assert!(add.on_click.is_some());

    // The row holds the battery text, progress bar, and the drain/charge buttons.
    let row = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Row)
        .unwrap();
    assert_eq!(row.children.len(), 4);

    // The progress bar binds its fraction to the level state.
    let progress = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Progress)
        .unwrap();
    assert!(matches!(
        progress.value,
        Some(ValueSource::Binding(_))
    ));

    // The switch binds its checked state and carries an onToggle handler.
    let switch = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Switch)
        .unwrap();
    assert!(matches!(switch.value, Some(ValueSource::Binding(_))));
    assert!(switch.on_click.is_some());
}

#[test]
fn lowers_local_while_if_and_assignment() {
    let source = r#"
const count = state(0);
ui.mount(ui.button("Run", { onClick: () => {
  let i = 0;
  while (i < 2) { i++; }
  if (i === 2) { count.value = i; }
} }));
"#;
    let image = compile_source("flow.ts", source).unwrap();
    let handler = image
        .functions
        .iter()
        .find(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .unwrap();
    assert_eq!(handler.locals, 1);
    assert!(
        handler
            .code
            .iter()
            .any(|op| matches!(op, Instruction::Jump(_)))
    );
    assert!(
        handler
            .code
            .iter()
            .any(|op| matches!(op, Instruction::JumpIfFalse(_)))
    );
    assert!(
        handler
            .code
            .iter()
            .any(|op| matches!(op, Instruction::StoreState(_)))
    );
}

#[test]
fn not_equal_lowers_to_equality_then_negation() {
    let source = r#"
const count = state(0);
ui.mount(ui.button("Run", { onClick: () => {
  if (count.value !== 0) { count.value = 1; }
} }));
"#;
    let image = compile_source("flow.ts", source).unwrap();
    let handler = image
        .functions
        .iter()
        .find(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .unwrap();
    // `!==` must be Eq followed by Not (not a plain Eq, which would invert the
    // condition). Find an Eq that is immediately followed by Not.
    let has_eq_then_not = handler
        .code
        .windows(2)
        .any(|window| matches!((&window[0], &window[1]), (Instruction::Eq, Instruction::Not)));
    assert!(has_eq_then_not, "`!==` must lower to Eq + Not");
}

#[test]
fn lowers_app_manifest_into_metadata() {
    let image = compile_source(
        "meta.ts",
        r#"
app({ id: "counter", name: "Counter", icon: "C" });
const count = state(0);
ui.mount(ui.column([ui.text(bind(() => count.value))]));
"#,
    )
    .unwrap();
    assert_eq!(image.metadata.id, "counter");
    assert_eq!(image.metadata.name, "Counter");
    assert_eq!(image.metadata.icon, "C");

    // Without an app() call the manifest falls back to a default.
    let fallback = compile_source(
        "plain.ts",
        r#"
const count = state(0);
ui.mount(ui.column([ui.text(bind(() => count.value))]));
"#,
    )
    .unwrap();
    assert_eq!(fallback.metadata.name, "App");
}

#[test]
fn lowers_host_calls_into_requests_and_instructions() {
    let source = r#"
const list = state("");
const note = state("");
const refresh = state(0);
ui.mount(ui.column([
  ui.text(bind(() => `chip: ${device.chip()} flash: ${device.flashBytes()}`)),
  ui.text(bind(() => `wifi: ${net.wifiState()} ${net.wifiSsid()} ${refresh.value}`)),
  ui.button("backlight", { onClick: () => { device.setBacklight(4); } }),
  ui.button("scan", { onClick: () => { net.scanWifi((l) => { list.value = l; }); } }),
  ui.button("http", { onClick: () => { net.httpGet("http://x", (r) => { note.value = r; }); } }),
]));
"#;
    let image = compile_source("host.ts", source).unwrap();

    use micro_ir::HostCallKind;
    let kinds: Vec<_> = image.host_requests.iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&HostCallKind::DeviceChip));
    assert!(kinds.contains(&HostCallKind::DeviceFlashBytes));
    assert!(kinds.contains(&HostCallKind::NetWifiState));
    assert!(kinds.contains(&HostCallKind::NetWifiSsid));
    assert!(kinds.contains(&HostCallKind::DeviceSetBacklight));
    assert!(kinds.contains(&HostCallKind::NetScanWifi));
    assert!(kinds.contains(&HostCallKind::NetHttpGet));

    // Actions carry no result; async requests carry a 1-arg callback.
    let backlight = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::DeviceSetBacklight)
        .unwrap();
    assert_eq!(backlight.result_kind, None);
    assert_eq!(backlight.arg_kinds, [ScalarType::Number]);

    let scan = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::NetScanWifi)
        .unwrap();
    assert!(scan.callback.is_some());
    let callback = &image.functions[scan.callback.unwrap().0 as usize];
    assert_eq!(callback.arg_count, 1);
    assert!(matches!(callback.kind, FunctionKind::Handler(_)));

    let http = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::NetHttpGet)
        .unwrap();
    assert_eq!(http.arg_kinds, [ScalarType::String]);
    assert!(http.callback.is_some());

    // Every handler with a host action emits a HostCall instruction.
    let host_callers = image
        .functions
        .iter()
        .filter(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, Instruction::HostCall(_)))
        })
        .count();
    assert!(host_callers >= 3);
}

#[test]
fn lowers_os_calls_into_requests() {
    let source = r#"
const app0 = state("");
ui.mount(ui.column([
  ui.text(bind(() => os.appName(0))),
  ui.button(bind(() => os.appIcon(0)), { onClick: () => { os.launchIndex(0); } }),
  ui.button("back", { onClick: () => { os.goBack(); } }),
  ui.button("poll", { onClick: () => { os.delay(500, (s) => { app0.value = s; }); } }),
]));
"#;
    let image = compile_source("os.ts", source).unwrap();

    use micro_ir::HostCallKind;
    let kinds: Vec<_> = image.host_requests.iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&HostCallKind::OsAppName));
    assert!(kinds.contains(&HostCallKind::OsAppIcon));
    assert!(kinds.contains(&HostCallKind::OsLaunchIndex));
    assert!(kinds.contains(&HostCallKind::OsGoBack));
    assert!(kinds.contains(&HostCallKind::OsDelay));

    let app_name = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::OsAppName)
        .unwrap();
    assert_eq!(app_name.arg_kinds, [ScalarType::Number]);
    assert_eq!(app_name.result_kind, Some(ScalarType::String));

    let launch = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::OsLaunchIndex)
        .unwrap();
    assert_eq!(launch.arg_kinds, [ScalarType::Number]);
    assert_eq!(launch.result_kind, None);

    let delay = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::OsDelay)
        .unwrap();
    assert_eq!(delay.arg_kinds, [ScalarType::Number]);
    assert!(delay.callback.is_some());

    // os.appIcon(0) inside a ui.button label lowers to a Binding text source.
    let button = image
        .nodes
        .iter()
        .find(|node| matches!(node.kind, UiKind::Button))
        .unwrap();
    assert!(matches!(button.text, Some(TextSource::Binding(_))));
}

#[test]
fn expands_align_and_anchors_to_layout_spec() {
    let source = r#"
const x = state(0);
ui.mount(ui.column([
  ui.place(ui.text("a"), { align: "bottom" }),
  ui.place(ui.text("b"), { align: "bottom", anchor: { bottom: 8 } }),
  ui.place(ui.text("c"), { left: 16, top: 20 }),
  ui.place(ui.text("d"), { align: "client" }),
  ui.place(ui.text("e"), { left: 0, anchor: { left: 0, right: 0, bottom: 0 } }),
]));
"#;
    let image = compile_source("place.ts", source).unwrap();
    let layouts = image
        .nodes
        .iter()
        .filter_map(|node| node.layout)
        .collect::<Vec<_>>();
    use micro_ir::{AnchorSpec, LayoutSpec};
    let expected = [
        LayoutSpec { left: None, top: None, width: None, height: None, anchor: AnchorSpec { left: Some(0.0), top: None, right: Some(0.0), bottom: Some(0.0) } }, // align bottom
        LayoutSpec { left: None, top: None, width: None, height: None, anchor: AnchorSpec { left: Some(0.0), top: None, right: Some(0.0), bottom: Some(8.0) } }, // align bottom + anchor.bottom:8
        LayoutSpec { left: Some(16.0), top: Some(20.0), width: None, height: None, anchor: AnchorSpec::default() }, // left+top only
        LayoutSpec { left: None, top: None, width: None, height: None, anchor: AnchorSpec { left: Some(0.0), top: Some(0.0), right: Some(0.0), bottom: Some(0.0) } }, // align client
        LayoutSpec { left: Some(0.0), top: None, width: None, height: None, anchor: AnchorSpec { left: Some(0.0), top: None, right: Some(0.0), bottom: Some(0.0) } }, // lt.left + anchor l/r/b
    ];
    for spec in expected {
        assert!(layouts.contains(&spec), "missing {spec:?} in {layouts:?}");
    }
    assert_eq!(layouts.len(), expected.len());
}
