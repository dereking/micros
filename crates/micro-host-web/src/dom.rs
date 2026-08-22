use std::collections::BTreeMap;

use micro_ir::{FunctionId, NodeId, TextStyle};
use micro_renderer_web::WebDom;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Document, Element, Event};

use crate::{ActivationQueue, InputChangeQueue, inline_text_style};

pub struct DomBridge {
    document: Document,
    container: Element,
    elements: BTreeMap<u32, Element>,
    activations: ActivationQueue,
    input_changes: InputChangeQueue,
    click_handlers: Vec<Closure<dyn FnMut(Event)>>,
}

impl DomBridge {
    pub fn new(
        document: Document,
        container: Element,
        activations: ActivationQueue,
        input_changes: InputChangeQueue,
    ) -> Self {
        Self {
            document,
            container,
            elements: BTreeMap::new(),
            activations,
            input_changes,
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
        element
            .set_attribute("aria-checked", if checked { "true" } else { "false" })
            .map_err(|error| format!("set switch aria: {error:?}"))
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
}
