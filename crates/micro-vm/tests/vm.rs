use micro_ir::{
    AppImage, Constant, Function, FunctionId, FunctionKind, HandlerId, Instruction, NodeId,
    ScalarType, StateDecl, StateId, TextSource, UiKind, UiNodeSpec,
};
use micro_vm::{Execution, StateAccess, StateError, Value, Vm, VmError};

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
