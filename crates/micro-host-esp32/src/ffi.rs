//! ESP-IDF uses `panic=abort`; no Rust panic may be used as an error path here.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;

use micro_ir::{FunctionId, NodeId, TextStyle};
use micro_lvgl::NativeUi;

use crate::native_text_style::{AVAILABLE_NATIVE_FONTS, call_with_native_text_style};
use crate::{HostError, MicroAction, MicroErrorCode, MicroEvent, MicroState, OsHost, RuntimeHost};

unsafe extern "C" {
    fn micro_esp_ui_create_column(node: u32, parent: u32) -> c_int;
    fn micro_esp_ui_create_row(node: u32, parent: u32) -> c_int;
    fn micro_esp_ui_create_list(node: u32, parent: u32) -> c_int;
    fn micro_esp_ui_create_progress(node: u32, parent: u32, fraction: f64) -> c_int;
    fn micro_esp_ui_create_switch(node: u32, parent: u32, checked: c_int, handler: u32)
        -> c_int;
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
    fn micro_esp_ui_set_progress_value(node: u32, fraction: f64) -> c_int;
    fn micro_esp_ui_set_switch_checked(node: u32, checked: c_int) -> c_int;
    fn micro_esp_ui_create_input(
        node: u32,
        parent: u32,
        text: *const u8,
        len: usize,
        placeholder: *const u8,
        placeholder_len: usize,
        handler: u32,
        font_handle: usize,
        line_height_px: u32,
    ) -> c_int;
    fn micro_esp_ui_set_input_text(node: u32, text: *const u8, len: usize) -> c_int;
    fn micro_esp_ui_create_slider(
        node: u32,
        parent: u32,
        value: f64,
        min: f64,
        max: f64,
        handler: u32,
    ) -> c_int;
    fn micro_esp_ui_set_slider_value(node: u32, value: f64) -> c_int;
    fn micro_esp_ui_create_checkbox(
        node: u32,
        parent: u32,
        label: *const u8,
        label_len: usize,
        checked: c_int,
        handler: u32,
    ) -> c_int;
    fn micro_esp_ui_create_dropdown(
        node: u32,
        parent: u32,
        options: *const u8,
        options_len: usize,
        index: f64,
        handler: u32,
    ) -> c_int;
    fn micro_esp_ui_create_led(node: u32, parent: u32, on: c_int) -> c_int;
    fn micro_esp_ui_set_led(node: u32, on: c_int) -> c_int;
    fn micro_esp_ui_create_spinner(node: u32, parent: u32, active: c_int) -> c_int;
    fn micro_esp_ui_set_spinner(node: u32, active: c_int) -> c_int;
    fn micro_esp_ui_create_scale(node: u32, parent: u32, value: f64, min: f64, max: f64) -> c_int;
    fn micro_esp_ui_set_scale_value(node: u32, value: f64) -> c_int;
    fn micro_esp_ui_create_roller(
        node: u32,
        parent: u32,
        options: *const u8,
        options_len: usize,
        index: f64,
        handler: u32,
    ) -> c_int;
    fn micro_esp_ui_set_selection_value(node: u32, index: f64) -> c_int;
    fn micro_esp_ui_destroy_app_root() -> c_int;
    fn micro_esp_ui_take_activation(handler_id: *mut u32) -> c_int;
    fn micro_esp_ui_take_input_change(
        handler_id: *mut u32,
        text: *mut u8,
        text_capacity: usize,
        text_len: *mut usize,
    ) -> c_int;
    fn micro_esp_ui_take_slider_change(handler_id: *mut u32, value: *mut f64) -> c_int;
    fn micro_esp_ui_take_checkbox_change(handler_id: *mut u32, checked: *mut c_int) -> c_int;
    fn micro_esp_ui_take_dropdown_change(handler_id: *mut u32, index: *mut f64) -> c_int;
    fn micro_esp_ui_take_roller_change(handler_id: *mut u32, index: *mut f64) -> c_int;
    fn micro_esp_ui_report_diagnostic(node: u32, message: *const u8, len: usize);
}

struct EspNativeUi;

