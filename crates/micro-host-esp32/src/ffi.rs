//! ESP-IDF uses `panic=abort`; no Rust panic may be used as an error path here.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;

use micro_ir::{FunctionId, NodeId, TextStyle};
use micro_lvgl::NativeUi;

use crate::native_text_style::{AVAILABLE_NATIVE_FONTS, call_with_native_text_style};
use crate::{HostError, MicroAction, MicroErrorCode, MicroEvent, MicroState, OsHost, RuntimeHost};

unsafe extern "C" {
    fn micro_esp_ui_create_column(node: u32, parent: u32) -> c_int;
    fn micro_esp_ui_create_label(
        node: u32,
        parent: u32,
        text: *const u8,
        len: usize,
        font_handle: usize,
        line_height_px: u32,
    ) -> c_int;
    fn micro_esp_ui_create_button(
        node: u32,
        parent: u32,
        text: *const u8,
        len: usize,
        handler: u32,
        font_handle: usize,
        line_height_px: u32,
    ) -> c_int;
    fn micro_esp_ui_set_label_text(node: u32, text: *const u8, len: usize) -> c_int;
    fn micro_esp_ui_destroy_app_root() -> c_int;
}

struct EspNativeUi;

impl NativeUi for EspNativeUi {
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_create_column(node.0, parent_id(parent)) })
    }

    fn create_label(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let result =
            call_with_native_text_style(style, AVAILABLE_NATIVE_FONTS, |selected| unsafe {
                micro_esp_ui_create_label(
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                    text.len(),
                    selected.font_handle.0 as usize,
                    selected.line_height_px,
                )
            })?;
        native_result(result)
    }

    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let result =
            call_with_native_text_style(style, AVAILABLE_NATIVE_FONTS, |selected| unsafe {
                micro_esp_ui_create_button(
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                    text.len(),
                    handler.0,
                    selected.font_handle.0 as usize,
                    selected.line_height_px,
                )
            })?;
        native_result(result)
    }

    fn set_label_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_label_text(node.0, text.as_ptr(), text.len()) })
    }

    fn destroy_app_root(&mut self) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_destroy_app_root() })
    }
}

fn parent_id(parent: Option<NodeId>) -> u32 {
    parent.map_or(u32::MAX, |node| node.0)
}

fn native_result(result: c_int) -> Result<(), String> {
    if result == 0 {
        Ok(())
    } else {
        Err(format!("ESP UI bridge failed with code {result}"))
    }
}

fn write_diagnostic(buffer: *mut c_char, length: usize, diagnostic: &str) {
    if buffer.is_null() || length == 0 || crate::validate_region_length(length, 1).is_err() {
        return;
    }
    let output = unsafe { std::slice::from_raw_parts_mut(buffer.cast::<u8>(), length) };
    crate::write_diagnostic(output, diagnostic);
}

fn report(error: HostError, buffer: *mut c_char, length: usize) -> c_int {
    write_diagnostic(buffer, length, &error.to_string());
    error.code() as c_int
}

fn valid_optional_byte_region(pointer: *mut c_char, length: usize) -> bool {
    (length == 0 || !pointer.is_null()) && crate::validate_region_length(length, 1).is_ok()
}

#[repr(C)]
pub(crate) struct RawMicroEvent {
    kind: u32,
    result: u32,
    failure: u32,
    wifi_failure: u32,
    app: u32,
    flag: u32,
    after_secs: u32,
    reserved: u32,
    session_id: u64,
    operation_id: u64,
    confirmation_id: u64,
}

impl RawMicroEvent {
    fn validate(&self) -> Result<MicroEvent, MicroErrorCode> {
        MicroEvent::from_raw(
            self.kind,
            self.result,
            self.failure,
            self.wifi_failure,
            self.app,
            self.flag,
            self.after_secs,
            self.reserved,
            self.session_id,
            self.operation_id,
            self.confirmation_id,
        )
    }
}

