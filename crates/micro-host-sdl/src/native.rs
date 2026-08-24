use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::ptr::NonNull;
use std::rc::Rc;

use micro_ir::{FontFamily, FontWeight, FunctionId, NodeId, TextStyle};
use micro_lvgl::NativeUi;

unsafe extern "C" {
    fn micro_native_create(
        width: c_int,
        height: c_int,
        hidden: c_int,
        error: *mut c_char,
        error_length: usize,
    ) -> *mut c_void;
    fn micro_native_destroy(native: *mut c_void);
    fn micro_native_poll(native: *mut c_void) -> c_int;
    fn micro_native_timer(native: *mut c_void) -> c_uint;
    fn micro_native_take_activation(native: *mut c_void, handler_id: *mut c_uint) -> c_int;
    fn micro_native_inject_activation(native: *mut c_void, handler_id: c_uint);
    fn micro_native_queue_click(native: *mut c_void, node: c_uint) -> c_int;
    fn micro_native_create_column(native: *mut c_void, node: c_uint, parent: c_uint) -> c_int;
    fn micro_native_create_row(native: *mut c_void, node: c_uint, parent: c_uint) -> c_int;
    fn micro_native_create_list(native: *mut c_void, node: c_uint, parent: c_uint) -> c_int;
    fn micro_native_create_tabview(native: *mut c_void, node: c_uint, parent: c_uint, titles: *const c_char) -> c_int;
    fn micro_native_create_tab_content(native: *mut c_void, index: c_uint) -> c_int;
    fn micro_native_create_progress(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        fraction: f64,
    ) -> c_int;
    fn micro_native_create_switch(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        checked: c_int,
        handler: c_uint,
    ) -> c_int;
    fn micro_native_create_label(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        text: *const c_char,
        font_handle: usize,
        line_height_px: c_uint,
    ) -> c_int;
    fn micro_native_create_button(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        text: *const c_char,
        handler: c_uint,
        font_handle: usize,
        line_height_px: c_uint,
    ) -> c_int;
    fn micro_native_set_label_text(native: *mut c_void, node: c_uint, text: *const c_char)
    -> c_int;
    fn micro_native_set_progress_value(
        native: *mut c_void,
        node: c_uint,
        fraction: f64,
    ) -> c_int;
    fn micro_native_set_switch_checked(native: *mut c_void, node: c_uint, checked: c_int)
        -> c_int;
    fn micro_native_create_input(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        text: *const c_char,
        placeholder: *const c_char,
        handler: c_uint,
        font_handle: usize,
        line_height_px: c_uint,
    ) -> c_int;
    fn micro_native_set_input_text(native: *mut c_void, node: c_uint, text: *const c_char)
        -> c_int;
    fn micro_native_create_slider(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        value: f64,
        min: f64,
        max: f64,
        handler: c_uint,
    ) -> c_int;
    fn micro_native_set_slider_value(native: *mut c_void, node: c_uint, value: f64) -> c_int;
    fn micro_native_create_checkbox(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        label: *const c_char,
        checked: c_int,
        handler: c_uint,
    ) -> c_int;
    fn micro_native_create_dropdown(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        options: *const c_char,
        index: f64,
        handler: c_uint,
    ) -> c_int;
    fn micro_native_set_layout_spec(native: *mut c_void, node: c_uint, mask: c_uint, left: f64, top: f64, width: f64, height: f64, anchor_left: f64, anchor_top: f64, anchor_right: f64, anchor_bottom: f64) -> c_int;
    fn micro_native_apply_delphi_layout(native: *mut c_void, container: c_uint, child_ids: *const c_uint, child_count: c_uint) -> c_int;
    fn micro_native_create_led(native: *mut c_void, node: c_uint, parent: c_uint, on: c_int) -> c_int;
    fn micro_native_set_led(native: *mut c_void, node: c_uint, on: c_int) -> c_int;
    fn micro_native_create_spinner(native: *mut c_void, node: c_uint, parent: c_uint, active: c_int) -> c_int;
    fn micro_native_set_spinner(native: *mut c_void, node: c_uint, active: c_int) -> c_int;
    fn micro_native_create_scale(native: *mut c_void, node: c_uint, parent: c_uint, value: f64, min: f64, max: f64) -> c_int;
    fn micro_native_set_scale_value(native: *mut c_void, node: c_uint, value: f64) -> c_int;
    fn micro_native_create_roller(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        options: *const c_char,
        index: f64,
        handler: c_uint,
    ) -> c_int;
    fn micro_native_set_selection_value(native: *mut c_void, node: c_uint, index: f64) -> c_int;
    fn micro_native_destroy_app_root(native: *mut c_void) -> c_int;
}

