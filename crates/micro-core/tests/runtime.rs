use std::cell::RefCell;
use std::rc::Rc;

use micro_core::{
    Event, EventQueue, MicroUiTree, RenderError, RenderPatch, RenderPort, Runtime, RuntimeError,
};
use micro_ir::{
    AppImage, AppMetadata, BindingId, Capability, Constant, FontWeight, Function, FunctionId,
    FunctionKind, HandlerId, HostCallKind, HostRequest, Instruction, NodeId, ScalarType, StateDecl,
    StateId, TextSource, TextStyle, UiKind, UiNodeSpec, ValueSource,
};
use micro_vm::{HostAccess, Value, VmError};

#[derive(Default)]
struct RecordingRenderer {
    created: Vec<MicroUiTree>,
    patches: Vec<RenderPatch>,
    fail_apply: bool,
}

impl RenderPort for RecordingRenderer {
    fn create_tree(&mut self, tree: &MicroUiTree) -> Result<(), RenderError> {
        self.created.push(tree.clone());
        Ok(())
    }

    fn apply(&mut self, patches: &[RenderPatch]) -> Result<(), RenderError> {
        if self.fail_apply {
            return Err(RenderError("renderer rejected patch".into()));
        }
        self.patches.extend_from_slice(patches);
        Ok(())
    }
}

fn counter_image() -> AppImage {
    AppImage {
        constants: vec![
            Constant::Number(0.0),
            Constant::String("Count: ".into()),
            Constant::Number(1.0),
            Constant::Number(2.0),
            Constant::String("Add".into()),
        ],
        states: vec![StateDecl {
            ty: ScalarType::Number,
            initial: 0,
        }],
        functions: vec![
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 2,
                code: vec![
                    Instruction::Const(1),
                    Instruction::LoadState(StateId(0)),
                    Instruction::ToString,
                    Instruction::Concat,
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 2,
                code: vec![
                    Instruction::LoadState(StateId(0)),
                    Instruction::Const(2),
                    Instruction::Add,
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(1)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(2),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Const(3),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(2)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(0),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(3)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(2),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Jump(2),
                ],
            },
        ],
        nodes: vec![
            UiNodeSpec {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1), NodeId(2)],
                text: None,
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
            UiNodeSpec {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: Some(TextSource::Binding(FunctionId(0))),
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
            UiNodeSpec {
                id: NodeId(2),
                kind: UiKind::Button,
                children: vec![],
                text: Some(TextSource::Constant(4)),
                value: None,
                on_click: Some(FunctionId(1)),
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
        ],
        host_requests: vec![],
        metadata: AppMetadata {
            id: "counter".into(),
            name: "Counter".into(),
            icon: "C".into(),
            capabilities: None,
        },
        root: NodeId(0),
    }
}

#[test]
fn event_queue_is_fifo() {
    let mut queue = EventQueue::default();
    queue.push(Event::Activate(FunctionId(2)));
    queue.push(Event::Activate(FunctionId(1)));
    assert_eq!(queue.pop(), Some(Event::Activate(FunctionId(2))));
    assert_eq!(queue.pop(), Some(Event::Activate(FunctionId(1))));
}

#[test]
fn creates_once_and_patches_counter_text() {
    let mut runtime = Runtime::new(counter_image(), RecordingRenderer::default(), 10_000).unwrap();
    assert_eq!(runtime.renderer().created.len(), 1);
    assert_eq!(runtime.renderer().created[0].nodes[1].text, "Count: 0");
    assert_eq!(
        runtime.renderer().created[0].nodes[1].text_style,
        Some(TextStyle::ui_sans(24, FontWeight::Regular, 32).unwrap())
    );
    assert_eq!(
        runtime.renderer().created[0].nodes[2].text_style,
        Some(TextStyle::ui_sans(14, FontWeight::Regular, 18).unwrap())
    );

    runtime.enqueue(Event::Activate(FunctionId(1)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetText {
            node: NodeId(1),
            text: "Count: 1".into()
        }]
    );
    assert_eq!(runtime.renderer().created.len(), 1);
}

#[test]
fn preserves_text_style_while_patching_only_text() {
    let mut image = counter_image();
    let style = TextStyle::ui_sans(24, FontWeight::Regular, 32).unwrap();
    image.nodes[1].text_style = Some(style);
    let mut runtime = Runtime::new(image, RecordingRenderer::default(), 10_000).unwrap();

    assert_eq!(
        runtime.renderer().created[0].nodes[1].text_style,
        Some(style)
    );

    runtime.enqueue(Event::Activate(FunctionId(1)));
    runtime.tick().unwrap();

    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetText {
            node: NodeId(1),
            text: "Count: 1".into(),
        }]
    );
    assert_eq!(
        runtime.renderer().created[0].nodes[1].text_style,
        Some(style)
    );
}