impl NativeUi for EspNativeUi {
    fn report_diagnostic(&mut self, node: NodeId, message: &str) {
        unsafe { micro_esp_ui_report_diagnostic(node.0, message.as_ptr(), message.len()) };
    }

    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_create_column(node.0, parent_id(parent)) })
    }

    fn create_row(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_create_row(node.0, parent_id(parent)) })
    }

    fn create_list(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_create_list(node.0, parent_id(parent)) })
    }

    fn create_progress(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        fraction: f64,
    ) -> Result<(), String> {
        native_result(unsafe {
            micro_esp_ui_create_progress(node.0, parent_id(parent), fraction)
        })
    }

    fn create_switch(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        native_result(unsafe {
            micro_esp_ui_create_switch(
                node.0,
                parent_id(parent),
                c_int::from(checked),
                handler.map_or(u32::MAX, |id| id.0),
            )
        })
    }

    fn set_progress_value(&mut self, node: NodeId, fraction: f64) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_progress_value(node.0, fraction) })
    }

    fn set_switch_checked(&mut self, node: NodeId, checked: bool) -> Result<(), String> {
        native_result(unsafe {
            micro_esp_ui_set_switch_checked(node.0, c_int::from(checked))
        })
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

    fn create_input(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        placeholder: &str,
        handler: Option<FunctionId>,
        style: Option<&TextStyle>,
    ) -> Result<(), String> {
        let result =
            call_with_native_text_style(style, AVAILABLE_NATIVE_FONTS, |selected| unsafe {
                micro_esp_ui_create_input(
                    node.0,
                    parent_id(parent),
                    text.as_ptr(),
                    text.len(),
                    placeholder.as_ptr(),
                    placeholder.len(),
                    handler.map_or(u32::MAX, |id| id.0),
                    selected.font_handle.0 as usize,
                    selected.line_height_px,
                )
            })?;
        native_result(result)
    }

    fn set_input_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_input_text(node.0, text.as_ptr(), text.len()) })
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
        native_result(unsafe {
            micro_esp_ui_create_slider(
                node.0,
                parent_id(parent),
                value,
                min,
                max,
                handler.map_or(u32::MAX, |id| id.0),
            )
        })
    }

    fn set_slider_value(&mut self, node: NodeId, value: f64) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_slider_value(node.0, value) })
    }

    fn create_checkbox(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        label: &str,
        checked: bool,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        native_result(unsafe {
            micro_esp_ui_create_checkbox(
                node.0,
                parent_id(parent),
                label.as_ptr(),
                label.len(),
                c_int::from(checked),
                handler.map_or(u32::MAX, |id| id.0),
            )
        })
    }

    fn create_dropdown(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let joined = options.join("\n");
        native_result(unsafe {
            micro_esp_ui_create_dropdown(
                node.0,
                parent_id(parent),
                joined.as_ptr(),
                joined.len(),
                index,
                handler.map_or(u32::MAX, |id| id.0),
            )
        })
    }

    fn create_led(&mut self, node: NodeId, parent: Option<NodeId>, on: bool) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_create_led(node.0, parent_id(parent), c_int::from(on)) })
    }

    fn set_led(&mut self, node: NodeId, on: bool) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_led(node.0, c_int::from(on)) })
    }

    fn create_spinner(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        active: bool,
    ) -> Result<(), String> {
        native_result(unsafe {
            micro_esp_ui_create_spinner(node.0, parent_id(parent), c_int::from(active))
        })
    }

    fn set_spinner(&mut self, node: NodeId, active: bool) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_spinner(node.0, c_int::from(active)) })
    }

    fn create_scale(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        value: f64,
        range: Option<(f64, f64)>,
    ) -> Result<(), String> {
        let (min, max) = range.unwrap_or((0.0, 100.0));
        native_result(unsafe {
            micro_esp_ui_create_scale(node.0, parent_id(parent), value, min, max)
        })
    }

    fn set_scale_value(&mut self, node: NodeId, value: f64) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_scale_value(node.0, value) })
    }

    fn create_roller(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        options: &[String],
        index: f64,
        handler: Option<FunctionId>,
    ) -> Result<(), String> {
        let joined = options.join("\n");
        native_result(unsafe {
            micro_esp_ui_create_roller(
                node.0,
                parent_id(parent),
                joined.as_ptr(),
                joined.len(),
                index,
                handler.map_or(u32::MAX, |id| id.0),
            )
        })
    }

    fn set_selection_value(&mut self, node: NodeId, index: f64) -> Result<(), String> {
        native_result(unsafe { micro_esp_ui_set_selection_value(node.0, index) })
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
    let runtime = unsafe { &mut *runtime.cast::<RuntimeHost<EspNativeUi>>() };
    loop {
        let mut handler = 0;
        match unsafe { micro_esp_ui_take_activation(&raw mut handler) } {
            1 => {
                if let Err(runtime_error) = runtime.activate(FunctionId(handler)) {
                    return report(runtime_error, error, error_length);
                }
            }
            0 => break,
            code => {
                return report(
                    HostError {
                        code: MicroErrorCode::Ui,
                        diagnostic: format!("ESP activation queue failed with code {code}"),
                    },
                    error,
                    error_length,
                );
            }
        }
    }
    loop {
        let mut handler = 0;
        let mut text = [0u8; 256];
        let mut text_len = 0usize;
        match unsafe {
            micro_esp_ui_take_input_change(
                &raw mut handler,
                text.as_mut_ptr(),
                text.len(),
                &raw mut text_len,
            )
        } {
            1 => {
                if let Ok(text) = std::str::from_utf8(&text[..text_len]) {
                    if let Err(runtime_error) =
                        runtime.set_input_text(FunctionId(handler), text.to_owned())
                    {
                        return report(runtime_error, error, error_length);
                    }
                } else {
                    write_diagnostic(error, error_length, "input text is not valid UTF-8");
                    return MicroErrorCode::Ui as c_int;
                }
            }
            0 => break,
            code => {
                return report(
                    HostError {
                        code: MicroErrorCode::Ui,
                        diagnostic: format!("ESP input-change queue failed with code {code}"),
                    },
                    error,
                    error_length,
                );
            }
        }
    }
    loop {
        let mut handler = 0;
        let mut value = 0.0_f64;
        match unsafe { micro_esp_ui_take_slider_change(&raw mut handler, &raw mut value) } {
            1 => {
                if let Err(runtime_error) =
                    runtime.set_slider_value(FunctionId(handler), value)
                {
                    return report(runtime_error, error, error_length);
                }
            }
            0 => break,
            code => {
                return report(
                    HostError {
                        code: MicroErrorCode::Ui,
                        diagnostic: format!("ESP slider-change queue failed with code {code}"),
                    },
                    error,
                    error_length,
                );
            }
        }
    }
    loop {
        let mut handler = 0;
        let mut checked = 0;
        match unsafe { micro_esp_ui_take_checkbox_change(&raw mut handler, &raw mut checked) } {
            1 => {
                if let Err(runtime_error) =
                    runtime.set_checkbox_checked(FunctionId(handler), checked != 0)
                {
                    return report(runtime_error, error, error_length);
                }
            }
            0 => break,
            code => {
                return report(
                    HostError {
                        code: MicroErrorCode::Ui,
                        diagnostic: format!("ESP checkbox-change queue failed with code {code}"),
                    },
                    error,
                    error_length,
                );
            }
        }
    }
    loop {
        let mut handler = 0;
        let mut index = 0.0_f64;
        match unsafe { micro_esp_ui_take_dropdown_change(&raw mut handler, &raw mut index) } {
            1 => {
                if let Err(runtime_error) =
                    runtime.set_selection(FunctionId(handler), index)
                {
                    return report(runtime_error, error, error_length);
                }
            }
            0 => break,
            code => {
                return report(
                    HostError {
                        code: MicroErrorCode::Ui,
                        diagnostic: format!("ESP dropdown-change queue failed with code {code}"),
                    },
                    error,
                    error_length,
                );
            }
        }
    }
    loop {
        let mut handler = 0;
        let mut index = 0.0_f64;
        match unsafe { micro_esp_ui_take_roller_change(&raw mut handler, &raw mut index) } {
            1 => {
                if let Err(runtime_error) =
                    runtime.set_selection(FunctionId(handler), index)
                {
                    return report(runtime_error, error, error_length);
                }
            }
            0 => break,
            code => {
                return report(
                    HostError {
                        code: MicroErrorCode::Ui,
                        diagnostic: format!("ESP roller-change queue failed with code {code}"),
                    },
                    error,
                    error_length,
                );
            }
        }
    }
    match runtime.tick() {
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
