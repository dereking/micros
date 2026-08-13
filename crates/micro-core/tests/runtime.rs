use micro_core::{
    Event, EventQueue, MicroUiTree, RenderError, RenderPatch, RenderPort, Runtime, RuntimeError,
};
use micro_ir::{
    AppImage, BindingId, Constant, Function, FunctionId, FunctionKind, HandlerId, Instruction,
    NodeId, ScalarType, StateDecl, StateId, TextSource, UiKind, UiNodeSpec,
};
use micro_vm::{Value, VmError};

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
                on_click: None,
            },
            UiNodeSpec {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: Some(TextSource::Binding(FunctionId(0))),
                on_click: None,
            },
            UiNodeSpec {
                id: NodeId(2),
                kind: UiKind::Button,
                children: vec![],
                text: Some(TextSource::Constant(4)),
                on_click: Some(FunctionId(1)),
            },
        ],
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
            on_click: None,
        }],
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