#[test]
fn coalesces_two_writes_and_ignores_no_op_writes() {
    let mut runtime = Runtime::new(counter_image(), RecordingRenderer::default(), 10_000).unwrap();
    runtime.enqueue(Event::Activate(FunctionId(2)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetText {
            node: NodeId(1),
            text: "Count: 2".into()
        }]
    );

    runtime.renderer_mut().patches.clear();
    runtime.enqueue(Event::Activate(FunctionId(2)));
    runtime.tick().unwrap();
    assert!(runtime.renderer().patches.is_empty());
}

#[test]
fn flushes_partial_state_after_budget_exhaustion() {
    let mut runtime = Runtime::new(counter_image(), RecordingRenderer::default(), 4).unwrap();
    runtime.enqueue(Event::Activate(FunctionId(4)));
    assert!(matches!(
        runtime.tick(),
        Err(RuntimeError::Vm(VmError::BudgetExceeded {
            executed: 4,
            ..
        }))
    ));
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetText {
            node: NodeId(1),
            text: "Count: 1".into()
        }]
    );
}

#[test]
fn propagates_renderer_errors_without_panicking() {
    let mut runtime = Runtime::new(counter_image(), RecordingRenderer::default(), 10_000).unwrap();
    runtime.renderer_mut().fail_apply = true;
    runtime.enqueue(Event::Activate(FunctionId(1)));
    assert!(matches!(runtime.tick(), Err(RuntimeError::Render(_))));
}

#[test]
fn state_access_is_type_checked() {
    let runtime = Runtime::new(counter_image(), RecordingRenderer::default(), 10_000).unwrap();
    assert_eq!(runtime.state(StateId(0)), Some(&Value::Number(0.0)));
}

#[test]
fn replaces_binding_dependencies_after_each_evaluation() {
    let image = AppImage {
        constants: vec![
            Constant::Bool(false),
            Constant::Number(1.0),
            Constant::Number(10.0),
            Constant::Bool(true),
            Constant::Number(11.0),
            Constant::Number(2.0),
        ],
        states: vec![
            StateDecl {
                ty: ScalarType::Bool,
                initial: 0,
            },
            StateDecl {
                ty: ScalarType::Number,
                initial: 1,
            },
            StateDecl {
                ty: ScalarType::Number,
                initial: 2,
            },
        ],
        functions: vec![
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::LoadState(StateId(0)),
                    Instruction::JumpIfFalse(4),
                    Instruction::LoadState(StateId(1)),
                    Instruction::Jump(5),
                    Instruction::LoadState(StateId(2)),
                    Instruction::ToString,
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(3),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(1)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(4),
                    Instruction::StoreState(StateId(2)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(2)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(5),
                    Instruction::StoreState(StateId(1)),
                    Instruction::Return,
                ],
            },
        ],
        nodes: vec![UiNodeSpec {
            id: NodeId(0),
            kind: UiKind::Text,
            children: vec![],
            text: Some(TextSource::Binding(FunctionId(0))),
            value: None,
            on_click: None,
            text_style: None,
            range: None,
            options: vec![],
            layout: None,
            visible: None,
        }],
        host_requests: vec![],
        metadata: AppMetadata {
            id: "tick".into(),
            name: "Tick".into(),
            icon: "T".into(),
            capabilities: None,
        },
        root: NodeId(0),
    };
    let mut runtime = Runtime::new(image, RecordingRenderer::default(), 10_000).unwrap();
    assert_eq!(runtime.renderer().created[0].nodes[0].text, "10");

    runtime.enqueue(Event::Activate(FunctionId(1)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches.last().unwrap(),
        &RenderPatch::SetText {
            node: NodeId(0),
            text: "1".into()
        }
    );

    runtime.renderer_mut().patches.clear();
    runtime.enqueue(Event::Activate(FunctionId(2)));
    runtime.tick().unwrap();
    assert!(runtime.renderer().patches.is_empty());

    runtime.enqueue(Event::Activate(FunctionId(3)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetText {
            node: NodeId(0),
            text: "2".into()
        }]
    );
}

