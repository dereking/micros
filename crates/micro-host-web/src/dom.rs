use std::collections::BTreeMap;

use micro_ir::{FunctionId, NodeId, TextStyle};
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
            click_handlers: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.container.set_text_content(None);
        self.elements.clear();
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
        parent
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
        let element = self.create_element("button", node, "micro-button")?;
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

    fn set_dropdown_value(&mut self, node: NodeId, index: f64) -> Result<(), String> {
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
