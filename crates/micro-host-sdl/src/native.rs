use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::ptr::NonNull;

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
    fn micro_native_destroy_app_root(native: *mut c_void) -> c_int;
}

pub struct NativeBridge {
    raw: NonNull<c_void>,
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
        Ok(Self { raw })
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.raw.as_ptr()
    }

    pub fn poll(&mut self) -> bool {
        unsafe { micro_native_poll(self.raw.as_ptr()) != 0 }
    }

    pub fn timer(&mut self) -> u32 {
        unsafe { micro_native_timer(self.raw.as_ptr()) }
    }

    pub fn take_activation(&mut self) -> Option<FunctionId> {
        let mut id = 0;
        (unsafe { micro_native_take_activation(self.raw.as_ptr(), &mut id) } != 0)
            .then_some(FunctionId(id))
    }

    pub fn inject_activation(&mut self, id: FunctionId) {
        unsafe { micro_native_inject_activation(self.raw.as_ptr(), id.0) };
    }

    pub fn queue_click(&mut self, node: NodeId) -> Result<(), String> {
        native_result(
            unsafe { micro_native_queue_click(self.raw.as_ptr(), node.0) },
            "queue click",
        )
    }
}

impl Drop for NativeBridge {
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
            unsafe { micro_native_create_column(self.raw.as_ptr(), node.0, parent_id(parent)) },
            "create column",
        )
    }

    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_row(self.raw.as_ptr(), node.0, parent_id(parent)) },
            "create row",
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
                micro_native_create_progress(self.raw.as_ptr(), node.0, parent_id(parent), fraction)
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
                    self.raw.as_ptr(),
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
            unsafe { micro_native_set_progress_value(self.raw.as_ptr(), node.0, fraction) },
            "set progress value",
        )
    }

    fn set_switch_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String> {
        native_result(
            unsafe {
                micro_native_set_switch_checked(self.raw.as_ptr(), node.0, c_int::from(checked))
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
                    self.raw.as_ptr(),
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
                    self.raw.as_ptr(),
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
            unsafe { micro_native_set_label_text(self.raw.as_ptr(), node.0, text.as_ptr()) },
            "set label text",
        )
    }

    fn destroy_app_root(&mut self) -> Result<(), String> {
        native_result(
            unsafe { micro_native_destroy_app_root(self.raw.as_ptr()) },
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