fn value_image() -> AppImage {
    AppImage {
        constants: vec![
            Constant::Number(0.0),
            Constant::Bool(false),
            Constant::Number(0.7),
            Constant::Bool(true),
            Constant::String("oops".into()),
        ],
        states: vec![
            StateDecl {
                ty: ScalarType::Number,
                initial: 0,
            },
            StateDecl {
                ty: ScalarType::Bool,
                initial: 1,
            },
        ],
        functions: vec![
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![Instruction::LoadState(StateId(0)), Instruction::Return],
            },
            Function {
                kind: FunctionKind::Binding(BindingId(1)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![Instruction::LoadState(StateId(1)), Instruction::Return],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(2),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(1)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(3),
                    Instruction::StoreState(StateId(1)),
                    Instruction::Return,
                ],
            },
        ],
        nodes: vec![
            UiNodeSpec {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1), NodeId(2)],
                text: None,
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
            UiNodeSpec {
                id: NodeId(1),
                kind: UiKind::Progress,
                children: vec![],
                text: None,
                value: Some(ValueSource::Binding(FunctionId(0))),
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
            UiNodeSpec {
                id: NodeId(2),
                kind: UiKind::Switch,
                children: vec![],
                text: None,
                value: Some(ValueSource::Binding(FunctionId(1))),
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
        ],
        host_requests: vec![],
        metadata: AppMetadata {
            id: "values".into(),
            name: "Values".into(),
            icon: "V".into(),
            capabilities: None,
        },
        root: NodeId(0),
    }
}

#[test]
fn materializes_and_flushes_progress_and_switch_values() {
    let mut runtime = Runtime::new(value_image(), RecordingRenderer::default(), 10_000).unwrap();
    assert_eq!(
        runtime.renderer().created[0].nodes[1].value,
        Some(Value::Number(0.0))
    );
    assert_eq!(
        runtime.renderer().created[0].nodes[2].value,
        Some(Value::Bool(false))
    );

    runtime.enqueue(Event::Activate(FunctionId(2)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetProgress {
            node: NodeId(1),
            fraction: 0.7
        }]
    );

    runtime.renderer_mut().patches.clear();
    runtime.enqueue(Event::Activate(FunctionId(3)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetChecked {
            node: NodeId(2),
            checked: true
        }]
    );
}

#[test]
fn progress_binding_returning_string_errors() {
    let mut image = value_image();
    image.functions[0].code = vec![Instruction::Const(4), Instruction::Return];
    let result = Runtime::new(image, RecordingRenderer::default(), 10_000);
    assert!(matches!(
        result,
        Err(RuntimeError::ProgressIsNotNumber(NodeId(1)))
    ));
}

#[test]
fn switch_binding_returning_number_errors() {
    let mut image = value_image();
    image.functions[1].code = vec![Instruction::Const(2), Instruction::Return];
    let result = Runtime::new(image, RecordingRenderer::default(), 10_000);
    assert!(matches!(
        result,
        Err(RuntimeError::SwitchIsNotBoolean(NodeId(2)))
    ));
}

