mod activation;
pub mod host;
mod system;

#[cfg(any(target_arch = "wasm32", test))]
use micro_ir::{FontFamily, FontWeight, TextStyle};

pub use activation::{ActivationQueue, CheckboxChangeQueue, InputChangeQueue, SelectionChangeQueue, SliderChangeQueue};
pub use system::{SystemIntent, SystemShell, SystemSnapshot};

#[cfg(any(target_arch = "wasm32", test))]
fn inline_text_style(style: Option<&TextStyle>) -> Result<Option<String>, String> {
    let Some(style) = style else {
        return Err("Web text style must be normalized before DOM rendering".into());
    };
    let family = match style.family {
        FontFamily::UiSans => "MicroUiSans",
    };
    let weight = match style.weight {
        FontWeight::Regular => 400,
    };
    Ok(Some(format!(
        "font-family: {family}; font-size: {}px; font-weight: {weight}; line-height: {}px;",
        style.size_px, style.line_height_px
    )))
}

#[cfg(test)]
mod text_style_tests {
    use micro_ir::{FontWeight, TextStyle};

    use super::inline_text_style;

    #[test]
    fn maps_every_generated_metric_pair_to_browser_css() {
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (size_px, line_height_px) in TextStyle::UI_SANS_METRICS {
            let style = TextStyle::ui_sans(size_px, FontWeight::Regular, line_height_px).unwrap();
            assert_eq!(
                inline_text_style(Some(&style)),
                Ok(Some(format!(
                    "font-family: MicroUiSans; font-size: {size_px}px; font-weight: 400; line-height: {line_height_px}px;"
                )))
            );
            let generated = std::fs::read_to_string(
                repository.join(format!("assets/fonts/lvgl/micro_ui_sans_{size_px}.c")),
            )
            .unwrap();
            assert!(generated.contains(&format!(".line_height = {line_height_px},")));
        }
    }

