use micro_core::{MicroUiNode, MicroUiTree, RenderPatch, RenderPort};
use micro_ir::{FunctionId, NodeId, UiKind};
use micro_renderer_web::{WebDom, WebRenderer};

#[derive(Debug, Default)]
struct FakeDom {
    operations: Vec<String>,
}

impl WebDom for FakeDom {
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        self.operations
            .push(format!("column:{}:{:?}", node.0, parent.map(|id| id.0)));
        Ok(())
    }

    fn create_text(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
    ) -> Result<(), String> {
        self.operations.push(format!(
            "text:{}:{:?}:{text}",
            node.0,
            parent.map(|id| id.0)
        ));
        Ok(())
    }

    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
    ) -> Result<(), String> {
        self.operations.push(format!(
            "button:{}:{:?}:{text}:{}",
            node.0,
            parent.map(|id| id.0),
            handler.0
        ));
        Ok(())
    }

    fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        self.operations.push(format!("set_text:{}:{text}", node.0));
        Ok(())
    }
}

#[test]
fn maps_tree_preorder_and_applies_text_patch() {
    let tree = MicroUiTree {
        root: NodeId(0),
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
    };

    let mut renderer = WebRenderer::new(FakeDom::default());
    renderer.create_tree(&tree).unwrap();
    assert_eq!(
        renderer.dom().operations,
        [
            "column:0:None",
            "text:1:Some(0):Count: 0",
            "button:2:Some(0):Add:7",
        ]
    );

    renderer
        .apply(&[RenderPatch::SetText {
            node: NodeId(1),
            text: "Count: 1".into(),
        }])
        .unwrap();
    assert_eq!(
        renderer.dom().operations.last().unwrap(),
        "set_text:1:Count: 1"
    );
}
