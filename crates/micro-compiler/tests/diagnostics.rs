use micro_compiler::{compile_source, validate_source};

#[test]
fn accepts_the_counter_sdk_surface() {
    let source = include_str!("../../../apps/counter/app.ts");
    validate_source("app.ts", source).unwrap();
}

#[test]
fn rejects_unsupported_constructs_with_locations() {
    let cases = [
        ("class.ts", "class App {}", "MTS001", 1, 1),
        ("function.ts", "function run() {}", "MTS001", 1, 1),
        ("async.ts", "const run = async () => 1;", "MTS001", 1, 13),
        ("import.ts", "import value from 'pkg';", "MTS001", 1, 1),
        ("jsx.ts", "const view = <Button />;", "MTS000", 1, 23),
        ("spread.ts", "const x = [...items];", "MTS001", 1, 12),
        ("destructure.ts", "const { value } = item;", "MTS001", 1, 7),
        (
            "dynamic.ts",
            "const x = state(0); x['value'];",
            "MTS001",
            1,
            21,
        ),
        ("global.ts", "console.log('no');", "MTS001", 1, 1),
    ];

    for (path, source, code, line, column) in cases {
        let errors = validate_source(path, source).unwrap_err();
        assert_eq!(errors[0].code, code, "{path}: {errors:?}");
        assert_eq!((errors[0].line, errors[0].column), (line, column), "{path}");
        assert!(
            errors[0]
                .to_string()
                .starts_with(&format!("{path}:{line}:{column}"))
        );
    }
}

#[test]
fn accepts_host_call_surface() {
    let source = r#"
const list = state("");
const note = state("");
const refresh = state(0);
ui.mount(ui.column([
  ui.text(bind(() => `chip: ${device.chip()} flash: ${device.flashBytes()}`)),
  ui.text(bind(() => `wifi: ${net.wifiState()} ${net.wifiSsid()} ${refresh.value}`)),
  ui.button("backlight", { onClick: () => { device.setBacklight(4); } }),
  ui.button("connect", { onClick: () => { net.wifiConnect("SSID", "pass"); } }),
  ui.button("off", { onClick: () => { net.wifiDisconnect(); } }),
  ui.button("scan", { onClick: () => { net.scanWifi((l) => { list.value = l; }); } }),
  ui.button("http", { onClick: () => { net.httpGet("http://x", (r) => { note.value = r; }); } }),
  ui.text(bind(() => `list: ${list.value} http: ${note.value}`)),
]));
"#;
    validate_source("host.ts", source).unwrap();
}

#[test]
fn rejects_unknown_host_calls() {
    let cases = [
        ("device-bogus.ts", "ui.mount(ui.text(device.bogus()));", "MTS001", 1, 18),
        ("net-bogus.ts", "ui.mount(ui.text(net.bogus()));", "MTS001", 1, 18),
        ("net-member.ts", "const x = net.wifiState;", "MTS001", 1, 11),
        ("os-bogus.ts", "ui.mount(ui.text(os.bogus()));", "MTS001", 1, 18),
    ];
    for (path, source, code, line, column) in cases {
        let errors = validate_source(path, source).unwrap_err();
        assert_eq!(errors[0].code, code, "{path}: {errors:?}");
        assert_eq!((errors[0].line, errors[0].column), (line, column), "{path}");
    }
}

#[test]
fn rejects_host_call_wrong_arity() {
    let errors = validate_source("arity.ts", "ui.mount(ui.button('x', { onClick: () => { device.setBacklight(); } }));")
        .unwrap_err();
    assert_eq!(errors[0].code, "MTS002");
}

#[test]
fn rejects_async_host_call_inside_a_binding() {
    let source = r#"
const list = state("");
ui.mount(ui.text(bind(() => { net.scanWifi((l) => { list.value = l; }); return "x"; })));
"#;
    let errors = compile_source("binding-async.ts", source).unwrap_err();
    assert_eq!(errors[0].code, "MTS013");
}

#[test]
fn rejects_unknown_button_props_and_multiple_mounts() {
    let unknown = r#"ui.mount(ui.button("Add", { onTap: () => 1 }));"#;
    let errors = validate_source("prop.ts", unknown).unwrap_err();
    assert_eq!(errors[0].code, "MTS002");

    let mounts = "ui.mount(ui.text('one'));\nui.mount(ui.text('two'));";
    let errors = validate_source("mount.ts", mounts).unwrap_err();
    assert_eq!(errors[0].code, "MTS003");
    assert_eq!((errors[0].line, errors[0].column), (2, 1));
}

#[test]
fn rejects_non_scalar_widget_values() {
    let cases = [
        ("progress-bool.ts", "ui.mount(ui.progress(true));"),
        ("switch-number.ts", "ui.mount(ui.switch(5));"),
    ];
    for (path, source) in cases {
        let errors = compile_source(path, source).unwrap_err();
        assert_eq!(errors[0].code, "MTS012", "{path}: {errors:?}");
        assert_eq!(errors.len(), 1, "{path}: {errors:?}");
    }
}

#[test]
fn rejects_unknown_switch_props() {
    let errors = validate_source(
        "switch-prop.ts",
        "ui.mount(ui.switch(bind(() => true), { color: 'red' }));",
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "MTS002");
}
