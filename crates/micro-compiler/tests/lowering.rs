use micro_compiler::compile_source;
use micro_ir::{FunctionKind, Instruction, ScalarType, TextSource, UiKind, ValueSource};

#[test]
fn lowers_counter_studio_states_ui_bindings_and_handlers() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("app.ts", source).unwrap();

    assert_eq!(image.states.len(), 8);
    assert_eq!(
        image
            .states
            .iter()
            .filter(|slot| slot.ty == ScalarType::Number)
            .count(),
        7
    );
    assert_eq!(
        image
            .states
            .iter()
            .filter(|slot| slot.ty == ScalarType::String)
            .count(),
        1
    );

    let bindings: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Binding(_)))
        .collect();
    assert_eq!(bindings.len(), 15);

    let handlers: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .collect();
    assert_eq!(handlers.len(), 10);

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
    assert_eq!(root.kind, UiKind::Column);
    assert_eq!(root.children.len(), 18);
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
