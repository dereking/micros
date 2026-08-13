#![cfg(feature = "native")]

use micro_host_sdl::NativeBridge;
use micro_ir::FunctionId;

#[test]
fn native_create_and_destroy_hidden_window() {
    let mut bridge = NativeBridge::create(320, 240, true).unwrap();
    bridge.inject_activation(FunctionId(7));
    assert_eq!(bridge.take_activation(), Some(FunctionId(7)));
    assert!(bridge.poll());
    let _ = bridge.timer();
    drop(bridge);
}
