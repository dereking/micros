use micro_compiler::compile_source;
use micro_core::{Event, MicroUiTree, RenderError, RenderPatch, RenderPort, Runtime};
use micro_ir::{FunctionId, HostCallKind, HostRequest, StateId, UiKind};
use micro_vm::{HostAccess, Value, VmError};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Default)]
struct SimHost {
    kv: BTreeMap<String, Value>,
    config: BTreeMap<String, Value>,
    pending: Vec<(FunctionId, Value)>,
    /// Response used by every HTTP call; `None` keeps the old `HTTP 200\nOK`.
    http_response: Option<String>,
    /// KV writes made by the App, so tests can prove nothing was cleared.
    kv_writes: Rc<RefCell<Vec<(String, Value)>>>,
    /// Last `os.setTitle` payload, so tests can assert the app-bar label.
    title_seen: Rc<RefCell<Option<String>>>,
}

impl HostAccess for SimHost {
    fn call(&mut self, request: &HostRequest, args: &[Value]) -> Result<Option<Value>, VmError> {
        let key = match args.first() {
            Some(Value::String(value)) => value.clone(),
            _ => String::new(),
        };
        Ok(match request.kind {
            HostCallKind::ClockNow => Some(Value::Number(1_000_000.0)),
            HostCallKind::KvGetNumber => Some(read_number(&self.kv, &key)),
            HostCallKind::KvGetText => Some(read_text(&self.kv, &key)),
            HostCallKind::KvSetNumber | HostCallKind::KvSetText => {
                if let Some(value) = args.get(1) {
                    self.kv_writes
                        .borrow_mut()
                        .push((key.clone(), value.clone()));
                    self.kv.insert(key, value.clone());
                }
                None
            }
            HostCallKind::KvRemove => {
                self.kv.remove(&key);
                None
            }
            HostCallKind::ConfigGetNumber => Some(read_number(&self.config, &key)),
            HostCallKind::ConfigGetText => Some(read_text(&self.config, &key)),
            HostCallKind::ConfigSetNumber | HostCallKind::ConfigSetText => {
                if let Some(value) = args.get(1) {
                    self.config.insert(key, value.clone());
                }
                None
            }
            HostCallKind::ConfigRemove => {
                self.config.remove(&key);
                None
            }
            HostCallKind::JsonGetText => Some(Value::String(micro_json::get_text(
                string_arg(args, 0),
                string_arg(args, 1),
            ))),
            HostCallKind::JsonGetNumber => Some(Value::Number(micro_json::get_number(
                string_arg(args, 0),
                string_arg(args, 1),
            ))),
            HostCallKind::JsonLength => Some(Value::Number(micro_json::length(
                string_arg(args, 0),
                string_arg(args, 1),
            ))),
            HostCallKind::AudioSpeak => None,
            HostCallKind::OsLaunchIndex | HostCallKind::OsGoBack | HostCallKind::OsDelay => None,
            HostCallKind::OsSetTitle => {
                *self.title_seen.borrow_mut() = Some(string_arg(args, 0).to_string());
                None
            }
            HostCallKind::NetHttpGet
            | HostCallKind::NetHttpRequest
            | HostCallKind::NetHttpRequestWithHeaders
            | HostCallKind::NetScanWifi => {
                if let Some(callback) = request.callback {
                    let response = self
                        .http_response
                        .clone()
                        .unwrap_or_else(|| "HTTP 200\nOK".into());
                    self.pending.push((callback, Value::String(response)));
                }
                None
            }
            _ => None,
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
    }
}

fn string_arg(args: &[Value], index: usize) -> &str {
    args.get(index)
        .and_then(|value| match value {
            Value::String(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("")
}

fn read_number(store: &BTreeMap<String, Value>, key: &str) -> Value {
    match store.get(key) {
        Some(Value::Number(value)) => Value::Number(*value),
        Some(Value::String(value)) => Value::Number(value.parse::<f64>().unwrap_or(0.0)),
        _ => Value::Number(0.0),
    }
}

fn read_text(store: &BTreeMap<String, Value>, key: &str) -> Value {
    match store.get(key) {
        Some(Value::String(value)) => Value::String(value.clone()),
        Some(Value::Number(value)) => Value::String(value.to_string()),
        _ => Value::String(String::new()),
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

fn button(tree: &MicroUiTree, label: &str) -> FunctionId {
    tree.nodes
        .iter()
        .find(|node| node.kind == UiKind::Button && node.text == label)
        .unwrap_or_else(|| panic!("button {label}"))
        .on_click
        .unwrap_or_else(|| panic!("button {label} has no handler"))
}

fn texts(patches: &[RenderPatch]) -> Vec<&str> {
    patches
        .iter()
        .filter_map(|patch| match patch {
            RenderPatch::SetText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn compiles_and_runs_a_momo_session() {
    let source = include_str!("../../../apps/momo/app.ts");
    let image = compile_source("apps/momo/app.ts", source).unwrap();
    assert_eq!(image.metadata.id, "momo");
    assert_eq!(image.states.len(), 33);

    let title_seen = Rc::new(RefCell::new(None));
    let mut runtime = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost {
            title_seen: title_seen.clone(),
            ..SimHost::default()
        }),
    )
    .unwrap();

    /* Boot names the active profile in the OS bar, not just "momo". */
    assert_eq!(
        title_seen.borrow().as_deref(),
        Some("momo · 默认用户"),
        "the launch binding should set the app-bar title"
    );

    let (start, know, blank) = {
        let tree = &runtime.renderer().created[0];
        let blank = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Surface && node.on_click.is_some())
            .and_then(|node| node.on_click)
            .expect("tappable blank surface");
        (button(tree, "开始今日学习"), button(tree, "认识"), blank)
    };
    /* The profile card starts read-only: its two-input form group is hidden
     * and only becomes visible through 编辑. */
    let edit_group = {
        let tree = &runtime.renderer().created[0];
        tree.nodes
            .iter()
            .find(|node| {
                node.kind == UiKind::Column
                    && node.children.iter().all(|child| {
                        tree.nodes
                            .iter()
                            .any(|node| node.id == *child && node.kind == UiKind::Input)
                    })
                    && node.children.len() == 2
            })
            .map(|node| node.id)
            .expect("profile edit form group")
    };
    assert!(
        runtime
            .renderer()
            .patches
            .contains(&RenderPatch::SetVisible {
                node: edit_group,
                visible: false
            }),
        "the profile form must start hidden until 编辑"
    );
    runtime.renderer_mut().patches.clear();
    let edit = button(&runtime.renderer().created[0], "编辑");
    runtime.enqueue(Event::Activate(edit));
    runtime.tick().unwrap();
    assert!(
        runtime
            .renderer()
            .patches
            .contains(&RenderPatch::SetVisible {
                node: edit_group,
                visible: true
            }),
        "编辑 should reveal the profile form"
    );
    let save_node = runtime.renderer().created[0]
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button && node.text == "保存")
        .expect("保存 button")
        .id;
    assert!(
        runtime
            .renderer()
            .patches
            .contains(&RenderPatch::SetVisible {
                node: save_node,
                visible: true
            }),
        "编辑 should reveal 保存"
    );
    assert!(
        runtime
            .renderer()
            .patches
            .iter()
            .any(|patch| matches!(patch, RenderPatch::SetVisible { visible: false, .. })),
        "study-only controls should start hidden"
    );
    runtime.enqueue(Event::Activate(start));
    runtime.tick().unwrap();
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| *text == "第 1 步 · 听音辨义"),
        "start should enter the listen phase"
    );

    runtime.enqueue(Event::Activate(know));
    runtime.tick().unwrap();
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| text.contains("新词 1 / 5")),
        "grading a new word should count it"
    );
    assert_eq!(runtime.state(StateId(19)), Some(&Value::Number(1.0)));
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| *text == "释义 · 学习反馈"),
        "known should reveal the definition before advancing"
    );
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| text.contains("v. 放弃")),
        "the definition should be visible for the rated word"
    );

    runtime.enqueue(Event::Activate(blank));
    runtime.tick().unwrap();
    assert_eq!(runtime.state(StateId(19)), Some(&Value::Number(0.0)));
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| text.contains("第 1 步") || text.contains("第 2 步")),
        "blank tap should reveal the next word"
    );
}

