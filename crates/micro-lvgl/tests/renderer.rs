use std::cell::Cell;
use std::rc::Rc;

use micro_core::{MicroUiNode, MicroUiTree, RenderPatch, RenderPort, Value};
use micro_ir::{FontWeight, FunctionId, NodeId, TextStyle, UiKind};
use micro_lvgl::{LvglRenderer, NativeUi};

#[derive(Debug, PartialEq)]
enum Call {
    Column(NodeId, Option<NodeId>),
    Row(NodeId, Option<NodeId>),
    Progress(NodeId, Option<NodeId>, f64),
    Switch(NodeId, Option<NodeId>, bool, Option<FunctionId>),
    Label(NodeId, Option<NodeId>, String, Option<TextStyle>),
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

#[derive(Default)]
struct FakeBridge(Vec<Call>);

impl NativeUi for FakeBridge {
    fn report_diagnostic(&mut self, node: NodeId, message: &str) {
        self.0.push(Call::Diagnostic(node, message.into()));
    }
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        self.0.push(Call::Column(node, parent));
        Ok(())
    }
    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        self.0.push(Call::Row(node, parent));
        Ok(())
    }
    fn create_progress(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        fraction: f64,
    ) -> Result<(), String> {
        self.0.push(Call::Progress(node, parent, fraction));
        Ok(())
    }
    fn create_switch(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        self.0.push(Call::Switch(node, parent, checked, handler));
        Ok(())
    }
    fn create_label(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        self.0
            .push(Call::Label(node, parent, text.into(), style.copied()));
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
        self.0.push(Call::Button(
            node,
            parent,
            text.into(),
            handler,
            style.copied(),
        ));
        Ok(())
    }
    fn set_label_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        self.0.push(Call::SetText(node, text.into()));
        Ok(())
    }
    fn set_progress_value(&mut self, node: NodeId, fraction: f64) -> Result<(), String> {
        self.0.push(Call::SetProgress(node, fraction));
        Ok(())
    }
    fn set_switch_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String> {
        self.0.push(Call::SetChecked(node, checked));
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

    fn create_led(&mut self, _node: NodeId, _parent: Option<NodeId>, _on: bool) -> Result<(), String> {
        Ok(())
    }

    fn set_led(&mut self, _node: NodeId, _on: bool) -> Result<(), String> {
        Ok(())
    }

    fn create_spinner(&mut self, _node: NodeId, _parent: Option<NodeId>, _active: bool) -> Result<(), String> {
        Ok(())
    }

    fn set_spinner(&mut self, _node: NodeId, _active: bool) -> Result<(), String> {
        Ok(())
    }

    fn create_scale(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _value: f64,
        _range: Option<(f64, f64)>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_scale_value(&mut self, _node: NodeId, _value: f64) -> Result<(), String> {
        Ok(())
    }

    fn create_list(&mut self, _node: NodeId, _parent: Option<NodeId>) -> Result<(), String> {
        Ok(())
    }

    fn create_tabview(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _titles: &[String],
    ) -> Result<(), String> {
        Ok(())
    }

    fn create_tab_content(&mut self, _index: u32) -> Result<(), String> {
        Ok(())
    }

    fn destroy_app_root(&mut self) -> Result<(), String> {
        Ok(())
    }
}

fn tree() -> MicroUiTree {
    let label_style = TextStyle::ui_sans(24, FontWeight::Regular, 32).unwrap();
    let button_style = TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap();
    MicroUiTree {
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
        root: NodeId(0),
    }
}

#[test]
fn maps_tree_preorder_and_applies_text_only_patch() {
    let label_style = TextStyle::ui_sans(24, FontWeight::Regular, 32).unwrap();
    let button_style = TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap();
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
            Call::Label(
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
            Call::SetText(NodeId(1), "Count: 1".into()),
        ]
    );
}

