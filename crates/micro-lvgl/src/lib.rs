//! LVGL renderer adapter behind a platform-neutral native bridge trait.

use micro_core::{MicroUiTree, RenderError, RenderPatch, RenderPort, Value};
use micro_ir::{FunctionId, NodeId, TextStyle, UiKind, sanitize_ui_text};

pub trait NativeUi {
    fn report_diagnostic(&mut self, node: NodeId, message: &str);
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_progress(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        fraction: f64,
    ) -> Result<(), String>;
    fn create_switch(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String>;
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
    fn set_progress_value(&mut self, node: NodeId, fraction: f64) -> Result<(), String>;
    fn set_switch_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String>;
    fn create_input(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        placeholder: &str,
        handler: Option<FunctionId>,
        style: Option<&TextStyle>,
    ) -> Result<(), String>;
    fn set_input_text(&mut self, node: NodeId, text: &str) -> Result<(), String>;
    fn create_slider(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
        handler: Option<FunctionId>,
    ) -> Result<(), String>;
    fn set_slider_value(&mut self, node: NodeId, value: f64) -> Result<(), String>;
    fn create_checkbox(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        label: &str,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String>;
    fn create_dropdown(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String>;
    fn create_roller(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String>;
    fn set_selection_value(&mut self, node: NodeId, index: f64) -> Result<(), String>;
    fn create_list(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_tabview(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        titles: &[String],
    ) -> Result<(), String>;
    fn create_tab_content(&mut self, index: u32) -> Result<(), String>;
    fn create_led(&mut self, node: NodeId, parent: Option<NodeId>, on: bool) -> Result<(), String>;
    fn set_led(&mut self, node: NodeId, on: bool) -> Result<(), String>;
    fn create_spinner(&mut self, node: NodeId, parent: Option<NodeId>, active: bool)
        -> Result<(), String>;
    fn set_spinner(&mut self, node: NodeId, active: bool) -> Result<(), String>;
    fn create_scale(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
    ) -> Result<(), String>;
    fn set_scale_value(&mut self, node: NodeId, value: f64) -> Result<(), String>;
    fn destroy_app_root(&mut self) -> Result<(), String>;
}

pub struct LvglRenderer<B: NativeUi> {
    bridge: B,
    owns_app_root: bool,
}

impl<B: NativeUi> LvglRenderer<B> {
    fn checked_text(&mut self, node: NodeId, text: &str) -> String {
        let (text, replaced) = sanitize_ui_text(text);
        if replaced {
            self.bridge
                .report_diagnostic(node, "unsupported glyph replaced with U+FFFD");
        }
        text.into_owned()
    }

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
            UiKind::Row => self.bridge.create_row(node.id, parent),
            UiKind::Progress => {
                let Some(Value::Number(fraction)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "progress {} has no numeric value",
                        node.id.0
                    )));
                };
                self.bridge
                    .create_progress(node.id, parent, fraction.clamp(0.0, 1.0))
            }
            UiKind::Switch => {
                let Some(Value::Bool(checked)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "switch {} has no boolean value",
                        node.id.0
                    )));
                };
                self.bridge
                    .create_switch(node.id, parent, *checked, node.on_click)
            }
            UiKind::Text => {
                let text = self.checked_text(node.id, &node.text);
                let style = node.text_style.unwrap_or(TextStyle::DEFAULT_TEXT);
                self.bridge
                    .create_label(node.id, parent, &text, Some(&style))
            }
            UiKind::Button => {
                let handler = node
                    .on_click
                    .ok_or_else(|| RenderError(format!("button {} has no handler", node.id.0)))?;
                let text = self.checked_text(node.id, &node.text);
                let style = node.text_style.unwrap_or(TextStyle::DEFAULT_BUTTON);
                self.bridge
                    .create_button(node.id, parent, &text, handler, Some(&style))
            }
            UiKind::Input => {
                let Some(Value::String(text)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "input {} has no string value",
                        node.id.0
                    )));
                };
                let text = self.checked_text(node.id, text);
                let placeholder = self.checked_text(node.id, &node.text);
                let style = node.text_style.unwrap_or(TextStyle::DEFAULT_TEXT);
                self.bridge.create_input(
                    node.id,
                    parent,
                    &text,
                    &placeholder,
                    node.on_click,
                    Some(&style),
                )
            }
            UiKind::Slider => {
                let Some(Value::Number(value)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "slider {} has no numeric value",
                        node.id.0
                    )));
                };
                self.bridge
                    .create_slider(node.id, parent, *value, node.range, node.on_click)
            }
            UiKind::Checkbox => {
                let Some(Value::Bool(checked)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "checkbox {} has no boolean value",
                        node.id.0
                    )));
                };
                let label = self.checked_text(node.id, &node.text);
                self.bridge
                    .create_checkbox(node.id, parent, &label, *checked, node.on_click)
            }
            UiKind::Dropdown => {
                let Some(Value::Number(index)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "dropdown {} has no numeric index",
                        node.id.0
                    )));
                };
                self.bridge.create_dropdown(
                    node.id,
                    parent,
                    &node.options,
                    *index,
                    node.on_click,
                )
            }
            UiKind::Roller => {
                let Some(Value::Number(index)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "roller {} has no numeric index",
                        node.id.0
                    )));
                };
                self.bridge.create_roller(
                    node.id,
                    parent,
                    &node.options,
                    *index,
                    node.on_click,
                )
            }
            UiKind::List => self.bridge.create_list(node.id, parent),
            UiKind::Tabview => {
                self.bridge
                    .create_tabview(node.id, parent, &node.options)
                    .map_err(RenderError)?;
                for (index, child) in node.children.iter().enumerate() {
                    self.bridge
                        .create_tab_content(index as u32)
                        .map_err(RenderError)?;
                    self.create_node(tree, *child, Some(node.id))?;
                }
                Ok(())
            }
            UiKind::Led => {
                let Some(Value::Bool(on)) = node.value.as_ref() else {
                    return Err(RenderError(format!("led {} has no boolean value", node.id.0)));
                };
                self.bridge.create_led(node.id, parent, *on)
            }
            UiKind::Spinner => {
                let Some(Value::Bool(active)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "spinner {} has no boolean value",
                        node.id.0
                    )));
                };
                self.bridge.create_spinner(node.id, parent, *active)
            }
            UiKind::Scale => {
                let Some(Value::Number(value)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "scale {} has no numeric value",
                        node.id.0
                    )));
                };
                self.bridge.create_scale(node.id, parent, *value, node.range)
            }
        }
        .map_err(RenderError)?;
        if parent.is_none() {
            self.owns_app_root = true;
        }
        for child in &node.children {
            /* Tabview mounts its content children itself; skip them here. */
            if !matches!(node.kind, UiKind::Tabview) {
                self.create_node(tree, *child, Some(node.id))?;
            }
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
            match patch {
                RenderPatch::SetText { node, text } => {
                    let text = self.checked_text(*node, text);
                    self.bridge
                        .set_label_text(*node, &text)
                        .map_err(RenderError)?;
                }
                RenderPatch::SetProgress { node, fraction } => self
                    .bridge
                    .set_progress_value(*node, fraction.clamp(0.0, 1.0))
                    .map_err(RenderError)?,
                RenderPatch::SetChecked { node, checked } => self
                    .bridge
                    .set_switch_checked(*node, *checked)
                    .map_err(RenderError)?,
                RenderPatch::SetInputText { node, text } => {
                    let text = self.checked_text(*node, text);
                    self.bridge
                        .set_input_text(*node, &text)
                        .map_err(RenderError)?;
                }
                RenderPatch::SetSliderValue { node, value } => self
                    .bridge
                    .set_slider_value(*node, *value)
                    .map_err(RenderError)?,
                RenderPatch::SetSelectionValue { node, index } => self
                    .bridge
                    .set_selection_value(*node, *index)
                    .map_err(RenderError)?,
                RenderPatch::SetLed { node, on } => self
                    .bridge
                    .set_led(*node, *on)
                    .map_err(RenderError)?,
                RenderPatch::SetSpinner { node, active } => self
                    .bridge
                    .set_spinner(*node, *active)
                    .map_err(RenderError)?,
                RenderPatch::SetScaleValue { node, value } => self
                    .bridge
                    .set_scale_value(*node, *value)
                    .map_err(RenderError)?,
            }
        }
        Ok(())
    }
}

impl<B: NativeUi> Drop for LvglRenderer<B> {
    fn drop(&mut self) {
        let _ = self.destroy_app_root();
    }
}
