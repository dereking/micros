mod activation;
mod system;

#[cfg(any(target_arch = "wasm32", test))]
use micro_ir::{FontFamily, FontWeight, TextStyle};

pub use activation::{ActivationQueue, InputChangeQueue};
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

    use crate::{ActivationQueue, InputChangeQueue};
    use crate::dom::DomBridge;

    #[wasm_bindgen]
    pub struct MicroWebRuntime {
        runtime: Runtime<WebRenderer<DomBridge>>,
        activations: ActivationQueue,
        input_changes: InputChangeQueue,
    }

    #[wasm_bindgen]
    impl MicroWebRuntime {
        #[wasm_bindgen(constructor)]
        pub fn new(
            container_id: &str,
            mbc: &[u8],
            event_budget: u64,
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
            let bridge = DomBridge::new(document, container, activations.clone(), input_changes.clone());
            let renderer = WebRenderer::new(bridge);
            let runtime = Runtime::new(image, renderer, event_budget).map_err(|error| {
                js_error("WEB_RUNTIME", &format!("cannot create Runtime: {error}"))
            })?;
            Ok(Self {
                runtime,
                activations,
                input_changes,
            })
        }

        pub fn tick(&mut self) -> Result<u32, JsValue> {
            while let Some(handler) = self.activations.pop() {
                self.runtime.enqueue(Event::Activate(FunctionId(handler.0)));
            }
            while let Some((handler, text)) = self.input_changes.pop() {
                self.runtime.enqueue(Event::InputChanged(handler, text));
            }

            let mut processed = 0_u32;
            loop {
                match self.runtime.tick() {
                    Ok(true) => processed = processed.saturating_add(1),
                    Ok(false) => return Ok(processed),
                    Err(error) => {
                        return Err(js_error(
                            "WEB_RUNTIME",
                            &format!("event processing failed: {error}"),
                        ));
                    }
                }
            }
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