#[test]
fn automatically_loads_the_next_pool_batch_when_active_batch_ends() {
    let source = include_str!("../../../apps/momo/app.ts");
    let image = compile_source("apps/momo/app.ts", source).unwrap();
    let mut kv = BTreeMap::new();
    for slot in 0..10 {
        let word = if slot == 0 {
            "abandon".to_string()
        } else {
            format!("filler{slot}")
        };
        kv.insert(format!("w{slot}"), Value::String(word));
        if slot > 0 {
            // Existing words that are not due: the batch has no other work,
            // so grading w0 must pull the next pool batch instead of stopping.
            kv.insert(format!("s{slot}"), Value::Number(3.0));
            kv.insert(format!("l{slot}"), Value::Number(2_000_000.0));
        }
    }
    kv.insert("v0".into(), Value::String("101".into()));
    kv.insert("d0".into(), Value::String("v. 放弃".into()));
    kv.insert("e0".into(), Value::String("They abandon the plan.".into()));
    kv.insert("rCount".into(), Value::Number(10.0));
    kv.insert("pool".into(), Value::String(r#"["absorb"]"#.into()));
    kv.insert("poolCount".into(), Value::Number(1.0));
    kv.insert("poolNext".into(), Value::Number(0.0));
    let host = SimHost {
        kv,
        ..SimHost::default()
    };
    let mut runtime =
        Runtime::new_with_host(image, RecordingRenderer::default(), 10_000, Box::new(host))
            .unwrap();

    let (start, know, blank, unknown) = {
        let tree = &runtime.renderer().created[0];
        let blank = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Surface && node.on_click.is_some())
            .and_then(|node| node.on_click)
            .expect("tappable blank surface");
        (
            button(tree, "开始今日学习"),
            button(tree, "认识"),
            blank,
            button(tree, "不认识"),
        )
    };

    runtime.enqueue(Event::Activate(start));
    runtime.tick().unwrap();
    runtime.enqueue(Event::Activate(know));
    runtime.tick().unwrap();

    assert_eq!(runtime.state(StateId(19)), Some(&Value::Number(1.0)));
    assert_eq!(
        runtime.state(StateId(1)),
        Some(&Value::Number(0.0)),
        "the next pool batch starts at listen"
    );
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| *text == "abandon"),
        "feedback after the batch boundary must keep the graded word"
    );
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| text.contains("v. 放弃")),
        "feedback after the batch boundary must keep the graded definition"
    );

    runtime.enqueue(Event::Activate(blank));
    runtime.tick().unwrap();
    runtime.enqueue(Event::Activate(unknown));
    runtime.tick().unwrap();
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| *text == "absorb"),
        "blank tap should reveal the first word of the next pool batch"
    );
}

