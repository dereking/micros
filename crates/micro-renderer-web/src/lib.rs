//! Platform-neutral Web renderer behind a narrow DOM bridge.

use micro_core::{MicroUiTree, RenderError, RenderPatch, RenderPort, Value};
use micro_ir::{FunctionId, LayoutSpec, NodeId, TextStyle, UiKind, sanitize_ui_text};

pub trait WebDom {
    fn report_diagnostic(&mut self, node: NodeId, message: &str);
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    /// Decorative filled rectangle behind other nodes. `label` is the
    /// accessible name for a tappable surface ("" when decorative). Hosts
    /// that do not implement a palette fall back to an unstyled container.
    fn create_surface(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        _tone: &str,
        _handler: Option<FunctionId>,
        _label: &str,
    ) -> Result<(), String> {
        self.create_column(node, parent)
    }
    /// `ui.show` toggles an existing node in place. Hosts that do not support
    /// conditional visibility may leave the default no-op.
    fn set_visible(&mut self, _node: NodeId, _visible: bool) -> Result<(), String> {
        Ok(())
    }
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
        variant: Option<&str>,
    ) -> Result<(), String>;
    fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), String>;
    /// A shared OS icon resolved from `micro_ir::icons`. `name` is the icon
    /// name (not a glyph); web hosts render inline SVG. The default renders
    /// the name as text so a minimal DOM bridge still shows something.
    #[allow(clippy::too_many_arguments)]
    fn create_icon(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        name: &str,
        _size_px: u8,
        _tone: &str,
        _variant: &str,
        _handler: Option<FunctionId>,
    ) -> Result<(), String> {
        self.create_text(node, parent, name, None)
    }
    fn set_icon(&mut self, node: NodeId, name: &str) -> Result<(), String> {
        self.set_text(node, name)
    }
    fn set_progress(&mut self, node: NodeId, fraction: f64) -> Result<(), String>;
    fn set_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String>;
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
    fn set_layout_spec(&mut self, node: NodeId, layout: LayoutSpec) -> Result<(), String>;
    fn apply_delphi_layout(&mut self, container: NodeId, children: &[NodeId])
    -> Result<(), String>;
    fn create_led(&mut self, node: NodeId, parent: Option<NodeId>, on: bool) -> Result<(), String>;
    fn set_led(&mut self, node: NodeId, on: bool) -> Result<(), String>;
    fn create_spinner(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        active: bool,
    ) -> Result<(), String>;
    fn set_spinner(&mut self, node: NodeId, active: bool) -> Result<(), String>;
    fn create_scale(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
    ) -> Result<(), String>;
    fn set_scale_value(&mut self, node: NodeId, value: f64) -> Result<(), String>;
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
            UiKind::Row => self.dom.create_row(node.id, parent),
            UiKind::Progress => {
                let Some(Value::Number(fraction)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "progress {} has no numeric value",
                        node.id.0
                    )));
                };
                self.dom
                    .create_progress(node.id, parent, fraction.clamp(0.0, 1.0))
            }
            UiKind::Surface => {
                let tone = node.options.first().map(String::as_str).unwrap_or("paper");
                let label = self.checked_text(node.id, &node.text);
                self.dom
                    .create_surface(node.id, parent, tone, node.on_click, &label)
            }
            UiKind::Icon => {
                let name = self.checked_text(node.id, &node.text);
                let style = node.text_style.unwrap_or(TextStyle::DEFAULT_ICON);
                let tone = node.options.first().map(String::as_str).unwrap_or("ink");
                let variant = node.options.get(1).map(String::as_str).unwrap_or("plain");
                let name = if micro_ir::icons::icon(&name).is_some() || name.chars().count() == 1 {
                    name
                } else {
                    self.dom.report_diagnostic(
                        node.id,
                        &format!("unknown icon `{name}` rendered as ?"),
                    );
                    "?".to_owned()
                };
                self.dom.create_icon(
                    node.id,
                    parent,
                    &name,
                    style.size_px,
                    tone,
                    variant,
                    node.on_click,
                )
            }
            UiKind::Switch => {
                let Some(Value::Bool(checked)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "switch {} has no boolean value",
                        node.id.0
                    )));
                };
                self.dom
                    .create_switch(node.id, parent, *checked, node.on_click)
            }
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
                let variant = node.options.first().map(String::as_str);
                self.dom
                    .create_button(node.id, parent, &text, handler, Some(&style), variant)
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
                self.dom.create_input(
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
                self.dom
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
                self.dom
                    .create_checkbox(node.id, parent, &label, *checked, node.on_click)
            }
            UiKind::Dropdown => {
                let Some(Value::Number(index)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "dropdown {} has no numeric index",
                        node.id.0
                    )));
                };
                self.dom
                    .create_dropdown(node.id, parent, &node.options, *index, node.on_click)
            }
            UiKind::Roller => {
                let Some(Value::Number(index)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "roller {} has no numeric index",
                        node.id.0
                    )));
                };
                self.dom
                    .create_roller(node.id, parent, &node.options, *index, node.on_click)
            }
            UiKind::List => self.dom.create_list(node.id, parent),
            UiKind::Tabview => {
                self.dom
                    .create_tabview(node.id, parent, &node.options)
                    .map_err(RenderError)?;
                for (index, child) in node.children.iter().enumerate() {
                    self.dom
                        .create_tab_content(index as u32)
                        .map_err(RenderError)?;
                    self.create_node(tree, *child, Some(node.id))?;
                }
                Ok(())
            }
            UiKind::Led => {
                let Some(Value::Bool(on)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "led {} has no boolean value",
                        node.id.0
                    )));
                };
                self.dom.create_led(node.id, parent, *on)
            }
            UiKind::Spinner => {
                let Some(Value::Bool(active)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "spinner {} has no boolean value",
                        node.id.0
                    )));
                };
                self.dom.create_spinner(node.id, parent, *active)
            }
            UiKind::Scale => {
                let Some(Value::Number(value)) = node.value.as_ref() else {
                    return Err(RenderError(format!(
                        "scale {} has no numeric value",
                        node.id.0
                    )));
                };
                self.dom.create_scale(node.id, parent, *value, node.range)
            }
        }
        .map_err(RenderError)?;

        for child in &node.children {
            /* Tabview mounts its content children itself; skip them here. */
            if !matches!(node.kind, UiKind::Tabview) {
                self.create_node(tree, *child, Some(node.id))?;
            }
        }
        if matches!(node.kind, UiKind::Column | UiKind::Row)
            && node.children.iter().any(|c| {
                tree.nodes
                    .get(c.0 as usize)
                    .is_some_and(|n| n.layout.is_some())
            })
        {
            for child in &node.children {
                if let Some(layout) = tree.nodes[child.0 as usize].layout {
                    self.dom
                        .set_layout_spec(*child, layout)
                        .map_err(RenderError)?;
                }
            }
            self.dom
                .apply_delphi_layout(node.id, &node.children)
                .map_err(RenderError)?;
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
            match patch {
                RenderPatch::SetVisible { node, visible } => {
                    self.dom.set_visible(*node, *visible).map_err(RenderError)?
                }
                RenderPatch::SetText { node, text } => {
                    let text = self.checked_text(*node, text);
                    self.dom.set_text(*node, &text).map_err(RenderError)?;
                }
                RenderPatch::SetIcon { node, name } => {
                    let name = self.checked_text(*node, name);
                    self.dom.set_icon(*node, &name).map_err(RenderError)?;
                }
                RenderPatch::SetProgress { node, fraction } => self
                    .dom
                    .set_progress(*node, fraction.clamp(0.0, 1.0))
                    .map_err(RenderError)?,
                RenderPatch::SetChecked { node, checked } => {
                    self.dom.set_checked(*node, *checked).map_err(RenderError)?
                }
                RenderPatch::SetInputText { node, text } => {
                    let text = self.checked_text(*node, text);
                    self.dom.set_input_text(*node, &text).map_err(RenderError)?;
                }
                RenderPatch::SetSliderValue { node, value } => self
                    .dom
                    .set_slider_value(*node, *value)
                    .map_err(RenderError)?,
                RenderPatch::SetSelectionValue { node, index } => self
                    .dom
                    .set_selection_value(*node, *index)
                    .map_err(RenderError)?,
                RenderPatch::SetLed { node, on } => {
                    self.dom.set_led(*node, *on).map_err(RenderError)?
                }
                RenderPatch::SetSpinner { node, active } => {
                    self.dom.set_spinner(*node, *active).map_err(RenderError)?
                }
                RenderPatch::SetScaleValue { node, value } => self
                    .dom
                    .set_scale_value(*node, *value)
                    .map_err(RenderError)?,
            }
        }
        Ok(())
    }
}