/// The underlying `micro_native_t` C handle.
struct NativeInner {
    raw: NonNull<c_void>,
}

/// Owns (via `Rc`) the native SDL/LVGL environment. Cheap to clone so the shell
/// and app runtimes can share one window/display; the C handle is destroyed
/// when the last clone (including the main-loop copy) drops.
#[derive(Clone)]
pub struct NativeBridge {
    inner: Rc<NativeInner>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeFontHandle(*const c_void);

impl Default for NativeFontHandle {
    fn default() -> Self {
        Self(std::ptr::null())
    }
}

// SAFETY: Catalog handles point only to immutable, platform-owned static font assets.
unsafe impl Sync for NativeFontHandle {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeTextStyle {
    font_handle: NativeFontHandle,
    line_height_px: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeFontKey {
    family: FontFamily,
    size_px: u8,
    weight: FontWeight,
}

unsafe extern "C" {
    static micro_ui_sans_12: c_void;
    static micro_ui_sans_14: c_void;
    static micro_ui_sans_18: c_void;
    static micro_ui_sans_24: c_void;
    static micro_ui_sans_32: c_void;
}

const AVAILABLE_NATIVE_FONTS: &[(NativeFontKey, NativeFontHandle)] = &[
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 12,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_12),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 14,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_14),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 18,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_18),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 24,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_24),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 32,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_32),
    ),
];

fn select_native_text_style(
    style: Option<&TextStyle>,
    available_fonts: &[(NativeFontKey, NativeFontHandle)],
) -> Result<NativeTextStyle, String> {
    let Some(style) = style else {
        return Err("native text style must be normalized before rendering".into());
    };
    let key = NativeFontKey {
        family: style.family,
        size_px: style.size_px,
        weight: style.weight,
    };
    available_fonts
        .iter()
        .find_map(|(candidate, font_handle)| {
            (*candidate == key).then_some(NativeTextStyle {
                font_handle: *font_handle,
                line_height_px: u32::from(style.line_height_px),
            })
        })
        .ok_or_else(|| {
            format!(
                "native font unavailable: {:?} {}px {:?}",
                style.family, style.size_px, style.weight
            )
        })
}

fn call_with_native_text_style(
    style: Option<&TextStyle>,
    available_fonts: &[(NativeFontKey, NativeFontHandle)],
    operation: &str,
    create: impl FnOnce(NativeTextStyle) -> c_int,
) -> Result<(), String> {
    let selected = select_native_text_style(style, available_fonts)?;
    native_result(create(selected), operation)
}

