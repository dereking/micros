use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::ptr::NonNull;

use micro_ir::{FunctionId, NodeId};
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
    fn micro_native_create_label(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        text: *const c_char,
    ) -> c_int;
    fn micro_native_create_button(
        native: *mut c_void,
        node: c_uint,
        parent: c_uint,
        text: *const c_char,
        handler: c_uint,
    ) -> c_int;
    fn micro_native_set_label_text(native: *mut c_void, node: c_uint, text: *const c_char)
    -> c_int;
    fn micro_native_destroy_app_root(native: *mut c_void) -> c_int;
}

pub struct NativeBridge {
    raw: NonNull<c_void>,
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
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(
            unsafe { micro_native_create_column(self.raw.as_ptr(), node.0, parent_id(parent)) },
            "create column",
        )
    }

    fn create_label(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
    ) -> Result<(), String> {
        let text = c_string(text)?;
        native_result(
            unsafe {
                micro_native_create_label(
                    self.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                )
            },
            "create label",
        )
    }

    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
    ) -> Result<(), String> {
        let text = c_string(text)?;
        native_result(
            unsafe {
                micro_native_create_button(
                    self.raw.as_ptr(),
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                    handler.0,
                )
            },
            "create button",
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
