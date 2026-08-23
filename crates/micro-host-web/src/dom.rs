use std::collections::BTreeMap;

use micro_ir::{FunctionId, LayoutSpec, NodeId, TextStyle};
use micro_renderer_web::WebDom;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Document, Element, Event};

use crate::{ActivationQueue, CheckboxChangeQueue, InputChangeQueue, SelectionChangeQueue, SliderChangeQueue, inline_text_style};

pub struct DomBridge {
    document: Document,
    container: Element,
    elements: BTreeMap<u32, Element>,
    activations: ActivationQueue,
    input_changes: InputChangeQueue,
    slider_changes: SliderChangeQueue,
    checkbox_changes: CheckboxChangeQueue,
    selection_changes: SelectionChangeQueue,
    active_tab: Option<u32>,
    /* (mask, left, top, right, bottom); mask bit0=left, bit1=top, bit2=right, bit3=bottom. */
    layout_specs: std::collections::BTreeMap<u32, (u8, f64, f64, f64, f64)>,
    click_handlers: Vec<Closure<dyn FnMut(Event)>>,
}

impl DomBridge {
    pub fn new(
        document: Document,
        container: Element,
        activations: ActivationQueue,
        input_changes: InputChangeQueue,
        slider_changes: SliderChangeQueue,
        checkbox_changes: CheckboxChangeQueue,
        selection_changes: SelectionChangeQueue,
    ) -> Self {
        Self {
            document,
            container,
            elements: BTreeMap::new(),
            activations,
            input_changes,
            slider_changes,
            checkbox_changes,
            selection_changes,
            active_tab: None,
            layout_specs: std::collections::BTreeMap::new(),
            click_handlers: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.container.set_text_content(None);
        self.elements.clear();
        self.layout_specs.clear();
        self.click_handlers.clear();
    }

    fn create_element(&self, tag: &str, node: NodeId, class: &str) -> Result<Element, String> {
        let element = self
            .document
            .create_element(tag)
            .map_err(|error| format!("create {tag}: {error:?}"))?;
        element
            .set_attribute("class", class)
            .map_err(|error| format!("set class: {error:?}"))?;
        element
            .set_attribute("data-micro-node", &node.0.to_string())
            .map_err(|error| format!("set node id: {error:?}"))?;
        Ok(element)
    }

    fn append(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        element: Element,
    ) -> Result<(), String> {
        let parent = match parent {
            Some(parent) => self
                .elements
                .get(&parent.0)
                .ok_or_else(|| format!("parent node {} is missing", parent.0))?,
            None => &self.container,
        };
        let target = if parent.class_name() == "micro-tabview" {
            /* Tab content children land on the page for the active tab. */
            if let Some(index) = self.active_tab {
                let page_index = index as usize * 2 + 1;
                if let Some(page) = parent
                    .clone()
                    .unchecked_into::<web_sys::Node>()
                    .child_nodes()
                    .item(page_index as u32)
                {
                    page.dyn_into::<web_sys::Element>().unwrap_or_else(|_| parent.clone())
                } else {
                    parent.clone()
                }
            } else {
                parent.clone()
            }
        } else {
            parent.clone()
        };
        target
            .append_child(&element)
            .map_err(|error| format!("append node {}: {error:?}", node.0))?;
        self.elements.insert(node.0, element);
        Ok(())
    }

    fn apply_text_style(element: &Element, style: Option<&TextStyle>) -> Result<(), String> {
        let Some(style) = inline_text_style(style)? else {
            return Ok(());
        };
        element
            .set_attribute("style", &style)
            .map_err(|error| format!("set text style: {error:?}"))
    }
}

impl WebDom for DomBridge {
    fn report_diagnostic(&mut self, node: NodeId, message: &str) {
        web_sys::console::warn_1(&format!("micro-ui node {}: {message}", node.0).into());
    }

    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        let element = self.create_element("div", node, "micro-column")?;
        self.append(node, parent, element)
    }

    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        let element = self.create_element("div", node, "micro-row")?;
        self.append(node, parent, element)
    }

    fn create_progress(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        fraction: f64,
    ) -> Result<(), String> {
        let element = self.create_element("progress", node, "micro-progress")?;
        element
            .set_attribute("max", "1")
            .map_err(|error| format!("set progress max: {error:?}"))?;
        element
            .set_attribute("value", &fraction.clamp(0.0, 1.0).to_string())
            .map_err(|error| format!("set progress value: {error:?}"))?;
        self.append(node, parent, element)
    }

    fn create_switch(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let element = self.create_element("button", node, "micro-switch")?;
        element
            .set_attribute("type", "button")
            .map_err(|error| format!("set switch type: {error:?}"))?;
        element
            .set_attribute("role", "switch")
            .map_err(|error| format!("set switch role: {error:?}"))?;
        element
            .set_attribute("aria-checked", if checked { "true" } else { "false" })
            .map_err(|error| format!("set switch aria: {error:?}"))?;
        if let Some(handler) = handler {
            let activations = self.activations.clone();
            let callback = Closure::wrap(Box::new(move |_event: Event| {
                activations.push(handler);
            }) as Box<dyn FnMut(Event)>);
            element
                .add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())
                .map_err(|error| format!("listen for switch click: {error:?}"))?;
            self.click_handlers.push(callback);
        }
        self.append(node, parent, element)
    }

    fn create_text(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let element = self.create_element("span", node, "micro-text")?;
        element.set_text_content(Some(text));
        Self::apply_text_style(&element, style)?;
        self.append(node, parent, element)
    }

    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let element = self.create_element("button", node, "micro-list-row")?;
        let is_row = parent.is_some()
            && self
                .elements
                .get(&parent.unwrap().0)
                .is_some_and(|el| el.class_name() == "micro-list");
        element
            .set_attribute("class", if is_row { "micro-list-row" } else { "micro-button" })
            .map_err(|error| format!("set button class: {error:?}"))?;
        element
            .set_attribute("type", "button")
            .map_err(|error| format!("set button type: {error:?}"))?;
        element.set_text_content(Some(text));
        Self::apply_text_style(&element, style)?;

        let activations = self.activations.clone();
        let callback = Closure::wrap(Box::new(move |_event: Event| {
            activations.push(handler);
        }) as Box<dyn FnMut(Event)>);
        element
            .add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())
            .map_err(|error| format!("listen for click: {error:?}"))?;
        self.click_handlers.push(callback);
        self.append(node, parent, element)
    }

    fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        element.set_text_content(Some(text));
        Ok(())
    }

    fn set_progress(&mut self, node: NodeId, fraction: f64) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        element
            .set_attribute("value", &fraction.clamp(0.0, 1.0).to_string())
            .map_err(|error| format!("set progress value: {error:?}"))
    }

    fn set_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        /* For a real checkbox input toggle the native property; otherwise
         * fall back to the aria attribute used by the switch button. */
        if let Ok(input) = element.clone().dyn_into::<web_sys::HtmlInputElement>() {
            input.set_checked(checked);
        }
        element
            .set_attribute("aria-checked", if checked { "true" } else { "false" })
            .map_err(|error| format!("set checked aria: {error:?}"))
    }

    fn create_input(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        placeholder: &str,
        handler: Option<FunctionId>,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let element = self.create_element("input", node, "micro-input")?;
        element
            .set_attribute("type", "text")
            .map_err(|error| format!("set input type: {error:?}"))?;
        element
            .set_attribute("value", text)
            .map_err(|error| format!("set input value: {error:?}"))?;
        if !placeholder.is_empty() {
            element
                .set_attribute("placeholder", placeholder)
                .map_err(|error| format!("set input placeholder: {error:?}"))?;
        }
        if let Some(handler) = handler {
            let input_changes = self.input_changes.clone();
            let callback = Closure::wrap(Box::new(move |event: Event| {
                let target = event.target();
                if let Some(value) = target
                    .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                    .map(|input| input.value())
                {
                    input_changes.push(handler, value);
                }
            }) as Box<dyn FnMut(Event)>);
            element
                .add_event_listener_with_callback("input", callback.as_ref().unchecked_ref())
                .map_err(|error| format!("listen for input change: {error:?}"))?;
            self.click_handlers.push(callback);
        }
        self.append(node, parent, element)
    }

    fn set_input_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        element
            .set_attribute("value", text)
            .map_err(|error| format!("set input value: {error:?}"))
    }

    fn create_slider(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let element = self.create_element("input", node, "micro-slider")?;
        element
            .set_attribute("type", "range")
            .map_err(|error| format!("set slider type: {error:?}"))?;
        let (min, max) = range.unwrap_or((0.0, 100.0));
        element
            .set_attribute("min", &min.to_string())
            .map_err(|error| format!("set slider min: {error:?}"))?;
        element
            .set_attribute("max", &max.to_string())
            .map_err(|error| format!("set slider max: {error:?}"))?;
        element
            .set_attribute("value", &value.to_string())
            .map_err(|error| format!("set slider value: {error:?}"))?;
        if let Some(handler) = handler {
            let slider_changes = self.slider_changes.clone();
            let callback = Closure::wrap(Box::new(move |event: Event| {
                if let Some(value) = event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                    .and_then(|input| input.value().parse::<f64>().ok())
                {
                    slider_changes.push(handler, value);
                }
            }) as Box<dyn FnMut(Event)>);
            element
                .add_event_listener_with_callback("input", callback.as_ref().unchecked_ref())
                .map_err(|error| format!("listen for slider change: {error:?}"))?;
            self.click_handlers.push(callback);
        }
        self.append(node, parent, element)
    }

    fn set_slider_value(&mut self, node: NodeId, value: f64) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        element
            .set_attribute("value", &value.to_string())
            .map_err(|error| format!("set slider value: {error:?}"))
    }

    fn create_dropdown(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let element = self.create_element("select", node, "micro-dropdown")?;
        for (i, option) in options.iter().enumerate() {
            let opt = self
                .document
                .create_element("option")
                .map_err(|error| format!("create dropdown option: {error:?}"))?;
            opt.set_text_content(Some(option));
            opt.set_attribute("value", &i.to_string())
                .map_err(|error| format!("set dropdown option value: {error:?}"))?;
            element
                .append_child(&opt)
                .map_err(|error| format!("append dropdown option: {error:?}"))?;
        }
        element
            .set_attribute("value", &index.to_string())
            .map_err(|error| format!("set dropdown value: {error:?}"))?;
        if let Some(handler) = handler {
            let selection_changes = self.selection_changes.clone();
            let callback = Closure::wrap(Box::new(move |event: Event| {
                if let Some(index) = event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok())
                    .and_then(|sel| sel.value().parse::<f64>().ok())
                {
                    selection_changes.push(handler, index);
                }
            }) as Box<dyn FnMut(Event)>);
            element
                .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
                .map_err(|error| format!("listen for dropdown change: {error:?}"))?;
            self.click_handlers.push(callback);
        }
        self.append(node, parent, element)
    }

    fn create_roller(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        self.create_dropdown(node, parent, options, index, handler)
    }

    fn create_list(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        let element = self.create_element("div", node, "micro-list")?;
        self.append(node, parent, element)
    }

    fn create_tabview(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        titles: &[String],
    ) -> Result<(), String> {
        let element = self.create_element("div", node, "micro-tabview")?;
        for title in titles {
            let tab = self
                .document
                .create_element("button")
                .map_err(|error| format!("create tab: {error:?}"))?;
            tab.set_attribute("class", "micro-tab-title")
                .map_err(|error| format!("set tab class: {error:?}"))?;
            tab.set_text_content(Some(title));
            element
                .append_child(&tab)
                .map_err(|error| format!("append tab: {error:?}"))?;
            let page = self
                .document
                .create_element("div")
                .map_err(|error| format!("create tab page: {error:?}"))?;
            page.set_attribute("class", "micro-tab-page")
                .map_err(|error| format!("set tab page class: {error:?}"))?;
            element
                .append_child(&page)
                .map_err(|error| format!("append tab page: {error:?}"))?;
        }
        self.append(node, parent, element)
    }

    fn set_layout_spec(&mut self, node: NodeId, layout: LayoutSpec) -> Result<(), String> {
        let mask = layout.left.map_or(0, |_| 1)
            | layout.top.map_or(0, |_| 2)
            | layout.right.map_or(0, |_| 4)
            | layout.bottom.map_or(0, |_| 8);
        self.layout_specs.insert(
            node.0,
            (
                mask,
                layout.left.unwrap_or(0.0),
                layout.top.unwrap_or(0.0),
                layout.right.unwrap_or(0.0),
                layout.bottom.unwrap_or(0.0),
            ),
        );
        Ok(())
    }

    fn apply_delphi_layout(
        &mut self,
        container: NodeId,
        children: &[NodeId],
    ) -> Result<(), String> {
        let container_el = self
            .elements
            .get(&container.0)
            .ok_or_else(|| format!("container node {} is missing", container.0))?;
        let width = container_el.client_width() as f64;
        /* Match the web column's CSS gap so the computed height lines up with
         * how the in-flow (un-placed) children are already spaced. */
        let row_gap = 16.0;

        /* Pass 1 — measure every child so the container (and the scrollable
         * page) grows to hold the docked children, mirroring the C engine. */
        let mut top_extent = 0.0;
        let mut bottom_extent = 0.0;
        let mut top_count = 0usize;
        let mut bottom_count = 0usize;
        for child in children {
            let el = self
                .elements
                .get(&child.0)
                .ok_or_else(|| format!("child node {} is missing", child.0))?;
            let h = el.client_height() as f64;
            match self.layout_specs.get(&child.0).copied() {
                None => {
                    /* Un-placed child stays in flow, behaving as a top dock. */
                    top_extent += h;
                    top_count += 1;
                }
                Some((mask, _l, _t, _r, b)) => {
                    if mask & 2 != 0 && mask & 8 != 0 {
                        continue; /* vertical fill takes whatever remains */
                    }
                    if mask & 8 != 0 {
                        bottom_extent += h + b;
                        bottom_count += 1;
                    } else {
                        top_extent += h + if mask & 2 != 0 { _t } else { 0.0 };
                        top_count += 1;
                    }
                }
            }
        }
        let gaps = (top_count.saturating_sub(1) + bottom_count.saturating_sub(1)
            + usize::from(top_count > 0 && bottom_count > 0)) as f64
            * row_gap;
        let avail_h = top_extent + bottom_extent + gaps;
        container_el
            .set_attribute(
                "style",
                &format!("position:relative;height:{}px;", avail_h),
            )
            .map_err(|error| format!("size delphi container: {error:?}"))?;

        /* Pass 2 — position each placed child by its LTRB role. */
        let mut top_y = 0.0;
        let mut bottom_y = avail_h;
        for child in children {
            let Some((mask, l, t, r, b)) = self.layout_specs.get(&child.0).copied() else {
                continue;
            };
            let el = self
                .elements
                .get(&child.0)
                .ok_or_else(|| format!("child node {} is missing", child.0))?;
            let w = el.client_width() as f64;
            let h = el.client_height() as f64;
            let (x, style_w) = if mask & 1 != 0 && mask & 4 != 0 {
                (l, width - l - r)
            } else if mask & 1 != 0 {
                (l, w)
            } else if mask & 4 != 0 {
                (width - w - r, w)
            } else {
                (0.0, width)
            };
            let (y, style_h) = if mask & 2 != 0 && mask & 8 != 0 {
                (top_y, (bottom_y - top_y).max(0.0))
            } else if mask & 8 != 0 {
                bottom_y -= h;
                let y = bottom_y - b;
                bottom_y = y - row_gap;
                (y, h)
            } else {
                let y = top_y + if mask & 2 != 0 { t } else { 0.0 };
                top_y = y + h + row_gap;
                (y, h)
            };
            let style = format!(
                "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;",
                x, y, style_w, style_h
            );
            el.set_attribute("style", &style)
                .map_err(|error| format!("apply delphi layout: {error:?}"))?;
        }
        Ok(())
    }

    fn create_tab_content(&mut self, index: u32) -> Result<(), String> {
        /* Children of a tabview are appended after the title/page pairs: the
         * last page is at position 2*index+1 within the tabview, but the child
         * itself is appended by the generic append(). We just record the
         * current page count for ordering via a data attribute on the element
         * the child will land in. */
        self.active_tab = Some(index);
        Ok(())
    }

    fn create_led(&mut self, node: NodeId, parent: Option<NodeId>, on: bool) -> Result<(), String> {
        let element = self.create_element("span", node, "micro-led")?;
        element
            .set_attribute("data-on", if on { "1" } else { "0" })
            .map_err(|error| format!("set led state: {error:?}"))?;
        self.append(node, parent, element)
    }

    fn set_led(&mut self, node: NodeId, on: bool) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        element
            .set_attribute("data-on", if on { "1" } else { "0" })
            .map_err(|error| format!("set led state: {error:?}"))
    }

    fn create_spinner(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        active: bool,
    ) -> Result<(), String> {
        let element = self.create_element("div", node, "micro-spinner")?;
        if active {
            element
                .set_attribute("data-active", "1")
                .map_err(|error| format!("set spinner: {error:?}"))?;
        }
        self.append(node, parent, element)
    }

    fn set_spinner(&mut self, node: NodeId, active: bool) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        if active {
            element
                .set_attribute("data-active", "1")
                .map_err(|error| format!("set spinner: {error:?}"))
        } else {
            element
                .remove_attribute("data-active")
                .map_err(|error| format!("set spinner: {error:?}"))
        }
    }

    fn create_scale(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
    ) -> Result<(), String> {
        let (min, max) = range.unwrap_or((0.0, 100.0));
        let element = self.create_element("meter", node, "micro-scale")?;
        element
            .set_attribute("min", &min.to_string())
            .map_err(|error| format!("set scale min: {error:?}"))?;
        element
            .set_attribute("max", &max.to_string())
            .map_err(|error| format!("set scale max: {error:?}"))?;
        element
            .set_attribute("value", &value.to_string())
            .map_err(|error| format!("set scale value: {error:?}"))?;
        self.append(node, parent, element)
    }

    fn set_scale_value(&mut self, node: NodeId, value: f64) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        element
            .set_attribute("value", &value.to_string())
            .map_err(|error| format!("set scale value: {error:?}"))
    }

    fn set_selection_value(&mut self, node: NodeId, index: f64) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        element
            .set_attribute("value", &index.to_string())
            .map_err(|error| format!("set dropdown value: {error:?}"))
    }

    fn create_checkbox(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        label: &str,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let label_el = self.create_element("label", node, "micro-checkbox")?;
        let input = self
            .document
            .create_element("input")
            .map_err(|error| format!("create checkbox input: {error:?}"))?;
        input
            .set_attribute("type", "checkbox")
            .map_err(|error| format!("set checkbox type: {error:?}"))?;
        if checked {
            input
                .set_attribute("checked", "")
                .map_err(|error| format!("set checkbox checked: {error:?}"))?;
        }
        if let Some(handler) = handler {
            let checkbox_changes = self.checkbox_changes.clone();
            let callback = Closure::wrap(Box::new(move |event: Event| {
                if let Some(checked) = event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                    .map(|input| input.checked())
                {
                    checkbox_changes.push(handler, checked);
                }
            }) as Box<dyn FnMut(Event)>);
            input
                .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
                .map_err(|error| format!("listen for checkbox change: {error:?}"))?;
            self.click_handlers.push(callback);
        }
        label_el
            .append_child(&input)
            .map_err(|error| format!("append checkbox input: {error:?}"))?;
        let text = self
            .document
            .create_element("span")
            .map_err(|error| format!("create checkbox label: {error:?}"))?;
        text.set_text_content(Some(label));
        label_el
            .append_child(&text)
            .map_err(|error| format!("append checkbox label: {error:?}"))?;
        self.append(node, parent, label_el)
    }
}
