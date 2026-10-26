use micro_compiler::compile_source;
use micro_ir::{
    FunctionKind, HostCallKind, Instruction, ScalarType, TextSource, UiKind, ValueSource,
};

#[test]
fn lowers_counter_studio_states_ui_bindings_and_handlers() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("app.ts", source).unwrap();

    assert_eq!(image.states.len(), 15);
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
        2
    );

    let bindings: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Binding(_)))
        .collect();
    assert_eq!(bindings.len(), 25);

    let handlers: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .collect();
    assert_eq!(handlers.len(), 30);

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
    assert!(first_tab.children.iter().any(|child| matches!(
        image.nodes[child.0 as usize].text,
        Some(TextSource::Constant(_))
    )));
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
    assert!(matches!(progress.value, Some(ValueSource::Binding(_))));

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
    let has_eq_then_not = handler.code.windows(2).any(|window| {
        matches!(
            (&window[0], &window[1]),
            (Instruction::Eq, Instruction::Not)
        )
    });
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
fn lowers_config_calls_into_requests() {
    let source = r#"
app({ id: "cfg", name: "cfg", icon: "c" });
const goal = state(0);
ui.mount(ui.column([
  ui.text(bind(() => `token: ${config.getText("maimemoToken")}`)),
  ui.button("save", {
    onClick: () => {
      config.setText("maimemoToken", "abc");
      config.setNumber("goal", 5);
      config.remove("old");
      goal.value = config.getNumber("goal");
    },
  }),
]));
"#;
    let image = compile_source("cfg.ts", source).unwrap();
    let kinds: Vec<_> = image.host_requests.iter().map(|r| r.kind).collect();
    for kind in [
        HostCallKind::ConfigGetText,
        HostCallKind::ConfigSetText,
        HostCallKind::ConfigSetNumber,
        HostCallKind::ConfigRemove,
        HostCallKind::ConfigGetNumber,
    ] {
        assert!(kinds.contains(&kind), "missing {kind:?}");
    }

    let get = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::ConfigGetText)
        .unwrap();
    assert_eq!(get.arg_kinds, [ScalarType::String]);
    assert_eq!(get.result_kind, Some(ScalarType::String));

    let set = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::ConfigSetText)
        .unwrap();
    assert_eq!(set.arg_kinds, [ScalarType::String, ScalarType::String]);
    assert_eq!(set.result_kind, None);
}

#[test]
fn lowers_audio_calls_into_requests() {
    let source = r#"
ui.mount(ui.column([
  ui.button("speak", {
    onClick: () => {
      audio.setRate(0.8);
      audio.speak("abandon");
      audio.stop();
      let handle = audio.playAudio("https://example.com/word.mp3");
      audio.stopAudio(handle);
      audio.stopAllAudio();
      audio.playBackground("https://example.com/bgm.mp3");
      audio.stopBackground();
    },
  }),
]));
"#;
    let image = compile_source("audio.ts", source).unwrap();
    let kinds: Vec<_> = image.host_requests.iter().map(|r| r.kind).collect();
    for kind in [
        HostCallKind::AudioSetRate,
        HostCallKind::AudioSpeak,
        HostCallKind::AudioStop,
        HostCallKind::AudioPlayUrl,
        HostCallKind::AudioStopUrl,
        HostCallKind::AudioStopAllUrls,
        HostCallKind::AudioPlayBackground,
        HostCallKind::AudioStopBackground,
    ] {
        assert!(kinds.contains(&kind), "missing {kind:?}");
    }

    let rate = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioSetRate)
        .unwrap();
    assert_eq!(rate.arg_kinds, [ScalarType::Number]);
    assert_eq!(rate.result_kind, None);

    let speak = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioSpeak)
        .unwrap();
    assert_eq!(speak.arg_kinds, [ScalarType::String]);
    assert_eq!(speak.result_kind, None);

    let stop = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioStop)
        .unwrap();
    assert!(stop.arg_kinds.is_empty());

    let play = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioPlayUrl)
        .unwrap();
    assert_eq!(play.arg_kinds, [ScalarType::String]);
    assert_eq!(play.result_kind, Some(ScalarType::Number));

    let stop_one = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioStopUrl)
        .unwrap();
    assert_eq!(stop_one.arg_kinds, [ScalarType::Number]);
    assert_eq!(stop_one.result_kind, None);

    let stop_all = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioStopAllUrls)
        .unwrap();
    assert!(stop_all.arg_kinds.is_empty());

    let background = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioPlayBackground)
        .unwrap();
    assert_eq!(background.arg_kinds, [ScalarType::String]);
    assert_eq!(background.result_kind, None);

    let stop_background = image
        .host_requests
        .iter()
        .find(|r| r.kind == HostCallKind::AudioStopBackground)
        .unwrap();
    assert!(stop_background.arg_kinds.is_empty());
}