#[derive(Default)]
struct SimHost {
    pending: Vec<(FunctionId, Value)>,
}

struct DisposeHost(Rc<RefCell<bool>>);

impl HostAccess for DisposeHost {
    fn call(&mut self, _request: &HostRequest, _args: &[Value]) -> Result<Option<Value>, VmError> {
        Err(VmError::Host("unexpected host call".into()))
    }

    fn dispose(&mut self) {
        *self.0.borrow_mut() = true;
    }
}

impl HostAccess for SimHost {
    fn call(&mut self, request: &HostRequest, _args: &[Value]) -> Result<Option<Value>, VmError> {
        match request.kind {
            HostCallKind::DeviceChip => Ok(Some(Value::String("ESP32-S3".into()))),
            HostCallKind::NetScanWifi => {
                self.pending
                    .push((request.callback.unwrap(), Value::String("ap1\nap2".into())));
                Ok(None)
            }
            _ => Err(VmError::Host("unexpected host call".into())),
        }
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
    }
}

/// Image with one string state, a binding reading it, a 1-arg handler that
/// stores its argument into the state, and a 0-arg handler that defers an
/// async `net.scanWifi`.
fn host_image() -> AppImage {
    AppImage {
        constants: vec![Constant::String("off".into())],
        states: vec![StateDecl {
            ty: ScalarType::String,
            initial: 0,
        }],
        functions: vec![
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 2,
                code: vec![Instruction::LoadState(StateId(0)), Instruction::Return],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(0)),
                arg_count: 1,
                locals: 0,
                max_stack: 2,
                code: vec![
                    Instruction::LoadArg,
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(1)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![Instruction::HostCall(0), Instruction::Return],
            },
        ],
        nodes: vec![UiNodeSpec {
            id: NodeId(0),
            kind: UiKind::Text,
            children: vec![],
            text: Some(TextSource::Binding(FunctionId(0))),
            value: None,
            on_click: None,
            text_style: None,
            range: None,
            options: vec![],
            layout: None,
            visible: None,
        }],
        host_requests: vec![HostRequest::async_request(
            HostCallKind::NetScanWifi,
            vec![],
            FunctionId(1),
        )],
        metadata: AppMetadata {
            id: "host".into(),
            name: "Host".into(),
            icon: "H".into(),
            capabilities: Some(vec![Capability::Device, Capability::Net]),
        },
        root: NodeId(0),
    }
}

#[test]
fn dispose_host_releases_app_owned_resources() {
    let disposed = Rc::new(RefCell::new(false));
    let host = DisposeHost(disposed.clone());
    let mut runtime = Runtime::new_with_host(
        host_image(),
        RecordingRenderer::default(),
        10_000,
        Box::new(host),
    )
    .expect("runtime");
    runtime.dispose_host();
    assert!(*disposed.borrow());
}

#[test]
fn host_read_in_binding_materializes_the_host_value() {
    let mut image = host_image();
    image.host_requests[0] =
        HostRequest::sync(HostCallKind::DeviceChip, vec![], Some(ScalarType::String));
    image.functions[0].code = vec![Instruction::HostCall(0), Instruction::Return];
    /* The 0-arg handler no longer matches the now-sync request; consume its
     * result so the image stays valid. */
    image.functions[2].code = vec![
        Instruction::HostCall(0),
        Instruction::Pop,
        Instruction::Return,
    ];
    let runtime = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost::default()),
    )
    .unwrap();
    assert_eq!(runtime.renderer().created[0].nodes[0].text, "ESP32-S3");
}

