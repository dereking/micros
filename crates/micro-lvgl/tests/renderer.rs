use micro_core::{MicroUiNode, MicroUiTree, RenderPatch, RenderPort};
use micro_ir::{FunctionId, NodeId, UiKind};
use micro_lvgl::{LvglRenderer, NativeUi};

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Column(NodeId, Option<NodeId>),
    Label(NodeId, Option<NodeId>, String),
    Button(NodeId, Option<NodeId>, String, FunctionId),
    SetText(NodeId, String),
}

#[derive(Default)]
struct FakeBridge(Vec<Call>);

impl NativeUi for FakeBridge {
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        self.0.push(Call::Column(node, parent));
        Ok(())
    }
    fn create_label(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
    ) -> Result<(), String> {
        self.0.push(Call::Label(node, parent, text.into()));
        Ok(())
    }
    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
    ) -> Result<(), String> {
        self.0
            .push(Call::Button(node, parent, text.into(), handler));
        Ok(())
    }
    fn set_label_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        self.0.push(Call::SetText(node, text.into()));
        Ok(())
    }
}

fn tree() -> MicroUiTree {
    MicroUiTree {
        nodes: vec![
            MicroUiNode {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1), NodeId(2)],
                text: String::new(),
                on_click: None,
            },
            MicroUiNode {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: "Count: 0".into(),
                on_click: None,
            },
            MicroUiNode {
                id: NodeId(2),
                kind: UiKind::Button,
                children: vec![],
                text: "Add".into(),
                on_click: Some(FunctionId(7)),
            },
        ],
        root: NodeId(0),
    }
}

#[test]
fn maps_tree_preorder_and_applies_text_only_patch() {
    let mut renderer = LvglRenderer::new(FakeBridge::default());
    renderer.create_tree(&tree()).unwrap();
    renderer
        .apply(&[RenderPatch::SetText {
            node: NodeId(1),
            text: "Count: 1".into(),
        }])
        .unwrap();
    assert_eq!(
        renderer.bridge().0,
        [
            Call::Column(NodeId(0), None),
            Call::Label(NodeId(1), Some(NodeId(0)), "Count: 0".into()),
            Call::Button(NodeId(2), Some(NodeId(0)), "Add".into(), FunctionId(7)),
            Call::SetText(NodeId(1), "Count: 1".into()),
        ]
    );
}