#[test]
fn config_writes_are_rejected_inside_bindings() {
    let source = r#"
ui.mount(ui.column([
  ui.text(bind(() => {
    config.setText("k", "v");
    return "x";
  })),
]));
"#;
    let errors = compile_source("cfg.ts", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("not allowed inside a binding")),
        "{errors:?}"
    );
}

#[test]
fn audio_play_is_rejected_inside_bindings() {
    let source = r#"
ui.mount(ui.column([
  ui.text(bind(() => {
    audio.playAudio("https://example.com/a.mp3");
    return "x";
  })),
]));
"#;
    let errors = compile_source("audio_binding.ts", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("not allowed inside a binding")),
        "{errors:?}"
    );
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
        LayoutSpec {
            left: None,
            top: None,
            width: None,
            height: None,
            anchor: AnchorSpec {
                left: Some(0.0),
                top: None,
                right: Some(0.0),
                bottom: Some(0.0),
            },
        }, // align bottom
        LayoutSpec {
            left: None,
            top: None,
            width: None,
            height: None,
            anchor: AnchorSpec {
                left: Some(0.0),
                top: None,
                right: Some(0.0),
                bottom: Some(8.0),
            },
        }, // align bottom + anchor.bottom:8
        LayoutSpec {
            left: Some(16.0),
            top: Some(20.0),
            width: None,
            height: None,
            anchor: AnchorSpec::default(),
        }, // left+top only
        LayoutSpec {
            left: None,
            top: None,
            width: None,
            height: None,
            anchor: AnchorSpec {
                left: Some(0.0),
                top: Some(0.0),
                right: Some(0.0),
                bottom: Some(0.0),
            },
        }, // align client
        LayoutSpec {
            left: Some(0.0),
            top: None,
            width: None,
            height: None,
            anchor: AnchorSpec {
                left: Some(0.0),
                top: None,
                right: Some(0.0),
                bottom: Some(0.0),
            },
        }, // lt.left + anchor l/r/b
    ];
    for spec in expected {
        assert!(layouts.contains(&spec), "missing {spec:?} in {layouts:?}");
    }
    assert_eq!(layouts.len(), expected.len());
}

