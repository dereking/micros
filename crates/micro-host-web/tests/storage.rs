//! Off-wasm checks for the Web host's `storage.*` routing. The in-memory
//! backend mirrors the browser's per-App localStorage boundary and the same
//! `micro-store` path policy.

use micro_host_web::host::WebHost;
use micro_ir::{HostCallKind, HostRequest, ScalarType};
use micro_vm::{HostAccess, Value};

fn request(kind: HostCallKind, arg_kinds: Vec<ScalarType>) -> HostRequest {
    HostRequest::sync(kind, arg_kinds, None)
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

#[test]
fn web_storage_round_trips_and_isolates_apps() {
    let mut a = WebHost::new_for_app("app-a");
    let mut b = WebHost::new_for_app("app-b");

    let available = a
        .call(&request(HostCallKind::StorageAvailable, vec![]), &[])
        .expect("available");
    assert_eq!(available, Some(Value::Bool(true)));

    let write = a
        .call(
            &request(
                HostCallKind::StorageWriteText,
                vec![ScalarType::String, ScalarType::String],
            ),
            &[text("notes/today.txt"), text("hello")],
        )
        .expect("write");
    assert_eq!(write, Some(Value::Bool(true)));
    let read = a
        .call(
            &request(HostCallKind::StorageReadText, vec![ScalarType::String]),
            &[text("notes/today.txt")],
        )
        .expect("read");
    assert_eq!(read, Some(Value::String("hello".into())));

    let other = b
        .call(
            &request(HostCallKind::StorageReadText, vec![ScalarType::String]),
            &[text("notes/today.txt")],
        )
        .expect("cross-app read");
    assert_eq!(other, Some(Value::String(String::new())));

    let escape = a
        .call(
            &request(
                HostCallKind::StorageWriteText,
                vec![ScalarType::String, ScalarType::String],
            ),
            &[text("../app-b/secret.txt"), text("pwn")],
        )
        .expect("escape write");
    assert_eq!(escape, Some(Value::Bool(false)));

    let list = a
        .call(
            &request(HostCallKind::StorageList, vec![ScalarType::String]),
            &[text("")],
        )
        .expect("list");
    assert_eq!(list, Some(Value::String("notes/".into())));
}