    #[test]
    fn rejects_an_unnormalized_browser_style() {
        assert_eq!(
            inline_text_style(None),
            Err("Web text style must be normalized before DOM rendering".into())
        );
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_system {
    use wasm_bindgen::prelude::*;

    use crate::{SystemIntent, SystemShell};

    #[wasm_bindgen]
    pub struct MicroWebSystem {
        shell: SystemShell,
    }

    #[wasm_bindgen]
    impl MicroWebSystem {
        #[wasm_bindgen(constructor)]
        #[must_use]
        pub fn new() -> Self {
            Self {
                shell: SystemShell::configured_boot(),
            }
        }

        pub fn dispatch(&mut self, intent: &str) -> Result<String, JsValue> {
            let intent = SystemIntent::parse(intent)
                .ok_or_else(|| JsValue::from_str("WEB_SYSTEM: unknown intent"))?;
            serde_json::to_string(&self.shell.dispatch(intent))
                .map_err(|error| JsValue::from_str(&format!("WEB_SYSTEM: {error}")))
        }

        pub fn snapshot(&self) -> Result<String, JsValue> {
            serde_json::to_string(&self.shell.snapshot())
                .map_err(|error| JsValue::from_str(&format!("WEB_SYSTEM: {error}")))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_system::MicroWebSystem;

#[cfg(target_arch = "wasm32")]
mod dom;

#[cfg(target_arch = "wasm32")]
mod wasm_host {
    use micro_core::{Event, Runtime};
    use micro_ir::{FunctionId, decode};
    use micro_renderer_web::WebRenderer;
    use wasm_bindgen::prelude::*;

    use crate::{ActivationQueue, CheckboxChangeQueue, InputChangeQueue, SelectionChangeQueue, SliderChangeQueue};
    use crate::dom::DomBridge;
    use crate::host::{ShellState, WebHost};

    /// Decode an app MBC's manifest as a `name|icon` string for the launcher
    /// registry (reads the metadata section without running the app).
    #[wasm_bindgen]
    pub fn decode_app_metadata(mbc: &[u8]) -> Result<String, JsValue> {
        let image = decode(mbc)
            .map_err(|error| js_error("WEB_MBC", &format!("cannot decode MBC: {error}")))?;
        Ok(format!("{}|{}", image.metadata.name, image.metadata.icon))
    }

    #[wasm_bindgen]
    pub struct MicroWebRuntime {
        runtime: Runtime<WebRenderer<DomBridge>>,
        activations: ActivationQueue,
        input_changes: InputChangeQueue,
        slider_changes: SliderChangeQueue,
        checkbox_changes: CheckboxChangeQueue,
        selection_changes: SelectionChangeQueue,
        /// Shared OS-shell state: app registry + pending launch/back intents.
        nav: std::rc::Rc<std::cell::RefCell<ShellState>>,
    }

    #[wasm_bindgen]
    impl MicroWebRuntime {
        #[wasm_bindgen(constructor)]
        pub fn new(
            container_id: &str,
            mbc: &[u8],
            event_budget: u64,
            apps: &str,
        ) -> Result<MicroWebRuntime, JsValue> {
            let window = web_sys::window()
                .ok_or_else(|| js_error("WEB_CONTAINER", "window is unavailable"))?;
            let document = window
                .document()
                .ok_or_else(|| js_error("WEB_CONTAINER", "document is unavailable"))?;
            let container = document.get_element_by_id(container_id).ok_or_else(|| {
                js_error(
                    "WEB_CONTAINER",
                    &format!("element #{container_id} was not found"),
                )
            })?;
            let image = decode(mbc)
                .map_err(|error| js_error("WEB_MBC", &format!("cannot decode MBC: {error}")))?;
            let activations = ActivationQueue::default();
            let input_changes = InputChangeQueue::default();
            let slider_changes = SliderChangeQueue::default();
            let checkbox_changes = CheckboxChangeQueue::default();
            let selection_changes = SelectionChangeQueue::default();
            let bridge = DomBridge::new(
                document,
                container,
                activations.clone(),
                input_changes.clone(),
                slider_changes.clone(),
                checkbox_changes.clone(),
                selection_changes.clone(),
            );
            let renderer = WebRenderer::new(bridge);
            // The app registry must be present before the initial binding loop
            // materializes (the shell's os.appName/Icon bindings read it).
            let nav = std::rc::Rc::new(std::cell::RefCell::new(ShellState::default()));
            nav.borrow_mut().apps = apps
                .lines()
                .filter_map(|line| line.split_once('|'))
                .map(|(name, icon)| (name.to_owned(), icon.to_owned()))
                .collect();
            let mut host = WebHost::new();
            host.nav = nav.clone();
            let runtime = Runtime::new_with_host(image, renderer, event_budget, Box::new(host))
                .map_err(|error| {
                    js_error("WEB_RUNTIME", &format!("cannot create Runtime: {error}"))
                })?;
            Ok(Self {
                runtime,
                activations,
                input_changes,
                slider_changes,
                checkbox_changes,
                selection_changes,
                nav,
            })
        }

        /// Set the installed-app registry: `\n`-joined `name|icon` lines.
        pub fn set_apps(&mut self, apps: &str) {
            self.nav.borrow_mut().apps = apps
                .lines()
                .filter_map(|line| line.split_once('|'))
                .map(|(name, icon)| (name.to_owned(), icon.to_owned()))
                .collect();
        }

        /// Drain the pending `os.launchIndex(i)` request, or -1 when none.
        pub fn take_nav_launch(&mut self) -> i32 {
            match self.nav.borrow_mut().pending_launch.take() {
                Some(index) => index as i32,
                None => -1,
            }
        }

        /// Drain the pending `os.goBack` request.
        pub fn take_nav_back(&mut self) -> bool {
            let mut nav = self.nav.borrow_mut();
            let pending = nav.pending_back;
            nav.pending_back = false;
            pending
        }

        pub fn tick(&mut self) -> Result<u32, JsValue> {
            while let Some(handler) = self.activations.pop() {
                self.runtime.enqueue(Event::Activate(FunctionId(handler.0)));
            }
            while let Some((handler, text)) = self.input_changes.pop() {
                self.runtime.enqueue(Event::InputChanged(handler, text));
            }
            while let Some((handler, value)) = self.slider_changes.pop() {
                self.runtime.enqueue(Event::SliderChanged(handler, value));
            }
            while let Some((handler, checked)) = self.checkbox_changes.pop() {
                self.runtime.enqueue(Event::CheckedChanged(handler, checked));
            }
            while let Some((handler, index)) = self.selection_changes.pop() {
                self.runtime.enqueue(Event::SelectionChanged(handler, index));
            }

            let mut processed = 0_u32;
            loop {
                match self.runtime.tick() {
                    Ok(true) => processed = processed.saturating_add(1),
                    Ok(false) => break,
                    Err(error) => {
                        return Err(js_error(
                            "WEB_RUNTIME",
                            &format!("event processing failed: {error}"),
                        ));
                    }
                }
            }
            /* Async host requests (net.scanWifi / net.httpGet) complete one
             * tick later with the simulated result. */
            self.runtime.enqueue_host_results();
            Ok(processed)
        }

        pub fn dispose(&mut self) {
            self.runtime.renderer_mut().dom_mut().clear();
        }
    }

    fn js_error(code: &str, message: &str) -> JsValue {
        JsValue::from_str(&format!("{code}: {message}"))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_host::MicroWebRuntime;
