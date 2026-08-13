mod activation;

pub use activation::ActivationQueue;

#[cfg(target_arch = "wasm32")]
mod dom;

#[cfg(target_arch = "wasm32")]
mod wasm_host {
    use micro_core::{Event, Runtime};
    use micro_ir::{FunctionId, decode};
    use micro_renderer_web::WebRenderer;
    use wasm_bindgen::prelude::*;

    use crate::ActivationQueue;
    use crate::dom::DomBridge;

    #[wasm_bindgen]
    pub struct MicroWebRuntime {
        runtime: Runtime<WebRenderer<DomBridge>>,
        activations: ActivationQueue,
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
            let bridge = DomBridge::new(document, container, activations.clone());
            let renderer = WebRenderer::new(bridge);
            let runtime = Runtime::new(image, renderer, event_budget).map_err(|error| {
                js_error("WEB_RUNTIME", &format!("cannot create Runtime: {error}"))
            })?;
            Ok(Self {
                runtime,
                activations,
            })
        }

        pub fn tick(&mut self) -> Result<u32, JsValue> {
            while let Some(handler) = self.activations.pop() {
                self.runtime.enqueue(Event::Activate(FunctionId(handler.0)));
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
