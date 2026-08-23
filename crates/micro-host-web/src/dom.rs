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
    /* (mask, l, t, w, h, al, at, ar, ab); mask bit0..7 = left,top,width,height,anchor_l,t,r,b. */
    layout_specs: std::collections::BTreeMap<u32, (u8, f64, f64, f64, f64, f64, f64, f64, f64)>,
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
            /* Tab content children land on the page for the active tab. The
             * tab bar is child 0; page i is child i+1. */
            if let Some(index) = self.active_tab {
                let page_index = index + 1;
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

    /* Shared <select> builder for dropdown and roller (a DOM-native roller
     * adaptation). `class` lets each keep its own styling surface. */
    fn create_select(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
        class: &str,
    ) -> Result<(), String> {
        let element = self.create_element("select", node, class)?;
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
}

/* Show the page at `index` and highlight its title, hiding the rest. The tab
 * bar is child 0; page i is child i+1, both built with append_child only, so
 * there are no text nodes between them and child_nodes() ordering is stable. */
fn activate_tab(tabview: &Element, index: u32) {
    let tabview_node = tabview.clone().unchecked_into::<web_sys::Node>();
    let children = tabview_node.child_nodes();
    let count = children.length();
    for position in 1..count {
        if let Some(node) = children.item(position) {
            if let Ok(page) = node.dyn_into::<Element>() {
                let class = if position == index + 1 {
                    "micro-tab-page micro-tab-page-active"
                } else {
                    "micro-tab-page"
                };
                let _ = page.set_attribute("class", class);
            }
        }
    }
    if let Some(bar_node) = children.item(0) {
        if let Ok(bar) = bar_node.dyn_into::<Element>() {
            let titles = bar.clone().unchecked_into::<web_sys::Node>().child_nodes();
            for position in 0..titles.length() {
                if let Some(title_node) = titles.item(position) {
                    if let Ok(title) = title_node.dyn_into::<Element>() {
                        let class = if position == index {
                            "micro-tab-title micro-tab-active"
                        } else {
                            "micro-tab-title"
                        };
                        let _ = title.set_attribute("class", class);
                    }
                }
            }
        }
    }
}

/* Map a gauge value within [min, max] to a 0..1 fraction for the needle. */
fn scale_fraction(value: f64, min: f64, max: f64) -> f64 {
    if max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn scale_range(element: &Element) -> (f64, f64) {
    let min = element
        .get_attribute("data-min")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let max = element
        .get_attribute("data-max")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100.0);
    (min, max)
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
        Self::apply_text_style(&element, style)?;
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
        self.create_select(node, parent, options, index, handler, "micro-dropdown")
    }

    fn create_roller(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        self.create_select(node, parent, options, index, handler, "micro-roller")
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
        /* One tab bar holding every title, then one page per tab. Keeping the
         * bar a single child means pages are stable children 1..n, which both
         * append() routing and activate_tab() rely on. */
        let bar = self
            .document
            .create_element("div")
            .map_err(|error| format!("create tab bar: {error:?}"))?;
        bar.set_attribute("class", "micro-tab-bar")
            .map_err(|error| format!("set tab bar class: {error:?}"))?;
        element
            .append_child(&bar)
            .map_err(|error| format!("append tab bar: {error:?}"))?;

        for (index, title) in titles.iter().enumerate() {
            let tab = self
                .document
                .create_element("button")
                .map_err(|error| format!("create tab: {error:?}"))?;
            tab.set_attribute("type", "button")
                .map_err(|error| format!("set tab type: {error:?}"))?;
            tab.set_attribute(
                "class",
                if index == 0 {
                    "micro-tab-title micro-tab-active"
                } else {
                    "micro-tab-title"
                },
            )
            .map_err(|error| format!("set tab class: {error:?}"))?;
            tab.set_text_content(Some(title));
            bar.append_child(&tab)
                .map_err(|error| format!("append tab: {error:?}"))?;

            let page = self
                .document
                .create_element("div")
                .map_err(|error| format!("create tab page: {error:?}"))?;
            page.set_attribute(
                "class",
                if index == 0 {
                    "micro-tab-page micro-tab-page-active"
                } else {
                    "micro-tab-page"
                },
            )
            .map_err(|error| format!("set tab page class: {error:?}"))?;
            element
                .append_child(&page)
                .map_err(|error| format!("append tab page: {error:?}"))?;

            /* Clicking a title shows its page and marks the title active. */
            let tabview = element.clone();
            let tab_index = index as u32;
            let callback = Closure::wrap(Box::new(move |_event: Event| {
                activate_tab(&tabview, tab_index);
            }) as Box<dyn FnMut(Event)>);
            tab.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())
                .map_err(|error| format!("listen for tab click: {error:?}"))?;
            self.click_handlers.push(callback);
        }
        self.append(node, parent, element)
    }

    fn set_layout_spec(&mut self, node: NodeId, layout: LayoutSpec) -> Result<(), String> {
        let mask = layout.left.map_or(0, |_| 1)
            | layout.top.map_or(0, |_| 2)
            | layout.width.map_or(0, |_| 4)
            | layout.height.map_or(0, |_| 8)
            | layout.anchor.left.map_or(0, |_| 16)
            | layout.anchor.top.map_or(0, |_| 32)
            | layout.anchor.right.map_or(0, |_| 64)
            | layout.anchor.bottom.map_or(0, |_| 128);
        self.layout_specs.insert(
            node.0,
            (
                mask,
                layout.left.unwrap_or(0.0),
                layout.top.unwrap_or(0.0),
                layout.width.unwrap_or(0.0),
                layout.height.unwrap_or(0.0),
                layout.anchor.left.unwrap_or(0.0),
                layout.anchor.top.unwrap_or(0.0),
                layout.anchor.right.unwrap_or(0.0),
                layout.anchor.bottom.unwrap_or(0.0),
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

        /* Pass 1 — measure every child so the container grows to hold the
         * absolutely-positioned children (mirroring the C engine). Mask bits:
         * 0=lt-left,1=lt-top,2=wh-width,3=wh-height,4=anchor_l,5=anchor_t,
         * 6=anchor_r,7=anchor_b. */
        let mut top_extent = 0.0;
        let mut bottom_extent = 0.0;
        for child in children {
            let el = self
                .elements
                .get(&child.0)
                .ok_or_else(|| format!("child node {} is missing", child.0))?;
            let h = el.client_height() as f64;
            match self.layout_specs.get(&child.0).copied() {
                None => {
                    /* Un-placed child stays in flow at the top. */
                    if h > top_extent {
                        top_extent = h;
                    }
                }
                Some((mask, _l, _t, _w, _h, _al, at, _ar, ab)) => {
                    let eh = if mask & 8 != 0 { _h } else { h }; /* explicit ?: content */
                    if mask & 32 != 0 && mask & 128 != 0 {
                        continue; /* vertical fill takes whatever remains */
                    }
                    if mask & 128 != 0 {
                        bottom_extent += eh + ab;
                    } else {
                        let top = if mask & 32 != 0 {
                            at
                        } else if mask & 2 != 0 {
                            _t
                        } else {
                            0.0
                        };
                        let bottom = top + eh;
                        if bottom > top_extent {
                            top_extent = bottom;
                        }
                    }
                }
            }
        }
        let avail_h = top_extent + bottom_extent;
        container_el
            .set_attribute(
                "style",
                &format!("position:relative;height:{}px;", avail_h),
            )
            .map_err(|error| format!("size delphi container: {error:?}"))?;

        /* Pass 2 — position each placed child absolutely by lt/wh + anchors. */
        for child in children {
            let Some((mask, l, t, w, h, al, at, ar, ab)) =
                self.layout_specs.get(&child.0).copied()
            else {
                continue;
            };
            let el = self
                .elements
                .get(&child.0)
                .ok_or_else(|| format!("child node {} is missing", child.0))?;
            let cw = el.client_width() as f64;
            let ch = el.client_height() as f64;
            /* Explicit width/height win over content size when set. */
            let ew = if mask & 4 != 0 { w } else { cw };
            let eh = if mask & 8 != 0 { h } else { ch };
            let (x, style_w) = if mask & 16 != 0 && mask & 64 != 0 {
                (al, width - al - ar)
            } else if mask & 64 != 0 {
                (width - ew - ar, ew)
            } else if mask & 16 != 0 {
                /* Pin the left edge (a lone left anchor, not a stretch). */
                (al, ew)
            } else {
                (if mask & 1 != 0 { l } else { 0.0 }, ew)
            };
            let (y, style_h) = if mask & 32 != 0 && mask & 128 != 0 {
                (at, (avail_h - at - ab).max(0.0))
            } else if mask & 128 != 0 {
                (avail_h - eh - ab, eh)
            } else if mask & 32 != 0 {
                /* Pin the top edge (a lone top anchor, not a stretch). */
                (at, eh)
            } else {
                (if mask & 2 != 0 { t } else { 0.0 }, eh)
            };
            /* Merge into the existing style instead of overwriting it:
             * replacing the whole `style` attribute would wipe a widget's own
             * inline styles (e.g. the font-size/line-height a text label
             * applied at creation). Runs once per container during create. */
            let mut style = el.get_attribute("style").unwrap_or_default();
            if !style.is_empty() && !style.ends_with(';') {
                style.push(';');
            }
            style.push_str(&format!(
                "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;",
                x, y, style_w, style_h
            ));
            el.set_attribute("style", &style)
                .map_err(|error| format!("apply delphi layout: {error:?}"))?;
        }
        Ok(())
    }

    fn create_tab_content(&mut self, index: u32) -> Result<(), String> {
        /* Record the active tab so the generic append() routes subsequent
         * children into that tab's page (page i is tabview child i+1, after
         * the tab bar at child 0). */
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
        let element = self.create_element("div", node, "micro-scale")?;
        element
            .set_attribute("data-min", &min.to_string())
            .map_err(|error| format!("set scale min: {error:?}"))?;
        element
            .set_attribute("data-max", &max.to_string())
            .map_err(|error| format!("set scale max: {error:?}"))?;
        element
            .set_attribute(
                "style",
                &format!("--micro-scale:{};", scale_fraction(value, min, max)),
            )
            .map_err(|error| format!("set scale value: {error:?}"))?;
        self.append(node, parent, element)
    }

    fn set_scale_value(&mut self, node: NodeId, value: f64) -> Result<(), String> {
        let element = self
            .elements
            .get(&node.0)
            .ok_or_else(|| format!("node {} is missing", node.0))?;
        let (min, max) = scale_range(element);
        /* Merge into the existing style: replacing the whole `style` would
         * wipe the absolute position/dimensions the delphi pass applied. */
        let style = element.get_attribute("style").unwrap_or_default();
        let mut parts: Vec<String> = style
            .split(';')
            .filter(|p| !p.trim_start().starts_with("--micro-scale"))
            .map(String::from)
            .collect();
        parts.push(format!("--micro-scale:{};", scale_fraction(value, min, max)));
        element
            .set_attribute("style", &parts.join(";"))
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
