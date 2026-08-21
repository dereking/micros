use std::cell::Cell;
use std::rc::Rc;

use micro_core::{MicroUiNode, MicroUiTree, RenderPatch, RenderPort};
use micro_ir::{FontWeight, FunctionId, NodeId, TextStyle, UiKind};
use micro_lvgl::{LvglRenderer, NativeUi};

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Column(NodeId, Option<NodeId>),
    Label(NodeId, Option<NodeId>, String, Option<TextStyle>),
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
