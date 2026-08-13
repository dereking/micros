use micro_compiler::compile_source;
use micro_core::{Event, MicroUiTree, RenderError, RenderPatch, RenderPort, Runtime};
use micro_ir::{NodeId, UiKind};

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
    let tree = &runtime.renderer().created[0];
    let text = tree
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Text)
        .unwrap();
    let button = tree
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button)
        .unwrap();
    let handler = button.on_click.unwrap();
    assert_eq!(text.text, "Count: 0");

    runtime.enqueue(Event::Activate(handler));
    runtime.tick().unwrap();
    runtime.enqueue(Event::Activate(handler));
    runtime.tick().unwrap();

    assert_eq!(
        runtime.renderer().patches,
        [
            RenderPatch::SetText {
                node: NodeId(1),
                text: "Count: 1".into()
            },
            RenderPatch::SetText {
                node: NodeId(1),
                text: "Count: 2".into()
            },
        ]
    );
    assert_eq!(runtime.renderer().created.len(), 1);
}