#[repr(C)]
pub(crate) struct RawActionBuffer {
    actions: *mut RawMicroAction,
    capacity: usize,
    count: usize,
    required: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawMicroAction {
    kind: u32,
    child_count: u32,
    failure: u32,
    app: u32,
    after_secs: u32,
    backlight: u32,
    reserved_1: u32,
    reserved_2: u32,
    session_id: u64,
    operation_id: u64,
    confirmation_id: u64,
}

impl From<MicroAction> for RawMicroAction {
    fn from(action: MicroAction) -> Self {
        Self {
            kind: action.kind as u32,
            child_count: action.child_count,
            failure: action.failure as u32,
            app: action.app as u32,
            after_secs: action.after_secs,
            backlight: action.backlight as u32,
            reserved_1: action.reserved_1,
            reserved_2: action.reserved_2,
            session_id: action.session_id,
            operation_id: action.operation_id,
            confirmation_id: action.confirmation_id,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_create(
    mbc: *const u8,
    length: usize,
    budget: u64,
    error: *mut c_char,
    error_length: usize,
) -> *mut c_void {
    if !valid_optional_byte_region(error, error_length) {
        return ptr::null_mut();
    }
    if mbc.is_null()
        || length == 0
        || budget == 0
        || crate::validate_region_length(length, 1).is_err()
    {
        write_diagnostic(error, error_length, "invalid runtime arguments");
        return ptr::null_mut();
    }
    let mut owned = Vec::new();
    if owned.try_reserve_exact(length).is_err() {
        write_diagnostic(error, error_length, "unable to reserve MBC copy");
        return ptr::null_mut();
    }
    owned.extend_from_slice(unsafe { std::slice::from_raw_parts(mbc, length) });
    match RuntimeHost::from_owned_mbc(owned, EspNativeUi, budget) {
        Ok(runtime) => Box::into_raw(Box::new(runtime)).cast(),
        Err(runtime_error) => {
            write_diagnostic(error, error_length, &runtime_error.to_string());
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_activate(runtime: *mut c_void, handler_id: u32) -> c_int {
    if runtime.is_null() {
        return MicroErrorCode::InvalidArgument as c_int;
    }
    match unsafe { &mut *runtime.cast::<RuntimeHost<EspNativeUi>>() }
        .activate(FunctionId(handler_id))
    {
        Ok(()) => MicroErrorCode::Ok as c_int,
        Err(error) => error.code() as c_int,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_tick(
    runtime: *mut c_void,
    error: *mut c_char,
    error_length: usize,
) -> c_int {
    if !valid_optional_byte_region(error, error_length) {
        return MicroErrorCode::InvalidArgument as c_int;
    }
    if runtime.is_null() {
        write_diagnostic(error, error_length, "runtime handle is null");
        return MicroErrorCode::InvalidArgument as c_int;
    }
    match unsafe { (&mut *runtime.cast::<RuntimeHost<EspNativeUi>>()).tick() } {
        Ok(_) => MicroErrorCode::Ok as c_int,
        Err(runtime_error) => report(runtime_error, error, error_length),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_destroy(runtime: *mut c_void) {
    if runtime.is_null() {
        return;
    }
    let mut runtime = unsafe { Box::from_raw(runtime.cast::<RuntimeHost<EspNativeUi>>()) };
    let _ = runtime.stop();
}

#[unsafe(no_mangle)]
pub extern "C" fn micro_os_create() -> *mut c_void {
    Box::into_raw(Box::new(OsHost::new())).cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_os_dispatch(
    os: *mut c_void,
    event: *const RawMicroEvent,
    action_buffer: *mut RawActionBuffer,
    error: *mut c_char,
    error_length: usize,
) -> c_int {
    if !valid_optional_byte_region(error, error_length) {
        return MicroErrorCode::InvalidArgument as c_int;
    }
    if os.is_null() || event.is_null() || action_buffer.is_null() {
        write_diagnostic(error, error_length, "invalid OS dispatch arguments");
        return MicroErrorCode::InvalidArgument as c_int;
    }
    let buffer = unsafe { &mut *action_buffer };
    buffer.count = 0;
    buffer.required = 0;
    if buffer.capacity != 0 && buffer.actions.is_null() {
        write_diagnostic(error, error_length, "action storage is null");
        return MicroErrorCode::InvalidArgument as c_int;
    }
    if crate::validate_region_length(buffer.capacity, std::mem::size_of::<RawMicroAction>())
        .is_err()
    {
        write_diagnostic(
            error,
            error_length,
            "action capacity exceeds addressable range",
        );
        return MicroErrorCode::InvalidArgument as c_int;
    }
    let event = match unsafe { &*event }.validate() {
        Ok(event) => event,
        Err(code) => {
            write_diagnostic(error, error_length, "event payload is invalid");
            return code as c_int;
        }
    };
    let mut encoded = Vec::new();
    if encoded.try_reserve_exact(buffer.capacity).is_err() {
        write_diagnostic(
            error,
            error_length,
            "unable to reserve action staging buffer",
        );
        return MicroErrorCode::Runtime as c_int;
    }
    encoded.resize(
        buffer.capacity,
        MicroAction::new(crate::MicroActionKind::Rejected),
    );
    match unsafe { &mut *os.cast::<OsHost>() }.dispatch_into(event, &mut encoded) {
        Ok(count) => {
            for (index, action) in encoded.iter().copied().take(count).enumerate() {
                unsafe { buffer.actions.add(index).write(action.into()) };
            }
            buffer.count = count;
            buffer.required = count;
            MicroErrorCode::Ok as c_int
        }
        Err(dispatch_error) => {
            buffer.required = dispatch_error.required;
            write_diagnostic(error, error_length, "action buffer is too small");
            dispatch_error.code as c_int
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_os_state(os: *const c_void) -> MicroState {
    if os.is_null() {
        return MicroState::SafeMode;
    }
    unsafe { (&*os.cast::<OsHost>()).state() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_os_destroy(os: *mut c_void) {
    if !os.is_null() {
        drop(unsafe { Box::from_raw(os.cast::<OsHost>()) });
    }
}