#[test]
fn empty_today_items_keeps_the_previous_queue_and_explains_the_reason() {
    let source = include_str!("../../../apps/momo/app.ts");
    let image = compile_source("apps/momo/app.ts", source).unwrap();
    let mut kv = BTreeMap::new();
    kv.insert("w0".into(), Value::String("alpha".into()));
    kv.insert("rCount".into(), Value::Number(10.0));
    kv.insert("pool".into(), Value::String(r#"["beta"]"#.into()));
    kv.insert("poolCount".into(), Value::Number(1.0));
    kv.insert("poolNext".into(), Value::Number(0.0));
    let mut config = BTreeMap::new();
    config.insert("maimemoToken".into(), Value::String("token".into()));
    let kv_writes = Rc::new(RefCell::new(Vec::new()));
    let host = SimHost {
        kv,
        config,
        http_response: Some(
            "HTTP 200\n{\"errors\":[],\"data\":{\"today_items\":[]},\"success\":true}".into(),
        ),
        kv_writes: kv_writes.clone(),
        ..SimHost::default()
    };
    let mut runtime =
        Runtime::new_with_host(image, RecordingRenderer::default(), 10_000, Box::new(host))
            .unwrap();

    let sync = button(&runtime.renderer().created[0], "同步墨墨");
    runtime.enqueue(Event::Activate(sync));
    for _ in 0..10 {
        runtime.tick().unwrap();
        runtime.enqueue_host_results();
    }

    assert_eq!(
        runtime.state(StateId(9)),
        Some(&Value::String(
            "墨墨今日列表为空：先在墨墨 App 打开今日学习并开启自动同步后重试。".into()
        )),
        "an empty today_items response must explain the upstream cause"
    );
    assert_eq!(
        runtime.state(StateId(10)),
        Some(&Value::Number(0.0)),
        "an empty response reports zero today items"
    );
    for key in ["pool", "poolCount", "poolNext"] {
        assert!(
            !kv_writes.borrow().iter().any(|(written, _)| written == key),
            "an empty sync must not clear {key}"
        );
    }
    assert_eq!(
        runtime.state(StateId(3)),
        Some(&Value::String(String::new())),
        "an empty sync must not start a study session by itself"
    );
}

// State ids 21..31 are the multi-user states appended after the original
// session/sync states: ns, activeUser, userCount, booted, userStatus,
// syncUser, batchUser, syncBusy, userName, nameDraft, tokenDraft.
const NS: StateId = StateId(21);
const ACTIVE_USER: StateId = StateId(22);
const USER_COUNT: StateId = StateId(23);
const USER_NAME: StateId = StateId(29);
const TOKEN_DRAFT: StateId = StateId(31);
const CUR_WORD: StateId = StateId(3);

#[test]
fn active_user_prefix_scopes_study_data_and_new_users_start_empty() {
    let source = include_str!("../../../apps/momo/app.ts");
    let image = compile_source("apps/momo/app.ts", source).unwrap();
    let mut kv = BTreeMap::new();
    kv.insert("u2:w0".into(), Value::String("second".into()));
    kv.insert("u2:rCount".into(), Value::Number(10.0));
    let mut config = BTreeMap::new();
    config.insert("userCount".into(), Value::Number(2.0));
    config.insert("activeUser".into(), Value::Number(2.0));
    config.insert("u2:name".into(), Value::String("小墨".into()));
    config.insert("u2:maimemoToken".into(), Value::String("token2".into()));
    let host = SimHost {
        kv,
        config,
        ..SimHost::default()
    };
    let mut runtime =
        Runtime::new_with_host(image, RecordingRenderer::default(), 10_000, Box::new(host))
            .unwrap();

    assert_eq!(runtime.state(NS), Some(&Value::String("u2:".into())));
    assert_eq!(runtime.state(ACTIVE_USER), Some(&Value::Number(2.0)));
    assert_eq!(runtime.state(USER_COUNT), Some(&Value::Number(2.0)));
    assert_eq!(
        runtime.state(USER_NAME),
        Some(&Value::String("小墨".into()))
    );
    assert_eq!(
        runtime.state(TOKEN_DRAFT),
        Some(&Value::String("token2".into()))
    );

    let (start, add) = {
        let tree = &runtime.renderer().created[0];
        (button(tree, "开始今日学习"), button(tree, "新建用户"))
    };
    runtime.enqueue(Event::Activate(start));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.state(CUR_WORD),
        Some(&Value::String("second".into())),
        "user 2 must study its own namespaced word"
    );

    runtime.enqueue(Event::Activate(add));
    runtime.tick().unwrap();
    assert_eq!(runtime.state(USER_COUNT), Some(&Value::Number(3.0)));
    assert_eq!(runtime.state(ACTIVE_USER), Some(&Value::Number(3.0)));
    assert_eq!(runtime.state(NS), Some(&Value::String("u3:".into())));

    runtime.enqueue(Event::Activate(start));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.state(CUR_WORD),
        Some(&Value::String("abandon".into())),
        "a fresh user seeds its own guide words instead of inheriting user 2's"
    );
}

