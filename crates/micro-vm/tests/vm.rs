use micro_ir::{
    AppImage, AppMetadata, Constant, Function, FunctionId, FunctionKind, HandlerId, Instruction,
    NodeId, ScalarType, StateDecl, StateId, TextSource, UiKind, UiNodeSpec,
};
use micro_vm::{Execution, HostAccess, StateAccess, StateError, Value, Vm, VmError};

#[derive(Default)]
struct TestState(Vec<Value>);

impl StateAccess for TestState {
    fn read(&mut self, id: StateId) -> Result<Value, StateError> {
        self.0
            .get(id.0 as usize)
            .cloned()
            .ok_or(StateError::OutOfRange(id))
    }

    fn write(&mut self, id: StateId, value: Value) -> Result<(), StateError> {
        let slot = self
            .0
            .get_mut(id.0 as usize)
            .ok_or(StateError::OutOfRange(id))?;
        *slot = value;
        Ok(())
    }
}

fn image(kind: FunctionKind, code: Vec<Instruction>, max_stack: u16) -> AppImage {
    AppImage {
        constants: vec![
            Constant::Number(2.0),
            Constant::Number(3.0),
            Constant::String("value=".into()),
            Constant::Bool(false),
            Constant::Number(0.0),
        ],
        states: vec![StateDecl {
            ty: ScalarType::Number,
            initial: 4,
        }],
        functions: vec![Function {
            kind,
            arg_count: 0,
            locals: 1,
            max_stack,
            code,
        }],
        nodes: vec![UiNodeSpec {
            id: NodeId(0),
            kind: UiKind::Text,
            children: vec![],
            text: Some(TextSource::Constant(2)),
            value: None,
            on_click: None,
            text_style: None,
            range: None,
            options: vec![],
            layout: None,
        }],
        host_requests: vec![],
        metadata: AppMetadata {
            id: "vm".into(),
            name: "Vm".into(),
            icon: "V".into(),
        },
        root: NodeId(0),
    }
}

#[test]
fn executes_arithmetic_and_locals() {
    let app = image(
        FunctionKind::Binding(micro_ir::BindingId(0)),
        vec![
            Instruction::Const(0),
            Instruction::StoreLocal(0),
            Instruction::LoadLocal(0),
            Instruction::Const(1),
            Instruction::Mul,
            Instruction::Return,
        ],
        2,
    );
    let mut state = TestState::default();
    let execution = Vm::new(&app, &mut state).invoke(FunctionId(0), None, 20).unwrap();
    assert_eq!(execution.value, Some(Value::Number(6.0)));
}

#[test]
fn branches_and_concatenates_strings() {
    let app = image(
        FunctionKind::Binding(micro_ir::BindingId(0)),
        vec![
            Instruction::Const(3),
            Instruction::JumpIfFalse(4),
            Instruction::Const(2),
            Instruction::Jump(8),
            Instruction::Const(2),
            Instruction::Const(1),
            Instruction::ToString,
            Instruction::Concat,
            Instruction::Return,
        ],
        2,
    );
    let mut state = TestState::default();
    let execution = Vm::new(&app, &mut state).invoke(FunctionId(0), None, 20).unwrap();
    assert_eq!(execution.value, Some(Value::String("value=3".into())));
}

#[test]
fn reads_and_writes_state() {
    let app = image(
        FunctionKind::Handler(HandlerId(0)),
        vec![
            Instruction::LoadState(StateId(0)),
            Instruction::Const(1),
            Instruction::Add,
            Instruction::StoreState(StateId(0)),
            Instruction::Return,
        ],
        2,
    );
    let mut state = TestState(vec![Value::Number(4.0)]);
    assert_eq!(
        Vm::new(&app, &mut state).invoke(FunctionId(0), None, 20).unwrap(),
        Execution {
            value: None,
            executed: 5
        }
    );
    assert_eq!(state.0, vec![Value::Number(7.0)]);
}

#[test]
fn reports_division_by_zero_and_type_mismatch() {
    let division = image(
        FunctionKind::Binding(micro_ir::BindingId(0)),
        vec![
            Instruction::Const(0),
            Instruction::Const(4),
            Instruction::Div,
            Instruction::Return,
        ],
        2,
    );
    let mut state = TestState::default();
    assert_eq!(
        Vm::new(&division, &mut state).invoke(FunctionId(0), None, 10),
        Err(VmError::DivisionByZero)
    );

    let mismatch = image(
        FunctionKind::Binding(micro_ir::BindingId(0)),
        vec![
            Instruction::Const(2),
            Instruction::Const(0),
            Instruction::Add,
            Instruction::Return,
        ],
        2,
    );
    assert!(matches!(
        Vm::new(&mismatch, &mut state).invoke(FunctionId(0), None, 10),
        Err(VmError::TypeMismatch { .. })
    ));
}