impl NativeBridge {
    pub fn create(width: u32, height: u32, hidden: bool) -> Result<Self, String> {
        let width = c_int::try_from(width).map_err(|_| "width is too large".to_owned())?;
        let height = c_int::try_from(height).map_err(|_| "height is too large".to_owned())?;
        let mut error = [0_i8; 512];
        let raw = unsafe {
            micro_native_create(
                width,
                height,
                c_int::from(hidden),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        let raw = NonNull::new(raw).ok_or_else(|| {
            let length = error
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(error.len());
            let bytes: Vec<u8> = error[..length].iter().map(|byte| *byte as u8).collect();
            String::from_utf8_lossy(&bytes).into_owned()
        })?;
        Ok(Self {
            inner: Rc::new(NativeInner { raw }),
        })
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.inner.raw.as_ptr()
    }

    pub fn poll(&mut self) -> bool {
        unsafe { micro_native_poll(self.inner.raw.as_ptr()) != 0 }
    }

    pub fn timer(&mut self) -> u32 {
        unsafe { micro_native_timer(self.inner.raw.as_ptr()) }
    }

    pub fn take_activation(&mut self) -> Option<FunctionId> {
        let mut id = 0;
        (unsafe { micro_native_take_activation(self.inner.raw.as_ptr(), &mut id) } != 0)
            .then_some(FunctionId(id))
    }

    pub fn inject_activation(&mut self, id: FunctionId) {
        unsafe { micro_native_inject_activation(self.inner.raw.as_ptr(), id.0) };
    }

    pub fn queue_click(&mut self, node: NodeId) -> Result<(), String> {
        native_result(
            unsafe { micro_native_queue_click(self.inner.raw.as_ptr(), node.0) },
            "queue click",
        )
    }
}

/// Frees the native SDL/LVGL environment when the last `NativeBridge` clone
/// drops (Rc calls the inner's `Drop` exactly once, at strong count zero).
impl Drop for NativeInner {
    fn drop(&mut self) {
        unsafe { micro_native_destroy(self.raw.as_ptr()) };
    }
}

impl NativeUi for NativeBridge {
    fn report_diagnostic(&mut self, node: NodeId, message: &str) {
        eprintln!("micro-ui node {}: {message}", node.0);
    }

    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_column(self.inner.raw.as_ptr(), node.0, parent_id(parent)) },
            "create column",
        )
    }

    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_row(self.inner.raw.as_ptr(), node.0, parent_id(parent)) },
            "create row",
        )
    }

    fn create_list(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_list(self.inner.raw.as_ptr(), node.0, parent_id(parent)) },
            "create list",
        )
    }

    fn create_tabview(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        titles: &[String],
    ) -> Result<(), String> {
        let joined = c_string(&titles.join("\n"))?;
        native_result(
            unsafe {
                micro_native_create_tabview(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    joined.as_ptr(),
                )
            },
            "create tabview",
        )
    }

    fn create_tab_content(&mut self, index: u32) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_tab_content(self.inner.raw.as_ptr(), index) },
            "create tab content",
        )
    }

    fn create_progress(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        fraction: f64,
    ) -> Result<(), String> {
        native_result(
            unsafe {
                micro_native_create_progress(self.inner.raw.as_ptr(), node.0, parent_id(parent), fraction)
            },
            "create progress",
        )
    }

    fn create_switch(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        native_result(
            unsafe {
                micro_native_create_switch(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    c_int::from(checked),
                    handler.map_or(u32::MAX, |id| id.0),
                )
            },
            "create switch",
        )
    }

    fn set_progress_value(&mut self, node: NodeId, fraction: f64) -> Result<(), String> {
        native_result(
            unsafe { micro_native_set_progress_value(self.inner.raw.as_ptr(), node.0, fraction) },
            "set progress value",
        )
    }

    fn set_switch_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String> {
        native_result(
            unsafe {
                micro_native_set_switch_checked(self.inner.raw.as_ptr(), node.0, c_int::from(checked))
            },
            "set switch checked",
        )
    }

    fn create_label(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let text = c_string(text)?;
        call_with_native_text_style(
            style,
            AVAILABLE_NATIVE_FONTS,
            "create label",
            |selected| unsafe {
                micro_native_create_label(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                    selected.font_handle.0 as usize,
                    selected.line_height_px,
                )
            },
        )
    }

    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let text = c_string(text)?;
        call_with_native_text_style(
            style,
            AVAILABLE_NATIVE_FONTS,
            "create button",
            |selected| unsafe {
                micro_native_create_button(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                    handler.0,
                    selected.font_handle.0 as usize,
                    selected.line_height_px,
                )
            },
        )
    }

    fn set_label_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        let text = c_string(text)?;
        native_result(
            unsafe { micro_native_set_label_text(self.inner.raw.as_ptr(), node.0, text.as_ptr()) },
            "set label text",
        )
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
        let text = c_string(text)?;
        let placeholder = c_string(placeholder)?;
        call_with_native_text_style(
            style,
            AVAILABLE_NATIVE_FONTS,
            "create input",
            |selected| unsafe {
                micro_native_create_input(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                    placeholder.as_ptr(),
                    handler.map_or(u32::MAX, |id| id.0),
                    selected.font_handle.0 as usize,
                    selected.line_height_px,
                )
            },
        )
    }

    fn set_input_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        let text = c_string(text)?;
        native_result(
            unsafe { micro_native_set_input_text(self.inner.raw.as_ptr(), node.0, text.as_ptr()) },
            "set input text",
        )
    }

    fn create_slider(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let (min, max) = range.unwrap_or((0.0, 100.0));
        native_result(
            unsafe {
                micro_native_create_slider(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    value,
                    min,
                    max,
                    handler.map_or(u32::MAX, |id| id.0),
                )
            },
            "create slider",
        )
    }

    fn set_slider_value(&mut self, node: NodeId, value: f64) -> Result<(), String> {
        native_result(
            unsafe { micro_native_set_slider_value(self.inner.raw.as_ptr(), node.0, value) },
            "set slider value",
        )
    }

    fn create_checkbox(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        label: &str,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let label = c_string(label)?;
        native_result(
            unsafe {
                micro_native_create_checkbox(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    label.as_ptr(),
                    c_int::from(checked),
                    handler.map_or(u32::MAX, |id| id.0),
                )
            },
            "create checkbox",
        )
    }

    fn create_dropdown(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let joined = c_string(&options.join("\n"))?;
        native_result(
            unsafe {
                micro_native_create_dropdown(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    joined.as_ptr(),
                    index,
                    handler.map_or(u32::MAX, |id| id.0),
                )
            },
            "create dropdown",
        )
    }

    fn set_layout_spec(&mut self, node: NodeId, layout: micro_ir::LayoutSpec) -> Result<(), String> {
        let mask = layout.left.map_or(0, |_| 1)
            | layout.top.map_or(0, |_| 2)
            | layout.width.map_or(0, |_| 4)
            | layout.height.map_or(0, |_| 8)
            | layout.anchor.left.map_or(0, |_| 16)
            | layout.anchor.top.map_or(0, |_| 32)
            | layout.anchor.right.map_or(0, |_| 64)
            | layout.anchor.bottom.map_or(0, |_| 128);
        native_result(
            unsafe {
                micro_native_set_layout_spec(
                    self.inner.raw.as_ptr(),
                    node.0,
                    mask,
                    layout.left.unwrap_or(0.0),
                    layout.top.unwrap_or(0.0),
                    layout.width.unwrap_or(0.0),
                    layout.height.unwrap_or(0.0),
                    layout.anchor.left.unwrap_or(0.0),
                    layout.anchor.top.unwrap_or(0.0),
                    layout.anchor.right.unwrap_or(0.0),
                    layout.anchor.bottom.unwrap_or(0.0),
                )
            },
            "set layout spec",
        )
    }

    fn apply_delphi_layout(&mut self, container: NodeId, children: &[NodeId]) -> Result<(), String> {
        let ids: Vec<c_uint> = children.iter().map(|c| c.0).collect();
        native_result(
            unsafe {
                micro_native_apply_delphi_layout(
                    self.inner.raw.as_ptr(),
                    container.0,
                    ids.as_ptr(),
                    ids.len() as c_uint,
                )
            },
            "apply delphi layout",
        )
    }

    fn create_led(&mut self, node: NodeId, parent: Option<NodeId>, on: bool) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_led(self.inner.raw.as_ptr(), node.0, parent_id(parent), c_int::from(on)) },
            "create led",
        )
    }

    fn set_led(&mut self, node: NodeId, on: bool) -> Result<(), String> {
        native_result(unsafe { micro_native_set_led(self.inner.raw.as_ptr(), node.0, c_int::from(on)) }, "set led")
    }

    fn create_spinner(&mut self, node: NodeId, parent: Option<NodeId>, active: bool) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_spinner(self.inner.raw.as_ptr(), node.0, parent_id(parent), c_int::from(active)) },
            "create spinner",
        )
    }

    fn set_spinner(&mut self, node: NodeId, active: bool) -> Result<(), String> {
        native_result(unsafe { micro_native_set_spinner(self.inner.raw.as_ptr(), node.0, c_int::from(active)) }, "set spinner")
    }

    fn create_scale(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
    ) -> Result<(), String> {
        let (min, max) = range.unwrap_or((0.0, 100.0));
        native_result(
            unsafe { micro_native_create_scale(self.inner.raw.as_ptr(), node.0, parent_id(parent), value, min, max) },
            "create scale",
        )
    }

    fn set_scale_value(&mut self, node: NodeId, value: f64) -> Result<(), String> {
        native_result(unsafe { micro_native_set_scale_value(self.inner.raw.as_ptr(), node.0, value) }, "set scale value")
    }

    fn create_roller(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let joined = c_string(&options.join("\n"))?;
        native_result(
            unsafe {
                micro_native_create_roller(
                    self.inner.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    joined.as_ptr(),
                    index,
                    handler.map_or(u32::MAX, |id| id.0),
                )
            },
            "create roller",
        )
    }

    fn set_selection_value(&mut self, node: NodeId, index: f64) -> Result<(), String> {
        native_result(
            unsafe { micro_native_set_selection_value(self.inner.raw.as_ptr(), node.0, index) },
            "set selection value",
        )
    }

    fn destroy_app_root(&mut self) -> Result<(), String> {
        native_result(
            unsafe { micro_native_destroy_app_root(self.inner.raw.as_ptr()) },
            "destroy app root",
        )
    }
}

