use std::ffi::{c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use micro_ir::{FunctionId, NodeId};
use micro_lvgl::NativeUi;

use crate::{HostError, MicroAction, MicroErrorCode, MicroEvent, MicroState, OsHost, RuntimeHost};

unsafe extern "C" {
    fn micro_esp_ui_create_column(node: u32, parent: u32) -> c_int;
    fn micro_esp_ui_create_label(node: u32, parent: u32, text: *const u8, len: usize) -> c_int;
    fn micro_esp_ui_create_button(
        node: u32,
        parent: u32,
        text: *const u8,
        len: usize,
        handler: u32,
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
    ) -> Result<(), String> {
        native_result(unsafe {
            micro_esp_ui_create_label(node.0, parent_id(parent), text.as_ptr(), text.len())
        })
    }

    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
    ) -> Result<(), String> {
        native_result(unsafe {
            micro_esp_ui_create_button(
                node.0,
                parent_id(parent),
                text.as_ptr(),
                text.len(),
                handler.0,
            )
        })
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
    if buffer.is_null() || length == 0 {
        return;
    }
    let bytes = diagnostic.as_bytes();
    let copied = bytes.len().min(length - 1);
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), copied);
        *buffer.add(copied) = 0;
    }
}

fn report(error: HostError, buffer: *mut c_char, length: usize) -> c_int {
    write_diagnostic(buffer, length, &error.to_string());
    error.code() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_create(
    mbc: *const u8,
    length: usize,
    budget: u64,
    error: *mut c_char,
    error_length: usize,
) -> *mut c_void {
    if mbc.is_null() || length == 0 || budget == 0 {
        write_diagnostic(error, error_length, "invalid runtime arguments");
        return ptr::null_mut();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let owned = unsafe { std::slice::from_raw_parts(mbc, length) }.to_vec();
        RuntimeHost::new(&owned, EspNativeUi, budget)
    }));
    match result {
        Ok(Ok(runtime)) => Box::into_raw(Box::new(runtime)).cast(),
        Ok(Err(runtime_error)) => {
            write_diagnostic(error, error_length, &runtime_error.to_string());
            ptr::null_mut()
        }
        Err(_) => {
            write_diagnostic(error, error_length, "panic contained in runtime create");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_activate(runtime: *mut c_void, handler_id: u32) -> c_int {
    if runtime.is_null() {
        return MicroErrorCode::InvalidArgument as c_int;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        unsafe { &mut *runtime.cast::<RuntimeHost<EspNativeUi>>() }.activate(FunctionId(handler_id))
    })) {
        Ok(Ok(())) => MicroErrorCode::Ok as c_int,
        Ok(Err(error)) => error.code() as c_int,
        Err(_) => MicroErrorCode::Panic as c_int,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_tick(
    runtime: *mut c_void,
    error: *mut c_char,
    error_length: usize,
) -> c_int {
    if runtime.is_null() {
        write_diagnostic(error, error_length, "runtime handle is null");
        return MicroErrorCode::InvalidArgument as c_int;
    }
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        (&mut *runtime.cast::<RuntimeHost<EspNativeUi>>()).tick()
    })) {
        Ok(Ok(_)) => MicroErrorCode::Ok as c_int,
        Ok(Err(runtime_error)) => report(runtime_error, error, error_length),
        Err(_) => {
            write_diagnostic(error, error_length, "panic contained in runtime tick");
            MicroErrorCode::Panic as c_int
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_runtime_destroy(runtime: *mut c_void) {
    if runtime.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        let mut runtime = Box::from_raw(runtime.cast::<RuntimeHost<EspNativeUi>>());
        let _ = runtime.stop();
    }));
}

#[unsafe(no_mangle)]
pub extern "C" fn micro_os_create() -> *mut c_void {
    catch_unwind(|| Box::into_raw(Box::new(OsHost::new())).cast()).unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_os_dispatch(os: *mut c_void, event: MicroEvent) -> MicroAction {
    if os.is_null() {
        return MicroAction::Rejected;
    }
    catch_unwind(AssertUnwindSafe(|| unsafe {
        (&mut *os.cast::<OsHost>()).dispatch(event)
    }))
    .unwrap_or(MicroAction::Rejected)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_os_state(os: *const c_void) -> MicroState {
    if os.is_null() {
        return MicroState::SafeMode;
    }
    catch_unwind(AssertUnwindSafe(|| unsafe {
        (&*os.cast::<OsHost>()).state()
    }))
    .unwrap_or(MicroState::SafeMode)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn micro_os_destroy(os: *mut c_void) {
    if !os.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            drop(Box::from_raw(os.cast::<OsHost>()));
        }));
    }
}