#[test]
fn lowers_inclusive_comparisons_as_negated_strict_ones() {
    let source = r#"
app({ id: "cmp", name: "Cmp", icon: "C" });
const a = state(1);
ui.mount(ui.column([
  ui.place(ui.button("go", {
    onClick: () => {
      if (a.value <= 2) { a.value = 3; }
      if (a.value >= 4) { a.value = 5; }
    },
  }), { width: 100, height: 40 }),
]));
"#;
    let image = compile_source("app.ts", source).unwrap();
    let handler = image
        .functions
        .iter()
        .find(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .unwrap();
    let strict: Vec<_> = handler
        .code
        .iter()
        .filter(|op| matches!(op, Instruction::Lt | Instruction::Gt | Instruction::Not))
        .collect();
    // `<=` becomes `Gt` + `Not`; `>=` becomes `Lt` + `Not`.
    assert!(
        strict.windows(2).any(|pair| {
            matches!(pair[0], Instruction::Gt) && matches!(pair[1], Instruction::Not)
        })
    );
    assert!(
        strict.windows(2).any(|pair| {
            matches!(pair[0], Instruction::Lt) && matches!(pair[1], Instruction::Not)
        })
    );
}

#[test]
fn lowers_header_requests_and_json_queries() {
    let source = r#"
app({ id: "sync", name: "Sync", icon: "S" });
const status = state("");
ui.mount(ui.column([
  ui.place(ui.button("sync", {
    onClick: () => {
      net.lookupWords("abandon\nabsorb", (words) => {
        status.value = json.getText(words, "entries.0.definition");
      });
      net.httpRequestWithHeaders(
        "POST",
        "https://example.test/items",
        "Authorization: Bearer t",
        "",
        (response) => {
          status.value = json.getText(response, "items.0.name");
          const total = json.length(response, "items");
          status.value = `${status.value} ${total} ${json.getNumber(response, "progress.done")}`;
        },
      );
    },
  }), { width: 100, height: 40 }),
]));
"#;
    let image = compile_source("app.ts", source).unwrap();
    assert!(
        image
            .host_requests
            .iter()
            .any(|request| request.kind == HostCallKind::NetHttpRequestWithHeaders)
    );
    assert!(
        image
            .host_requests
            .iter()
            .any(|request| request.kind == HostCallKind::NetLookupWords)
    );
    assert!(
        image
            .host_requests
            .iter()
            .any(|request| request.kind == HostCallKind::JsonGetText)
    );
    assert!(
        image
            .host_requests
            .iter()
            .any(|request| request.kind == HostCallKind::JsonLength)
    );
}

#[test]
fn template_literal_escapes_are_cooked() {
    let source = r#"
app({ id: "esc", name: "esc", icon: "e" });
ui.mount(ui.column([
  ui.button("x", {
    onClick: () => {
      kv.setText("k", `a\nb`);
    },
  }),
]));
"#;
    let image = compile_source("esc.ts", source).expect("compile");
    assert!(
        image.constants.iter().any(
            |constant| matches!(constant, micro_ir::Constant::String(text) if text.contains('\n'))
        ),
        "template escape should become a real newline: {:?}",
        image.constants
    );
}

#[test]
fn lowers_shared_icons_and_surface_labels() {
    let source = r#"
ui.mount(ui.column([
  ui.place(ui.surface("paper", {
    onClick: () => { os.launchIndex(0); },
    label: "Open Counter",
  }), { height: 80 }),
  ui.place(ui.icon("settings", {
    tone: "muted",
    variant: "badge",
    textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 },
  }), { height: 24 }),
  ui.place(ui.icon(bind(() => os.appIcon(0))), { height: 24 }),
]));
"#;
    let image = compile_source("icons.ts", source).unwrap();
    let surface = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Surface)
        .unwrap();
    assert_eq!(
        image.constants[surface.options[0] as usize],
        micro_ir::Constant::String("paper".into())
    );
    let constant = |text: &TextSource| match text {
        TextSource::Constant(index) => image.constants[*index as usize].clone(),
        TextSource::Binding(_) => panic!("expected a constant text source"),
    };
    assert_eq!(
        surface.text.as_ref().map(constant),
        Some(micro_ir::Constant::String("Open Counter".into())),
        "surface label is interned for an accessible name"
    );

    let icons: Vec<_> = image
        .nodes
        .iter()
        .filter(|node| node.kind == UiKind::Icon)
        .collect();
    assert_eq!(icons.len(), 2);
    assert_eq!(
        icons[0].text.as_ref().map(constant),
        Some(micro_ir::Constant::String("settings".into()))
    );
    assert_eq!(
        icons[0]
            .options
            .iter()
            .map(|option| &image.constants[*option as usize])
            .collect::<Vec<_>>(),
        vec![
            &micro_ir::Constant::String("muted".into()),
            &micro_ir::Constant::String("badge".into())
        ]
    );
    assert_eq!(icons[0].text_style.unwrap().size_px, 24);
    assert!(matches!(icons[1].text, Some(TextSource::Binding(_))));
    assert_eq!(
        icons[1].text_style,
        Some(micro_ir::TextStyle::DEFAULT_ICON),
        "an icon without textStyle uses the default icon size"
    );
    assert_eq!(
        icons[1]
            .options
            .iter()
            .map(|option| &image.constants[*option as usize])
            .collect::<Vec<_>>(),
        vec![
            &micro_ir::Constant::String("ink".into()),
            &micro_ir::Constant::String("plain".into())
        ],
        "tone and variant are always interned so hosts can read them by position"
    );
}