fn parent_id(parent: Option<NodeId>) -> u32 {
    parent.map_or(u32::MAX, |id| id.0)
}

fn c_string(text: &str) -> Result<CString, String> {
    CString::new(text).map_err(|_| "text contains a NUL byte".into())
}

fn native_result(result: c_int, operation: &str) -> Result<(), String> {
    if result != 0 {
        Ok(())
    } else {
        Err(format!("native operation failed: {operation}"))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use micro_ir::{FontFamily, FontWeight, TextStyle};

    use super::{
        AVAILABLE_NATIVE_FONTS, NativeFontHandle, NativeFontKey, NativeTextStyle,
        call_with_native_text_style, select_native_text_style,
    };

    #[test]
    fn rejects_current_lvgl_default_when_style_is_unset() {
        assert_eq!(
            select_native_text_style(None, AVAILABLE_NATIVE_FONTS),
            Err("native text style must be normalized before rendering".into())
        );
    }

    #[test]
    fn exposes_every_generated_regular_font() {
        for (size, line_height) in TextStyle::UI_SANS_METRICS {
            let style = TextStyle::ui_sans(size, FontWeight::Regular, line_height).unwrap();
            let selected = select_native_text_style(Some(&style), AVAILABLE_NATIVE_FONTS).unwrap();
            assert!(!selected.font_handle.0.is_null());
            assert_eq!(selected.line_height_px, u32::from(line_height));
        }
    }

    #[test]
    fn forwards_selected_font_handle_and_line_height_to_native_call() {
        static TEST_FONT: u8 = 0;
        const TEST_FONTS: [(NativeFontKey, NativeFontHandle); 1] = [(
            NativeFontKey {
                family: FontFamily::UiSans,
                size_px: 18,
                weight: FontWeight::Regular,
            },
            NativeFontHandle(&raw const TEST_FONT as *const c_void),
        )];
        let style = TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap();
        let mut applied = None;
        call_with_native_text_style(Some(&style), &TEST_FONTS, "create label", |selected| {
            applied = Some(selected);
            1
        })
        .unwrap();
        assert_eq!(
            applied,
            Some(NativeTextStyle {
                font_handle: NativeFontHandle(&raw const TEST_FONT as *const c_void),
                line_height_px: 24,
            })
        );
    }
}