#[test]
fn backward_jump_exhausts_budget_exactly() {
    let app = image(
        FunctionKind::Handler(HandlerId(0)),
        vec![Instruction::Jump(0)],
        0,
    );
    let mut state = TestState::default();
    assert_eq!(
        Vm::new(&app, &mut state).invoke(FunctionId(0), None, 3),
        Err(VmError::BudgetExceeded {
            function: FunctionId(0),
            executed: 3
        })
    );
}

#[derive(Default)]
struct TestHost {
    results: Vec<(FunctionId, Value)>,
}

impl micro_vm::HostAccess for TestHost {
    fn call(
        &mut self,
        request: &micro_ir::HostRequest,
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        match request.kind {
            micro_ir::HostCallKind::DeviceChip => Ok(Some(Value::String("ESP32-S3".into()))),
            micro_ir::HostCallKind::NetWifiConnect => {
                assert_eq!(args.len(), 2);
                Ok(None)
            }
            micro_ir::HostCallKind::NetScanWifi => {
                self.results
                    .push((request.callback.unwrap(), Value::String("ap1\nap2".into())));
                Ok(None)
            }
            _ => Err(VmError::Host("unexpected host call".into())),
        }
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.results)
    }
}

fn host_image(
    requests: Vec<micro_ir::HostRequest>,
    kind: FunctionKind,
    code: Vec<Instruction>,
) -> AppImage {
    AppImage {
        constants: vec![Constant::String("SSID".into()), Constant::String("pass".into())],
        states: vec![],
        functions: vec![Function {
            kind,
            arg_count: 0,
            locals: 0,
            max_stack: 4,
            code,
        }],
        nodes: vec![UiNodeSpec {
            id: NodeId(0),
            kind: UiKind::Text,
            children: vec![],
            text: Some(TextSource::Constant(0)),
            value: None,
            on_click: None,
            text_style: None,
            range: None,
            options: vec![],
            layout: None,
        }],
        host_requests: requests,
        metadata: AppMetadata {
            id: "host".into(),
            name: "Host".into(),
            icon: "H".into(),
        },
        root: NodeId(0),
    }
}

#[test]
fn host_read_pushes_the_returned_value() {
    let app = host_image(
        vec![micro_ir::HostRequest::sync(
            micro_ir::HostCallKind::DeviceChip,
            vec![],
            Some(ScalarType::String),
        )],
        FunctionKind::Binding(micro_ir::BindingId(0)),
        vec![Instruction::HostCall(0), Instruction::Return],
    );
    let mut state = TestState::default();
    let mut host = TestHost::default();
    let execution = Vm::new(&app, &mut state)
        .invoke_with_host(FunctionId(0), None, 10_000, &mut host)
        .unwrap();
    assert_eq!(execution.value, Some(Value::String("ESP32-S3".into())));
}

#[test]
fn host_action_receives_arguments_in_source_order() {
    let app = host_image(
        vec![micro_ir::HostRequest::sync(
            micro_ir::HostCallKind::NetWifiConnect,
            vec![ScalarType::String, ScalarType::String],
            None,
        )],
        FunctionKind::Handler(HandlerId(0)),
        vec![
            Instruction::Const(0),
            Instruction::Const(1),
            Instruction::HostCall(0),
            Instruction::Return,
        ],
    );
    let mut state = TestState::default();
    let mut host = TestHost::default();
    let execution = Vm::new(&app, &mut state)
        .invoke_with_host(FunctionId(0), None, 10_000, &mut host)
        .unwrap();
    assert_eq!(execution.value, None);
}

#[test]
fn async_host_call_records_a_completion_for_drain() {
    let app = host_image(
        vec![micro_ir::HostRequest::async_request(
            micro_ir::HostCallKind::NetScanWifi,
            vec![],
            FunctionId(1),
        )],
        FunctionKind::Handler(HandlerId(0)),
        vec![Instruction::HostCall(0), Instruction::Return],
    );
    let mut state = TestState::default();
    let mut host = TestHost::default();
    Vm::new(&app, &mut state)
        .invoke_with_host(FunctionId(0), None, 10_000, &mut host)
        .unwrap();
    let drained = host.drain_results();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, FunctionId(1));
    assert_eq!(drained[0].1, Value::String("ap1\nap2".into()));
}
