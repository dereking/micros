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
    Diagnostic(NodeId, String),
}

impl WebDom for FakeDom {
    fn report_diagnostic(&mut self, node: NodeId, message: &str) {
        self.operations.push(Call::Diagnostic(node, message.into()));
    }
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
fn replaces_missing_runtime_glyph_and_reports_it() {
    let mut renderer = WebRenderer::new(FakeDom::default());
    renderer.create_tree(&tree_with_text("hello 🦄")).unwrap();
    renderer
        .apply(&[RenderPatch::SetText {
            node: NodeId(1),
            text: "next 🦄".into(),
        }])
        .unwrap();
    assert!(renderer.dom().operations.contains(&Call::Text(
        NodeId(1),
        Some(NodeId(0)),
        "hello �".into(),
        Some(TextStyle::ui_sans(24, FontWeight::Regular, 32).unwrap())
    )));
    assert!(
        renderer
            .dom()
            .operations
            .contains(&Call::SetText(NodeId(1), "next �".into()))
    );
    assert_eq!(
        renderer
            .dom()
            .operations
            .iter()
            .filter(|call| matches!(call, Call::Diagnostic(NodeId(1), _)))
            .count(),
        2
    );
}

fn tree_with_text(text: &str) -> MicroUiTree {
    MicroUiTree {
        root: NodeId(0),
        nodes: vec![
            MicroUiNode {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1)],
                text: String::new(),
                on_click: None,
                text_style: None,
            },
            MicroUiNode {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: text.into(),
                on_click: None,
                text_style: None,
            },
        ],
    }
}

#[test]
fn maps_tree_preorder_and_applies_text_patch() {
    let label_style = TextStyle::ui_sans(24, FontWeight::Regular, 32).unwrap();
    let button_style = TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap();
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

#[test]
fn normalizes_unstyled_text_and_button_before_dom_calls() {
    let mut tree = tree_with_text("欢迎");
    tree.nodes[0].children.push(NodeId(2));
    tree.nodes.push(MicroUiNode {
        id: NodeId(2),
        kind: UiKind::Button,
        children: vec![],
        text: "确认".into(),
        on_click: Some(FunctionId(7)),
        text_style: None,
    });
    let mut renderer = WebRenderer::new(FakeDom::default());
    renderer.create_tree(&tree).unwrap();

    assert!(renderer.dom().operations.contains(&Call::Text(
        NodeId(1),
        Some(NodeId(0)),
        "欢迎".into(),
        Some(TextStyle::DEFAULT_TEXT),
    )));
    assert!(renderer.dom().operations.contains(&Call::Button(
        NodeId(2),
        Some(NodeId(0)),
        "确认".into(),
        FunctionId(7),
        Some(TextStyle::DEFAULT_BUTTON),
    )));
}
