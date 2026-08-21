use micro_compiler::compile_source;
use micro_core::{Event, MicroUiTree, RenderError, RenderPatch, RenderPort, Runtime};
use micro_ir::{TextStyle, UiKind};

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

#[test]
fn compiles_loads_and_clicks_the_real_counter() {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("apps/counter/app.ts", source).unwrap();
    let mut runtime = Runtime::new(image, RecordingRenderer::default(), 10_000).unwrap();
    let (count_id, count_style, add_style, handler) = {
        let tree = &runtime.renderer().created[0];
        let count_text = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Text && node.text.starts_with("Count: "))
            .expect("count display text");
        let add_button = tree
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Button && node.text == "Add")
            .expect("Add button");
        assert_eq!(count_text.text, "Count: 0");
        (
            count_text.id,
            count_text.text_style.clone(),
            add_button.text_style.clone(),
            add_button.on_click.unwrap(),
        )
    };
    assert_eq!(count_style, Some(TextStyle::DEFAULT_TEXT));
    assert_eq!(add_style, Some(TextStyle::DEFAULT_BUTTON));

    runtime.enqueue(Event::Activate(handler));
    runtime.tick().unwrap();
    runtime.enqueue(Event::Activate(handler));
    runtime.tick().unwrap();

    let count_patches: Vec<_> = runtime
        .renderer()
        .patches
        .iter()
        .filter_map(|patch| match patch {
            RenderPatch::SetText { node, text } if *node == count_id => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(count_patches, ["Count: 1", "Count: 2"]);
    assert_eq!(runtime.renderer().created.len(), 1);
}
