//! Platform-neutral Web renderer behind a narrow DOM bridge.

use micro_core::{MicroUiTree, RenderError, RenderPatch, RenderPort};
use micro_ir::{FunctionId, NodeId, TextStyle, UiKind, sanitize_ui_text};

pub trait WebDom {
    fn report_diagnostic(&mut self, node: NodeId, message: &str);
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_text(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        style: Option<&TextStyle>,
    ) -> Result<(), String>;
    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
        style: Option<&TextStyle>,
    ) -> Result<(), String>;
    fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), String>;
}

pub struct WebRenderer<D> {
    dom: D,
}

impl<D> WebRenderer<D> {
    pub fn new(dom: D) -> Self {
        Self { dom }
    }

    pub fn dom(&self) -> &D {
        &self.dom
    }

    pub fn dom_mut(&mut self) -> &mut D {
        &mut self.dom
    }
}

impl<D: WebDom> WebRenderer<D> {
    fn checked_text(&mut self, node: NodeId, text: &str) -> String {
        let (text, replaced) = sanitize_ui_text(text);
        if replaced {
            self.dom
                .report_diagnostic(node, "unsupported glyph replaced with U+FFFD");
        }
        text.into_owned()
    }

    fn create_node(
        &mut self,
        tree: &MicroUiTree,
        node_id: NodeId,
        parent: Option<NodeId>,
    ) -> Result<(), RenderError> {
        let node = tree
            .nodes
            .get(node_id.0 as usize)
            .ok_or_else(|| RenderError(format!("node {} is missing", node_id.0)))?;
        match node.kind {
            UiKind::Column => self.dom.create_column(node.id, parent),
            UiKind::Text => {
                let text = self.checked_text(node.id, &node.text);
                let style = node.text_style.unwrap_or(TextStyle::DEFAULT_TEXT);
                self.dom.create_text(node.id, parent, &text, Some(&style))
            }
            UiKind::Button => {
                let handler = node
                    .on_click
                    .ok_or_else(|| RenderError(format!("button {} has no handler", node.id.0)))?;
                let text = self.checked_text(node.id, &node.text);
                let style = node.text_style.unwrap_or(TextStyle::DEFAULT_BUTTON);
                self.dom
                    .create_button(node.id, parent, &text, handler, Some(&style))
            }
        }
        .map_err(RenderError)?;

        for child in &node.children {
            self.create_node(tree, *child, Some(node.id))?;
        }
        Ok(())
    }
}

impl<D: WebDom> RenderPort for WebRenderer<D> {
    fn create_tree(&mut self, tree: &MicroUiTree) -> Result<(), RenderError> {
        self.create_node(tree, tree.root, None)
    }

    fn apply(&mut self, patches: &[RenderPatch]) -> Result<(), RenderError> {
        for patch in patches {
            let RenderPatch::SetText { node, text } = patch;
            let text = self.checked_text(*node, text);
            self.dom.set_text(*node, &text).map_err(RenderError)?;
        }
        Ok(())
    }
}