#[test]
fn restores_the_saved_active_user_on_launch() {
    let source = include_str!("../../../apps/momo/app.ts");
    let image = compile_source("apps/momo/app.ts", source).unwrap();
    let mut kv = BTreeMap::new();
    // Legacy (unprefixed) data belongs to the first user and must not be used.
    kv.insert("w0".into(), Value::String("alpha".into()));
    kv.insert("rCount".into(), Value::Number(10.0));
    kv.insert("u3:w0".into(), Value::String("gamma".into()));
    kv.insert("u3:rCount".into(), Value::Number(10.0));
    let mut config = BTreeMap::new();
    config.insert("userCount".into(), Value::Number(3.0));
    config.insert("activeUser".into(), Value::Number(3.0));
    config.insert("u3:name".into(), Value::String("三号".into()));
    config.insert("u3:maimemoToken".into(), Value::String("token3".into()));
    let host = SimHost {
        kv,
        config,
        ..SimHost::default()
    };
    let mut runtime =
        Runtime::new_with_host(image, RecordingRenderer::default(), 10_000, Box::new(host))
            .unwrap();

    assert_eq!(runtime.state(NS), Some(&Value::String("u3:".into())));
    assert_eq!(runtime.state(ACTIVE_USER), Some(&Value::Number(3.0)));
    assert_eq!(
        runtime.state(USER_NAME),
        Some(&Value::String("三号".into()))
    );
    assert_eq!(
        runtime.state(TOKEN_DRAFT),
        Some(&Value::String("token3".into()))
    );
    let start = button(&runtime.renderer().created[0], "开始今日学习");
    runtime.enqueue(Event::Activate(start));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.state(CUR_WORD),
        Some(&Value::String("gamma".into())),
        "the restored user's own word must win over legacy user-1 data"
    );
}