#[test]
fn null_host_rejects_host_reads_during_creation() {
    let mut image = host_image();
    image.host_requests[0] =
        HostRequest::sync(HostCallKind::DeviceChip, vec![], Some(ScalarType::String));
    image.functions[0].code = vec![Instruction::HostCall(0), Instruction::Return];
    image.functions[2].code = vec![
        Instruction::HostCall(0),
        Instruction::Pop,
        Instruction::Return,
    ];
    let result = Runtime::new(image, RecordingRenderer::default(), 10_000);
    assert!(matches!(result, Err(RuntimeError::Vm(VmError::Host(_)))));
}

#[test]
fn capability_gate_denies_ungranted_host_call() {
    let mut image = host_image();
    image.host_requests[0] =
        HostRequest::sync(HostCallKind::DeviceChip, vec![], Some(ScalarType::String));
    image.functions[0].code = vec![Instruction::HostCall(0), Instruction::Return];
    image.functions[2].code = vec![
        Instruction::HostCall(0),
        Instruction::Pop,
        Instruction::Return,
    ];
    image.metadata.capabilities = Some(vec![Capability::Net, Capability::Kv]);
    let result = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost::default()),
    );
    assert!(
        matches!(result, Err(RuntimeError::Vm(VmError::Host(message))) if message.contains("capability `device`"))
    );
}

#[test]
fn capability_gate_allows_a_granted_host_call() {
    let mut image = host_image();
    image.host_requests[0] =
        HostRequest::sync(HostCallKind::DeviceChip, vec![], Some(ScalarType::String));
    image.functions[0].code = vec![Instruction::HostCall(0), Instruction::Return];
    image.functions[2].code = vec![
        Instruction::HostCall(0),
        Instruction::Pop,
        Instruction::Return,
    ];
    image.metadata.capabilities = Some(vec![Capability::Device]);
    let runtime = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost::default()),
    )
    .unwrap();
    assert_eq!(runtime.renderer().created[0].nodes[0].text, "ESP32-S3");
}

#[test]
fn capability_gate_denies_every_call_for_an_empty_grant() {
    let mut image = host_image();
    image.host_requests[0] =
        HostRequest::sync(HostCallKind::DeviceChip, vec![], Some(ScalarType::String));
    image.functions[0].code = vec![Instruction::HostCall(0), Instruction::Return];
    image.functions[2].code = vec![
        Instruction::HostCall(0),
        Instruction::Pop,
        Instruction::Return,
    ];
    image.metadata.capabilities = Some(vec![]);
    let result = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost::default()),
    );
    assert!(
        matches!(result, Err(RuntimeError::Vm(VmError::Host(message))) if message.contains("capability `device`"))
    );
}

#[test]
fn capability_gate_denies_host_calls_when_the_manifest_omits_capabilities() {
    let mut image = host_image();
    image.host_requests[0] =
        HostRequest::sync(HostCallKind::DeviceChip, vec![], Some(ScalarType::String));
    image.functions[0].code = vec![Instruction::HostCall(0), Instruction::Return];
    image.functions[2].code = vec![
        Instruction::HostCall(0),
        Instruction::Pop,
        Instruction::Return,
    ];
    image.metadata.capabilities = None;
    let result = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost::default()),
    );
    assert!(
        matches!(result, Err(RuntimeError::Vm(VmError::Host(message))) if message.contains("capability `device`"))
    );
}

#[test]
fn async_host_result_round_trips_through_drain_and_tick() {
    let mut runtime = Runtime::new_with_host(
        host_image(),
        RecordingRenderer::default(),
        10_000,
        Box::new(SimHost::default()),
    )
    .unwrap();

    runtime.enqueue(Event::Activate(FunctionId(2)));
    runtime.tick().unwrap();
    runtime.enqueue_host_results();
    runtime.tick().unwrap();

    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetText {
            node: NodeId(0),
            text: "ap1\nap2".into()
        }]
    );
}

/// Host whose `net.wifiState` read reflects an externally-mutated radio state.
struct WifiHost {
    state: Rc<RefCell<String>>,
}

