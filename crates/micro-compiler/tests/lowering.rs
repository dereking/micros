use micro_compiler::compile_source;
use micro_ir::{FunctionKind, Instruction, ScalarType, TextSource, UiKind};

#[test]
fn lowers_counter_state_ui_binding_and_handler() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("app.ts", source).unwrap();

    assert_eq!(image.states.len(), 1);
    assert_eq!(image.states[0].ty, ScalarType::Number);
    assert_eq!(image.functions.len(), 2);
    assert!(matches!(image.functions[0].kind, FunctionKind::Binding(_)));
    assert!(matches!(image.functions[1].kind, FunctionKind::Handler(_)));
    assert_eq!(
        image.functions[1].code,
        [
            Instruction::LoadState(micro_ir::StateId(0)),
            Instruction::Const(3),
            Instruction::Add,
            Instruction::Dup,
            Instruction::StoreState(micro_ir::StateId(0)),
            Instruction::Pop,
            Instruction::Return,
        ]
    );
    assert_eq!(image.constants[3], micro_ir::Constant::Number(1.0));

    let root = &image.nodes[image.root.0 as usize];
    assert_eq!(root.kind, UiKind::Column);
    assert_eq!(root.children.len(), 2);
    let text = &image.nodes[root.children[0].0 as usize];
    assert!(matches!(text.text, Some(TextSource::Binding(_))));
    let button = &image.nodes[root.children[1].0 as usize];
    assert_eq!(button.kind, UiKind::Button);
    assert!(button.on_click.is_some());
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