#[test]
fn rejects_unknown_icon_names_and_tones() {
    let unknown = r#"
ui.mount(ui.column([
  ui.place(ui.icon("socket"), { height: 24 }),
]));
"#;
    let errors = compile_source("icons.ts", unknown).unwrap_err();
    assert!(
        errors[0].message.contains("unknown icon `socket`"),
        "{:?}",
        errors[0]
    );

    let bad_tone = r#"
ui.mount(ui.column([
  ui.place(ui.icon("settings", { tone: "loud" }), { height: 24 }),
]));
"#;
    let errors = compile_source("icons.ts", bad_tone).unwrap_err();
    assert!(
        errors[0].message.contains("ui.icon tone"),
        "{:?}",
        errors[0]
    );

    let bad_manifest = r#"
app({ id: "x", name: "X", icon: "socket" });
ui.mount(ui.text("x"));
"#;
    let errors = compile_source("icons.ts", bad_manifest).unwrap_err();
    assert!(
        errors[0].message.contains("app metadata icon"),
        "{:?}",
        errors[0]
    );
}

#[test]
fn lowers_a_surface_tone() {
    let source = r#"
ui.mount(ui.column([
  ui.place(ui.surface("sage"), { height: 24, width: 120 }),
  ui.place(ui.surface(), { height: 1 }),
]));
"#;
    let image = compile_source("surface.ts", source).unwrap();
    let surfaces: Vec<_> = image
        .nodes
        .iter()
        .filter(|node| node.kind == UiKind::Surface)
        .collect();
    assert_eq!(surfaces.len(), 2);
    assert_eq!(
        image.constants[surfaces[0].options[0] as usize],
        micro_ir::Constant::String("sage".into())
    );
    assert!(surfaces[1].options.is_empty());
    assert!(surfaces.iter().all(|node| node.layout.is_some()));
}

#[test]
fn lowers_surface_taps_and_conditional_buttons() {
    let source = r#"
const mode = state(0);
ui.mount(ui.column([
  ui.place(ui.surface("canvas", {
    onClick: () => {
      mode.value = 1;
    },
  }), { height: 100 }),
  ui.place(ui.show(
    ui.button("start", {
      onClick: () => {
        mode.value = 0;
      },
    }),
    bind(() => mode.value === 0),
  ), { height: 40 }),
]));
"#;
    let image = compile_source("tap.ts", source).unwrap();
    let surface = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Surface)
        .expect("surface");
    assert!(surface.on_click.is_some(), "surface tap is a handler");
    let button = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button)
        .expect("button");
    assert!(
        matches!(button.value, Some(ValueSource::Binding(_))),
        "show should bind the button's visibility"
    );
}

#[test]
fn shadowed_locals_get_distinct_slots() {
    let source = r#"
const mode = state(0);
ui.mount(ui.column([
  ui.place(ui.text(bind(() => {
    let x = 1;
    if (mode.value > 0) {
      let x = "shadow";
      return x;
    }
    return `${x}`;
  })), { height: 20 }),
]));
"#;
    let image = compile_source("shadow.ts", source).unwrap();
    let binding = image
        .functions
        .iter()
        .find(|function| matches!(function.kind, FunctionKind::Binding(_)))
        .expect("binding");
    assert_eq!(binding.locals, 2, "each declaration needs its own slot");
}

