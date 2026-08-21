//! LVGL renderer adapter behind a platform-neutral native bridge trait.

use micro_core::{MicroUiTree, RenderError, RenderPatch, RenderPort};
use micro_ir::{FunctionId, NodeId, TextStyle, UiKind};

pub trait NativeUi {
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_label(
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
    fn set_label_text(&mut self, node: NodeId, text: &str) -> Result<(), String>;
    fn destroy_app_root(&mut self) -> Result<(), String>;
}

pub struct LvglRenderer<B: NativeUi> {
    bridge: B,
    owns_app_root: bool,
}

impl<B: NativeUi> LvglRenderer<B> {
    pub fn new(bridge: B) -> Self {
        Self {
            bridge,
            owns_app_root: false,
        }
    }

    pub fn bridge(&self) -> &B {
        &self.bridge
    }

    pub fn bridge_mut(&mut self) -> &mut B {
        &mut self.bridge
    }

    pub fn destroy_app_root(&mut self) -> Result<(), String> {
        if self.owns_app_root {
            self.bridge.destroy_app_root()?;
            self.owns_app_root = false;
        }
        Ok(())
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
            UiKind::Text => {
                self.bridge
                    .create_label(node.id, parent, &node.text, node.text_style.as_ref())
            }
            UiKind::Button => {
                let handler = node
                    .on_click
                    .ok_or_else(|| RenderError(format!("button {} has no handler", node.id.0)))?;
                self.bridge.create_button(
                    node.id,
                    parent,
                    &node.text,
                    handler,
                    node.text_style.as_ref(),
                )
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
        self.owns_app_root = true;
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

impl<B: NativeUi> Drop for LvglRenderer<B> {
    fn drop(&mut self) {
        let _ = self.destroy_app_root();
    }
}