impl HostAccess for WifiHost {
    fn call(&mut self, request: &HostRequest, _args: &[Value]) -> Result<Option<Value>, VmError> {
        match request.kind {
            HostCallKind::NetWifiState => Ok(Some(Value::String(self.state.borrow().clone()))),
            _ => Err(VmError::Host("unexpected host call".into())),
        }
    }
}

#[test]
fn reconcile_re_evaluates_bindings_after_host_value_changes() {
    let mut image = host_image();
    image.host_requests[0] =
        HostRequest::sync(HostCallKind::NetWifiState, vec![], Some(ScalarType::String));
    image.functions[0].code = vec![Instruction::HostCall(0), Instruction::Return];
    /* The 0-arg handler no longer matches the now-sync request; consume its
     * result so the image stays valid. */
    image.functions[2].code = vec![
        Instruction::HostCall(0),
        Instruction::Pop,
        Instruction::Return,
    ];

    let state = Rc::new(RefCell::new("connecting".to_owned()));
    let mut runtime = Runtime::new_with_host(
        image,
        RecordingRenderer::default(),
        10_000,
        Box::new(WifiHost {
            state: state.clone(),
        }),
    )
    .unwrap();
    assert_eq!(runtime.renderer().created[0].nodes[0].text, "connecting");

    /* The radio transitions outside the VM; a plain tick (no events) never
     * re-reads the host value, so the rendered text stays stale. */
    *state.borrow_mut() = "connected".to_owned();
    runtime.tick().unwrap();
    assert!(runtime.renderer().patches.is_empty());

    /* reconcile re-runs every binding and patches the text to the new value. */
    runtime.reconcile().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetText {
            node: NodeId(0),
            text: "connected".into()
        }]
    );
}