#[test]
fn lowers_storage_calls_into_requests() {
    let source = r#"
app({ id: "store", name: "Store", icon: "S" });
const size = state(0);
const listing = state("");
ui.mount(ui.column([
  ui.text(bind(() => storage.readText("notes/today.txt"))),
  ui.text(bind(() => {
    if (storage.available()) { return "sd"; }
    return "none";
  })),
  ui.text(bind(() => {
    if (storage.exists("notes")) { return "yes"; }
    return "no";
  })),
  ui.button("save", {
    onClick: () => {
      storage.writeText("notes/today.txt", "hello");
      storage.appendText("notes/today.txt", " world");
      storage.mkdir("notes/archive");
      listing.value = storage.list("");
      size.value = storage.size("notes/today.txt");
      storage.remove("notes/old.txt");
    },
  }),
]));
"#;
    let image = compile_source("store.ts", source).unwrap();
    let request = |kind: HostCallKind| {
        image
            .host_requests
            .iter()
            .find(|request| request.kind == kind)
            .unwrap_or_else(|| panic!("missing {kind:?}"))
    };

    let available = request(HostCallKind::StorageAvailable);
    assert!(available.arg_kinds.is_empty());
    assert_eq!(available.result_kind, Some(ScalarType::Bool));

    let read = request(HostCallKind::StorageReadText);
    assert_eq!(read.arg_kinds, [ScalarType::String]);
    assert_eq!(read.result_kind, Some(ScalarType::String));

    let exists = request(HostCallKind::StorageExists);
    assert_eq!(exists.result_kind, Some(ScalarType::Bool));

    let write = request(HostCallKind::StorageWriteText);
    assert_eq!(write.arg_kinds, [ScalarType::String, ScalarType::String]);
    assert_eq!(write.result_kind, Some(ScalarType::Bool));

    let append = request(HostCallKind::StorageAppendText);
    assert_eq!(append.arg_kinds, [ScalarType::String, ScalarType::String]);
    let list = request(HostCallKind::StorageList);
    assert_eq!(list.result_kind, Some(ScalarType::String));
    let size = request(HostCallKind::StorageSize);
    assert_eq!(size.result_kind, Some(ScalarType::Number));
    let remove = request(HostCallKind::StorageRemove);
    assert_eq!(remove.result_kind, Some(ScalarType::Bool));
    let mkdir = request(HostCallKind::StorageMkdir);
    assert_eq!(mkdir.result_kind, Some(ScalarType::Bool));
}

#[test]
fn storage_mutations_are_rejected_inside_bindings() {
    let sources = [
        r#"ui.mount(ui.text(bind(() => { storage.writeText("a.txt", "x"); return "x"; })));"#,
        r#"ui.mount(ui.text(bind(() => { storage.appendText("a.txt", "x"); return "x"; })));"#,
        r#"ui.mount(ui.text(bind(() => { storage.remove("a.txt"); return "x"; })));"#,
        r#"ui.mount(ui.text(bind(() => { storage.mkdir("dir"); return "x"; })));"#,
    ];
    for source in sources {
        let errors = compile_source("storage_binding.ts", source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("not allowed inside a binding")),
            "{errors:?}"
        );
    }
}

#[test]
fn lowers_button_variant_and_input_text_style() {
    let source = r#"
app({ id: "variant", name: "Variant", icon: "v" });

ui.mount(ui.column([
  ui.button("Go", { variant: "selected", onClick: () => {} }),
  ui.input("", {
    placeholder: "Name",
    textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
    onChange: (s) => {},
  }),
]));
"#;
    let image = compile_source("app.ts", source).unwrap();
    let button = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button)
        .unwrap();
    let variant = button.options.first().expect("button variant option");
    assert_eq!(
        image.constants[*variant as usize],
        micro_ir::Constant::String("selected".into())
    );
    let input = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Input)
        .unwrap();
    let style = input.text_style.expect("input text style");
    assert_eq!(style.size_px, 14);
    assert_eq!(style.line_height_px, 18);
}

#[test]
fn rejects_unknown_button_variant() {
    let source = r#"
app({ id: "variant", name: "Variant", icon: "v" });
ui.mount(ui.button("Go", { variant: "ghost", onClick: () => {} }));
"#;
    let errors = compile_source("app.ts", source).unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains("variant")),
        "{errors:?}"
    );
}
