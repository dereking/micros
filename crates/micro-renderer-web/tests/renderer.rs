use micro_core::{MicroUiNode, MicroUiTree, RenderPatch, RenderPort, Value};
use micro_ir::{FontWeight, FunctionId, NodeId, TextStyle, UiKind};
use micro_renderer_web::{WebDom, WebRenderer};

#[derive(Debug, Default)]
struct FakeDom {
    operations: Vec<Call>,
}

#[derive(Debug, PartialEq)]
enum Call {
    Column(NodeId, Option<NodeId>),
    Row(NodeId, Option<NodeId>),
    Progress(NodeId, Option<NodeId>, f64),
    Switch(NodeId, Option<NodeId>, bool, Option<FunctionId>),
    Text(NodeId, Option<NodeId>, String, Option<TextStyle>),
    Button(
        NodeId,
        Option<NodeId>,
        String,
        FunctionId,
        Option<TextStyle>,
    ),
    SetText(NodeId, String),
    SetProgress(NodeId, f64),
    SetChecked(NodeId, bool),
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

    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        self.operations.push(Call::Row(node, parent));
        Ok(())
    }

    fn create_progress(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        fraction: f64,
    ) -> Result<(), String> {
        self.operations.push(Call::Progress(node, parent, fraction));
        Ok(())
    }

    fn create_switch(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        self.operations
            .push(Call::Switch(node, parent, checked, handler));
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

    fn create_input(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _text: &str,
        _placeholder: &str,
        _handler: Option<FunctionId>,
        _style: Option<&TextStyle>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_input_text(&mut self, _node: NodeId, _text: &str) -> Result<(), String> {
        Ok(())
    }

    fn create_slider(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _value: f64,
        _range: Option<(f64, f64)>,
        _handler: Option<FunctionId>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_slider_value(&mut self, _node: NodeId, _value: f64) -> Result<(), String> {
        Ok(())
    }

    fn create_checkbox(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _label: &str,
        _checked: bool,
        _handler: Option<FunctionId>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn create_dropdown(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _options: &[String],
        _index: f64,
        _handler: Option<FunctionId>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_selection_value(&mut self, _node: NodeId, _index: f64) -> Result<(), String> {
        Ok(())
    }

    fn create_roller(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _options: &[String],
        _index: f64,
        _handler: Option<FunctionId>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        self.operations.push(Call::SetText(node, text.into()));
        Ok(())
    }

    fn set_progress(&mut self, node: NodeId, fraction: f64) -> Result<(), String> {
        self.operations.push(Call::SetProgress(node, fraction));
        Ok(())
    }

    fn set_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String> {
        self.operations.push(Call::SetChecked(node, checked));
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
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
            },
            MicroUiNode {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: text.into(),
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
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
                value: None,
                on_click: None,
                text_style: None,
                range: None,
                options: vec![],
            },
            MicroUiNode {
                id: NodeId(1),
                kind: UiKind::Text,
                children: vec![],
                text: "Count: 0".into(),
                value: None,
                on_click: None,
                text_style: Some(label_style),
                range: None,
                options: vec![],
            },
            MicroUiNode {
                id: NodeId(2),
                kind: UiKind::Button,
                children: vec![],
                text: "Add".into(),
                value: None,
                on_click: Some(FunctionId(7)),
                text_style: Some(button_style),
                range: None,
                options: vec![],
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
fn maps_row_progress_and_switch_and_applies_value_patches() {
    let mut renderer = WebRenderer::new(FakeDom::default());
    renderer
        .create_tree(&MicroUiTree {
            root: NodeId(0),
            nodes: vec![
                MicroUiNode {
                    id: NodeId(0),
                    kind: UiKind::Column,
                    children: vec![NodeId(1), NodeId(2)],
                    text: String::new(),
                    value: None,
                    on_click: None,
                    text_style: None,
                    range: None,
                    options: vec![],
                },
                MicroUiNode {
                    id: NodeId(1),
                    kind: UiKind::Row,
                    children: vec![NodeId(3), NodeId(4)],
                    text: String::new(),
                    value: None,
                    on_click: None,
                    text_style: None,
                    range: None,
                    options: vec![],
                },
                MicroUiNode {
                    id: NodeId(2),
                    kind: UiKind::Switch,
                    children: vec![],
                    text: String::new(),
                    value: Some(Value::Bool(false)),
                    on_click: Some(FunctionId(7)),
                    text_style: None,
                    range: None,
                    options: vec![],
                },
                MicroUiNode {
                    id: NodeId(3),
                    kind: UiKind::Text,
                    children: vec![],
                    text: "level: 3".into(),
                    value: None,
                    on_click: None,
                    text_style: None,
                    range: None,
                    options: vec![],
                },
                MicroUiNode {
                    id: NodeId(4),
                    kind: UiKind::Progress,
                    children: vec![],
                    text: String::new(),
                    value: Some(Value::Number(0.5)),
                    on_click: None,
                    text_style: None,
                    range: None,
                    options: vec![],
                },
            ],
        })
        .unwrap();
    renderer
        .apply(&[
            RenderPatch::SetProgress {
                node: NodeId(4),
                fraction: 0.75,
            },
            RenderPatch::SetChecked {
                node: NodeId(2),
                checked: true,
            },
        ])
        .unwrap();
    assert_eq!(
        renderer.dom().operations,
        [
            Call::Column(NodeId(0), None),
            Call::Row(NodeId(1), Some(NodeId(0))),
            Call::Text(
                NodeId(3),
                Some(NodeId(1)),
                "level: 3".into(),
                Some(TextStyle::DEFAULT_TEXT),
            ),
            Call::Progress(NodeId(4), Some(NodeId(1)), 0.5),
            Call::Switch(NodeId(2), Some(NodeId(0)), false, Some(FunctionId(7))),
            Call::SetProgress(NodeId(4), 0.75),
            Call::SetChecked(NodeId(2), true),
        ]
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
        value: None,
        on_click: Some(FunctionId(7)),
        text_style: None,
        range: None,
        options: vec![],
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
