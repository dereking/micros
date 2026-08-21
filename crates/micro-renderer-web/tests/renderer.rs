use micro_core::{MicroUiNode, MicroUiTree, RenderPatch, RenderPort};
use micro_ir::{FontWeight, FunctionId, NodeId, TextStyle, UiKind};
use micro_renderer_web::{WebDom, WebRenderer};

#[derive(Debug, Default)]
struct FakeDom {
    operations: Vec<Call>,
}

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Column(NodeId, Option<NodeId>),
    Text(NodeId, Option<NodeId>, String, Option<TextStyle>),
    Button(
        NodeId,
        Option<NodeId>,
        String,
        FunctionId,
        Option<TextStyle>,
    ),
    SetText(NodeId, String),
}

impl WebDom for FakeDom {
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        self.operations.push(Call::Column(node, parent));
        Ok(())
    }

    fn create_text(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        self.operations
            .push(Call::Text(node, parent, text.into(), style.copied()));
        Ok(())
    }

    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        self.operations.push(Call::Button(
            node,
            parent,
            text.into(),
            handler,
            style.copied(),
        ));
        Ok(())
    }

    fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        self.operations.push(Call::SetText(node, text.into()));
        Ok(())
    }
}

#[test]
fn maps_tree_preorder_and_applies_text_patch() {
    let label_style = TextStyle::ui_sans(24, FontWeight::Bold, 32).unwrap();
    let button_style = TextStyle::ui_sans(18, FontWeight::Medium, 24).unwrap();
    let tree = MicroUiTree {
        root: NodeId(0),
        nodes: vec![
            MicroUiNode {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1), NodeId(2)],
                text: String::new(),
                on_click: None,
                text_style: None,
            },
            MicroUiNode {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: "Count: 0".into(),
                on_click: None,
                text_style: Some(label_style),
            },
            MicroUiNode {
                id: NodeId(2),
                kind: UiKind::Button,
                children: vec![],
                text: "Add".into(),
                on_click: Some(FunctionId(7)),
                text_style: Some(button_style),
            },
        ],
    };

    let mut renderer = WebRenderer::new(FakeDom::default());
    renderer.create_tree(&tree).unwrap();
    assert_eq!(
        renderer.dom().operations,
        [
            Call::Column(NodeId(0), None),
            Call::Text(
                NodeId(1),
                Some(NodeId(0)),
                "Count: 0".into(),
                Some(label_style),
            ),
            Call::Button(
                NodeId(2),
                Some(NodeId(0)),
                "Add".into(),
                FunctionId(7),
                Some(button_style),
            ),
        ]
    );

    renderer
        .apply(&[RenderPatch::SetText {
            node: NodeId(1),
            text: "Count: 1".into(),
        }])
        .unwrap();
    assert_eq!(
        renderer.dom().operations.last(),
        Some(&Call::SetText(NodeId(1), "Count: 1".into()))
    );
}