#[test]
fn maps_row_progress_and_switch_and_applies_value_patches() {
    let mut renderer = LvglRenderer::new(FakeBridge::default());
    renderer
        .create_tree(&MicroUiTree {
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
            root: NodeId(0),
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
        renderer.bridge().0,
        [
            Call::Column(NodeId(0), None),
            Call::Row(NodeId(1), Some(NodeId(0))),
            Call::Label(
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
fn normalizes_unstyled_text_and_button_before_lvgl_calls() {
    let mut tree = tree();
    tree.nodes[1].text_style = None;
    tree.nodes[2].text_style = None;
    let mut renderer = LvglRenderer::new(FakeBridge::default());
    renderer.create_tree(&tree).unwrap();

    assert!(renderer.bridge().0.contains(&Call::Label(
        NodeId(1),
        Some(NodeId(0)),
        "Count: 0".into(),
        Some(TextStyle::DEFAULT_TEXT),
    )));
    assert!(renderer.bridge().0.contains(&Call::Button(
        NodeId(2),
        Some(NodeId(0)),
        "Add".into(),
        FunctionId(7),
        Some(TextStyle::DEFAULT_BUTTON),
    )));
}

struct TrackingBridge {
    root_created: bool,
    fail_label: bool,
    destroyed: Rc<Cell<usize>>,
}

impl NativeUi for TrackingBridge {
    fn report_diagnostic(&mut self, _node: NodeId, _message: &str) {}
    fn create_column(&mut self, _node: NodeId, _parent: Option<NodeId>) -> Result<(), String> {
        self.root_created = true;
        Ok(())
    }
    fn create_row(&mut self, _node: NodeId, _parent: Option<NodeId>) -> Result<(), String> {
        Ok(())
    }
    fn create_progress(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _fraction: f64,
    ) -> Result<(), String> {
        Ok(())
    }
    fn create_switch(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _checked: bool,
        _handler: Option<FunctionId>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn create_label(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _text: &str,
        _style: Option<&TextStyle>,
    ) -> Result<(), String> {
        if self.fail_label {
            Err("injected label failure".into())
        } else {
            Ok(())
        }
    }

    fn create_button(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _text: &str,
        _handler: FunctionId,
        _style: Option<&TextStyle>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_label_text(&mut self, _node: NodeId, _text: &str) -> Result<(), String> {
        unreachable!()
    }
    fn set_progress_value(&mut self, _node: NodeId, _fraction: f64) -> Result<(), String> {
        unreachable!()
    }
    fn set_switch_checked(&mut self, _node: NodeId, _checked: bool) -> Result<(), String> {
        unreachable!()
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

    fn create_led(&mut self, _node: NodeId, _parent: Option<NodeId>, _on: bool) -> Result<(), String> {
        Ok(())
    }

    fn set_led(&mut self, _node: NodeId, _on: bool) -> Result<(), String> {
        Ok(())
    }

    fn create_spinner(&mut self, _node: NodeId, _parent: Option<NodeId>, _active: bool) -> Result<(), String> {
        Ok(())
    }

    fn set_spinner(&mut self, _node: NodeId, _active: bool) -> Result<(), String> {
        Ok(())
    }

    fn create_scale(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _value: f64,
        _range: Option<(f64, f64)>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_scale_value(&mut self, _node: NodeId, _value: f64) -> Result<(), String> {
        Ok(())
    }

    fn create_list(&mut self, _node: NodeId, _parent: Option<NodeId>) -> Result<(), String> {
        Ok(())
    }

    fn create_tabview(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _titles: &[String],
    ) -> Result<(), String> {
        Ok(())
    }

    fn create_tab_content(&mut self, _index: u32) -> Result<(), String> {
        Ok(())
    }

    fn destroy_app_root(&mut self) -> Result<(), String> {
        assert!(self.root_created);
        self.destroyed.set(self.destroyed.get() + 1);
        Ok(())
    }
}

#[test]
fn replaces_missing_runtime_glyph_and_reports_it() {
    let mut runtime_tree = tree();
    runtime_tree.nodes[1].text = "hello 🦄".into();
    let mut renderer = LvglRenderer::new(FakeBridge::default());
    renderer.create_tree(&runtime_tree).unwrap();
    renderer
        .apply(&[RenderPatch::SetText {
            node: NodeId(1),
            text: "next 🦄".into(),
        }])
        .unwrap();
    assert!(
        renderer
            .bridge()
            .0
            .iter()
            .any(|call| matches!(call, Call::Label(NodeId(1), _, text, _) if text == "hello �"))
    );
    assert!(
        renderer
            .bridge()
            .0
            .contains(&Call::SetText(NodeId(1), "next �".into()))
    );
    assert_eq!(
        renderer
            .bridge()
            .0
            .iter()
            .filter(|call| matches!(call, Call::Diagnostic(NodeId(1), _)))
            .count(),
        2
    );
}

#[test]
fn destroys_partially_created_root_when_renderer_is_dropped_after_failure() {
    let destroyed = Rc::new(Cell::new(0));
    {
        let bridge = TrackingBridge {
            root_created: false,
            fail_label: true,
            destroyed: Rc::clone(&destroyed),
        };
        let mut renderer = LvglRenderer::new(bridge);
        assert_eq!(
            renderer.create_tree(&tree()).unwrap_err().0,
            "injected label failure"
        );
    }
    assert_eq!(destroyed.get(), 1);
}

#[test]
fn explicit_root_destroy_disarms_drop_cleanup() {
    let destroyed = Rc::new(Cell::new(0));
    {
        let bridge = TrackingBridge {
            root_created: false,
            fail_label: false,
            destroyed: Rc::clone(&destroyed),
        };
        let mut renderer = LvglRenderer::new(bridge);
        renderer.create_tree(&tree()).unwrap();
        renderer.destroy_app_root().unwrap();
    }
    assert_eq!(destroyed.get(), 1);
}

struct SharedRootBridge {
    root_exists: Rc<Cell<bool>>,
    destroyed: Rc<Cell<usize>>,
}

impl NativeUi for SharedRootBridge {
    fn report_diagnostic(&mut self, _node: NodeId, _message: &str) {}

    fn create_column(&mut self, _node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        assert!(parent.is_none());
        if self.root_exists.replace(true) {
            Err("root already exists".into())
        } else {
            Ok(())
        }
    }
    fn create_row(&mut self, _node: NodeId, _parent: Option<NodeId>) -> Result<(), String> {
        Ok(())
    }
    fn create_progress(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _fraction: f64,
    ) -> Result<(), String> {
        Ok(())
    }
    fn create_switch(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _checked: bool,
        _handler: Option<FunctionId>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn create_label(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _text: &str,
        _style: Option<&TextStyle>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn create_button(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _text: &str,
        _handler: FunctionId,
        _style: Option<&TextStyle>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn set_label_text(&mut self, _node: NodeId, _text: &str) -> Result<(), String> {
        Ok(())
    }
    fn set_progress_value(&mut self, _node: NodeId, _fraction: f64) -> Result<(), String> {
        Ok(())
    }
    fn set_switch_checked(&mut self, _node: NodeId, _checked: bool) -> Result<(), String> {
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

    fn create_led(&mut self, _node: NodeId, _parent: Option<NodeId>, _on: bool) -> Result<(), String> {
        Ok(())
    }

    fn set_led(&mut self, _node: NodeId, _on: bool) -> Result<(), String> {
        Ok(())
    }

    fn create_spinner(&mut self, _node: NodeId, _parent: Option<NodeId>, _active: bool) -> Result<(), String> {
        Ok(())
    }

    fn set_spinner(&mut self, _node: NodeId, _active: bool) -> Result<(), String> {
        Ok(())
    }

    fn create_scale(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _value: f64,
        _range: Option<(f64, f64)>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_scale_value(&mut self, _node: NodeId, _value: f64) -> Result<(), String> {
        Ok(())
    }

    fn create_list(&mut self, _node: NodeId, _parent: Option<NodeId>) -> Result<(), String> {
        Ok(())
    }

    fn create_tabview(
        &mut self,
        _node: NodeId,
        _parent: Option<NodeId>,
        _titles: &[String],
    ) -> Result<(), String> {
        Ok(())
    }

    fn create_tab_content(&mut self, _index: u32) -> Result<(), String> {
        Ok(())
    }

    fn destroy_app_root(&mut self) -> Result<(), String> {
        assert!(self.root_exists.replace(false));
        self.destroyed.set(self.destroyed.get() + 1);
        Ok(())
    }
}

#[test]
fn failed_second_root_creation_does_not_destroy_first_renderers_root() {
    let root_exists = Rc::new(Cell::new(false));
    let destroyed = Rc::new(Cell::new(0));
    let make_bridge = || SharedRootBridge {
        root_exists: Rc::clone(&root_exists),
        destroyed: Rc::clone(&destroyed),
    };
    let mut first = LvglRenderer::new(make_bridge());
    first.create_tree(&tree()).unwrap();
    {
        let mut second = LvglRenderer::new(make_bridge());
        assert_eq!(
            second.create_tree(&tree()).unwrap_err().0,
            "root already exists"
        );
    }
    assert!(root_exists.get());
    assert_eq!(destroyed.get(), 0);
    drop(first);
    assert!(!root_exists.get());
    assert_eq!(destroyed.get(), 1);
}
