#![cfg(feature = "native")]

use micro_host_sdl::NativeBridge;
use micro_ir::{FunctionId, NodeId};
use micro_lvgl::NativeUi;

#[test]
fn native_pointer_click_activates_button() {
    let mut bridge = NativeBridge::create(320, 240, true).unwrap();
    bridge.create_column(NodeId(0), None).unwrap();
    bridge
        .create_button(NodeId(1), Some(NodeId(0)), "Add", FunctionId(7))
        .unwrap();
    let _ = bridge.timer();
    bridge.queue_click(NodeId(1)).unwrap();
    assert!(bridge.poll());
    assert_eq!(bridge.take_activation(), Some(FunctionId(7)));
    drop(bridge);
}