#[test]
fn default_user_keeps_legacy_unprefixed_keys() {
    let source = include_str!("../../../apps/momo/app.ts");
    let image = compile_source("apps/momo/app.ts", source).unwrap();
    let mut kv = BTreeMap::new();
    // Pre-multi-user installs only have these bare keys.
    kv.insert("w0".into(), Value::String("alpha".into()));
    kv.insert("rCount".into(), Value::Number(10.0));
    let host = SimHost {
        kv,
        ..SimHost::default()
    };
    let mut runtime =
        Runtime::new_with_host(image, RecordingRenderer::default(), 10_000, Box::new(host))
            .unwrap();

    assert_eq!(runtime.state(NS), Some(&Value::String(String::new())));
    assert_eq!(runtime.state(ACTIVE_USER), Some(&Value::Number(1.0)));
    assert_eq!(
        runtime.state(USER_NAME),
        Some(&Value::String("默认用户".into()))
    );
    assert_eq!(
        runtime.state(TOKEN_DRAFT),
        Some(&Value::String(String::new()))
    );
    let start = button(&runtime.renderer().created[0], "开始今日学习");
    runtime.enqueue(Event::Activate(start));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.state(CUR_WORD),
        Some(&Value::String("alpha".into())),
        "the default user must keep reading its old unprefixed keys"
    );
}

#[test]
fn refuses_to_create_a_sixth_user() {
    let source = include_str!("../../../apps/momo/app.ts");
    let image = compile_source("apps/momo/app.ts", source).unwrap();
    let mut config = BTreeMap::new();
    config.insert("userCount".into(), Value::Number(5.0));
    config.insert("activeUser".into(), Value::Number(5.0));
    let host = SimHost {
        config,
        ..SimHost::default()
    };
    let mut runtime =
        Runtime::new_with_host(image, RecordingRenderer::default(), 10_000, Box::new(host))
            .unwrap();

    let add = button(&runtime.renderer().created[0], "新建用户");
    runtime.enqueue(Event::Activate(add));
    runtime.tick().unwrap();
    assert_eq!(runtime.state(USER_COUNT), Some(&Value::Number(5.0)));
    assert_eq!(runtime.state(ACTIVE_USER), Some(&Value::Number(5.0)));
    assert_eq!(runtime.state(NS), Some(&Value::String("u5:".into())));
    assert!(
        texts(&runtime.renderer().patches)
            .iter()
            .any(|text| *text == "最多 5 个用户，请切换到已有用户。"),
        "the add button should explain the cap instead of creating a sixth profile"
    );
}
