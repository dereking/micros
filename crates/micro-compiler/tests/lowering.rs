use micro_compiler::compile_source;
use micro_ir::{FunctionKind, Instruction, ScalarType, TextSource, UiKind, ValueSource};

#[test]
fn lowers_counter_studio_states_ui_bindings_and_handlers() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("app.ts", source).unwrap();

    assert_eq!(image.states.len(), 12);
    assert_eq!(
        image
            .states
            .iter()
            .filter(|slot| slot.ty == ScalarType::Number)
            .count(),
        11
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
    assert_eq!(bindings.len(), 17);

    let handlers: Vec<_> = image
        .functions
        .iter()
        .filter(|function| matches!(function.kind, FunctionKind::Handler(_)))
        .collect();
    assert_eq!(handlers.len(), 17);

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
    assert_eq!(root.children.len(), 3);
    // Tab titles are interned in the tabview's options.
    assert_eq!(root.options.len(), 3);
    // The first tab content is a column holding the styled title and the Add
    // button (the counter family).
    let first_tab = &image.nodes[root.children[0].0 as usize];
    assert_eq!(first_tab.kind, UiKind::Column);
    assert!(
        first_tab
            .children
            .iter()
            .any(|child| matches!(
                image.nodes[child.0 as usize].text,
                Some(TextSource::Constant(_))
            ))
    );
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

#[test]
fn expands_align_to_ltrb_and_lets_explicit_offsets_override() {
    let source = r#"
const x = state(0);
ui.mount(ui.column([
  ui.place(ui.text("a"), { align: "bottom" }),
  ui.place(ui.text("b"), { align: "bottom", bottom: 8 }),
  ui.place(ui.text("c"), { left: 16, top: 20 }),
  ui.place(ui.text("d"), { align: "client" }),
  ui.place(ui.text("e"), { left: 0, right: 0, bottom: 0 }),
]));
"#;
    let image = compile_source("place.ts", source).unwrap();
    let layouts = image
        .nodes
        .iter()
        .filter_map(|node| node.layout)
        .collect::<Vec<_>>();
    use micro_ir::LayoutSpec;
    let expected = [
        LayoutSpec { left: Some(0.0), top: None, right: Some(0.0), bottom: Some(0.0) }, // align bottom
        LayoutSpec { left: Some(0.0), top: None, right: Some(0.0), bottom: Some(8.0) }, // align bottom + bottom:8
        LayoutSpec { left: Some(16.0), top: Some(20.0), right: None, bottom: None }, // left+top only
        LayoutSpec { left: Some(0.0), top: Some(0.0), right: Some(0.0), bottom: Some(0.0) }, // align client
        LayoutSpec { left: Some(0.0), top: None, right: Some(0.0), bottom: Some(0.0) }, // explicit l/r/b
    ];
    for spec in expected {
        assert!(layouts.contains(&spec), "missing {spec:?} in {layouts:?}");
    }
    assert_eq!(layouts.len(), expected.len());
}
