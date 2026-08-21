use micro_compiler::compile_source;
use micro_ir::{FunctionKind, Instruction, ScalarType, TextSource, UiKind};

#[test]
fn lowers_counter_studio_states_ui_bindings_and_handlers() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("app.ts", source).unwrap();

    assert_eq!(image.states.len(), 2);
    assert!(image.states.iter().all(|slot| slot.ty == ScalarType::Number));

    let bindings: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Binding(_)))
        .collect();
    assert_eq!(bindings.len(), 3);

    let handlers: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .collect();
    assert_eq!(handlers.len(), 3);

    // The first handler (Add) increments count (state 0) and presses (state 1).
    let stores: Vec<_> = handlers[0]
        .code
        .iter()
        .filter_map(|op| match op {
            Instruction::StoreState(id) => Some(id.0),
            _ => None,
        })
        .collect();
    assert_eq!(stores, [0, 1]);
    // The third handler (Double) doubles count and increments presses.
    let loads: Vec<_> = handlers[2]
        .code
        .iter()
        .filter_map(|op| match op {
            Instruction::LoadState(id) => Some(id.0),
            _ => None,
        })
        .collect();
    assert!(loads.contains(&0) && loads.contains(&1));

    let root = &image.nodes[image.root.0 as usize];
    assert_eq!(root.kind, UiKind::Column);
    assert_eq!(root.children.len(), 7);
    // The first child is a styled static title.
    let title = &image.nodes[root.children[0].0 as usize];
    assert!(matches!(title.text, Some(TextSource::Constant(_))));
    // The Add button is the first button and has a handler.
    let add = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button)
        .unwrap();
    assert!(add.on_click.is_some());
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
