use std::fmt;

use micro_ir::{FunctionId, NodeId, TextStyle, UiKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicroUiNode {
    pub id: NodeId,
    pub kind: UiKind,
    pub children: Vec<NodeId>,
    pub text: String,
    pub on_click: Option<FunctionId>,
    pub text_style: Option<TextStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicroUiTree {
    pub nodes: Vec<MicroUiNode>,
    pub root: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderPatch {
    SetText { node: NodeId, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError(pub String);

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RenderError {}

pub trait RenderPort {
    fn create_tree(&mut self, tree: &MicroUiTree) -> Result<(), RenderError>;
    fn apply(&mut self, patches: &[RenderPatch]) -> Result<(), RenderError>;
}