fn input_image() -> AppImage {
    AppImage {
        constants: vec![
            Constant::String(String::new()),
            Constant::String("switch".into()),
            Constant::Number(0.0),
            Constant::Number(1.0),
        ],
        states: vec![
            StateDecl {
                ty: ScalarType::String,
                initial: 0,
            },
            StateDecl {
                ty: ScalarType::Number,
                initial: 2,
            },
        ],
        functions: vec![
            // Input value binding: read `gate` (so a profile switch can dirty
            // it) and then return the bound text state.
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 2,
                code: vec![
                    Instruction::LoadState(StateId(1)),
                    Instruction::Pop,
                    Instruction::LoadState(StateId(0)),
                    Instruction::Return,
                ],
            },
            // Switch handler: flip `gate` but keep the text at its boot value.
            Function {
                kind: FunctionKind::Handler(HandlerId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(3),
                    Instruction::StoreState(StateId(1)),
                    Instruction::Const(0),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            // onChange handler: the App stores the text elsewhere; the input
            // binding itself is not re-evaluated by this handler.
            Function {
                kind: FunctionKind::Handler(HandlerId(1)),
                arg_count: 1,
                locals: 0,
                max_stack: 1,
                code: vec![Instruction::Return],
            },
        ],
        nodes: vec![
            UiNodeSpec {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1), NodeId(2)],
                text: None,
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
            UiNodeSpec {
                id: NodeId(1),
                kind: UiKind::Input,
                children: vec![],
                text: Some(TextSource::Constant(0)),
                value: Some(ValueSource::Binding(FunctionId(0))),
                on_click: Some(FunctionId(2)),
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
            UiNodeSpec {
                id: NodeId(2),
                kind: UiKind::Button,
                children: vec![],
                text: Some(TextSource::Constant(1)),
                value: None,
                on_click: Some(FunctionId(1)),
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
        ],
        host_requests: vec![],
        metadata: AppMetadata {
            id: "input-cache".into(),
            name: "Input".into(),
            icon: "I".into(),
            capabilities: None,
        },
        root: NodeId(0),
    }
}

#[test]
fn typed_input_text_is_restored_when_the_binding_is_re_evaluated() {
    let mut runtime = Runtime::new(input_image(), RecordingRenderer::default(), 10_000).unwrap();
    runtime.enqueue(Event::InputChanged(FunctionId(2), "typed".into()));
    runtime.tick().unwrap();
    runtime.renderer_mut().patches.clear();

    // The profile switch re-evaluates the input binding to the same "" it had
    // at boot. Without remembering the typed text this compares equal and no
    // patch is emitted, leaving "typed" on screen for the next user.
    runtime.enqueue(Event::Activate(FunctionId(1)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetInputText {
            node: NodeId(1),
            text: String::new(),
        }]
    );
}

/// One icon node whose name comes from a string state; the handler replaces
/// that state with the second constant. Icon name changes must emit
/// `RenderPatch::SetIcon` (not `SetText`) so hosts re-resolve the SVG/glyph.
#[test]
fn icon_name_binding_patches_set_icon() {
    let image = AppImage {
        constants: vec![
            Constant::String("settings".into()),
            Constant::String("trash".into()),
        ],
        states: vec![StateDecl {
            ty: ScalarType::String,
            initial: 0,
        }],
        functions: vec![
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![Instruction::LoadState(StateId(0)), Instruction::Return],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(1),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
        ],
        nodes: vec![UiNodeSpec {
            id: NodeId(0),
            kind: UiKind::Icon,
            children: vec![],
            text: Some(TextSource::Binding(FunctionId(0))),
            value: None,
            on_click: None,
            text_style: None,
            range: None,
            options: vec![],
            layout: None,
            visible: None,
        }],
        host_requests: vec![],
        metadata: AppMetadata {
            id: "icon".into(),
            name: "Icon".into(),
            icon: "edit".into(),
            capabilities: None,
        },
        root: NodeId(0),
    };
    let mut runtime = Runtime::new(image, RecordingRenderer::default(), 10_000).unwrap();
    assert_eq!(runtime.renderer().created[0].nodes[0].text, "settings");

    runtime.enqueue(Event::Activate(FunctionId(1)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetIcon {
            node: NodeId(0),
            name: "trash".to_owned(),
        }]
    );
}

#[test]
fn visibility_bindings_patch_any_widget_and_start_hidden() {
    let image = AppImage {
        constants: vec![
            Constant::Number(0.0),
            Constant::Bool(false),
            Constant::String("hidden form".into()),
            Constant::Bool(true),
        ],
        states: vec![StateDecl {
            ty: ScalarType::Bool,
            initial: 1,
        }],
        functions: vec![
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![Instruction::LoadState(StateId(0)), Instruction::Return],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(0)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(3),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
            Function {
                kind: FunctionKind::Handler(HandlerId(1)),
                arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![
                    Instruction::Const(1),
                    Instruction::StoreState(StateId(0)),
                    Instruction::Return,
                ],
            },
        ],
        nodes: vec![
            UiNodeSpec {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1)],
                text: None,
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: Some(ValueSource::Binding(FunctionId(0))),
            },
            UiNodeSpec {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: Some(TextSource::Constant(2)),
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
                layout: None,
                visible: None,
            },
        ],
        host_requests: vec![],
        metadata: AppMetadata {
            id: "show".into(),
            name: "Show".into(),
            icon: "edit".into(),
            capabilities: None,
        },
        root: NodeId(0),
    };
    let mut runtime = Runtime::new(image, RecordingRenderer::default(), 10_000).unwrap();
    /* The form group starts hidden, before the first tick, so it never flashes. */
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetVisible {
            node: NodeId(0),
            visible: false
        }]
    );

    runtime.renderer_mut().patches.clear();
    runtime.enqueue(Event::Activate(FunctionId(1)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetVisible {
            node: NodeId(0),
            visible: true
        }]
    );

    runtime.renderer_mut().patches.clear();
    runtime.enqueue(Event::Activate(FunctionId(2)));
    runtime.tick().unwrap();
    assert_eq!(
        runtime.renderer().patches,
        [RenderPatch::SetVisible {
            node: NodeId(0),
            visible: false
        }]
    );
}
