//! LVGL renderer adapter behind a platform-neutral native bridge trait.

use micro_core::{MicroUiTree, RenderError, RenderPatch, RenderPort};
use micro_ir::{FunctionId, NodeId, UiKind};

pub trait NativeUi {
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_label(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
    ) -> Result<(), String>;
    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
    ) -> Result<(), String>;
    fn set_label_text(&mut self, node: NodeId, text: &str) -> Result<(), String>;
}

pub struct LvglRenderer<B> {
    bridge: B,
}

impl<B> LvglRenderer<B> {
    pub fn new(bridge: B) -> Self {
        Self { bridge }
    }

    pub fn bridge(&self) -> &B {
        &self.bridge
    }

    pub fn bridge_mut(&mut self) -> &mut B {
        &mut self.bridge
    }
}

impl<B: NativeUi> LvglRenderer<B> {
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
            UiKind::Column => self.bridge.create_column(node.id, parent),
            UiKind::Text => self.bridge.create_label(node.id, parent, &node.text),
            UiKind::Button => {
                let handler = node
                    .on_click
                    .ok_or_else(|| RenderError(format!("button {} has no handler", node.id.0)))?;
                self.bridge
                    .create_button(node.id, parent, &node.text, handler)
            }
        }
        .map_err(RenderError)?;
        for child in &node.children {
            self.create_node(tree, *child, Some(node.id))?;
        }
        Ok(())
    }
}

impl<B: NativeUi> RenderPort for LvglRenderer<B> {
    fn create_tree(&mut self, tree: &MicroUiTree) -> Result<(), RenderError> {
        self.create_node(tree, tree.root, None)
    }

    fn apply(&mut self, patches: &[RenderPatch]) -> Result<(), RenderError> {
        for patch in patches {
            let RenderPatch::SetText { node, text } = patch;
            self.bridge
                .set_label_text(*node, text)
                .map_err(RenderError)?;
        }
        Ok(())
    }
}
