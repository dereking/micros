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
    let label_style = TextStyle::ui_sans(24, FontWeight::Bold, 32).unwrap();
    let button_style = TextStyle::ui_sans(18, FontWeight::Medium, 24).unwrap();
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
    let label_style = TextStyle::ui_sans(24, FontWeight::Bold, 32).unwrap();
    let button_style = TextStyle::ui_sans(18, FontWeight::Medium, 24).unwrap();
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
