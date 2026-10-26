#![cfg(feature = "native")]

use std::sync::Mutex;

use micro_host_sdl::{HostAction, NativeBridge};
use micro_ir::{AnchorSpec, FunctionId, LayoutSpec, NodeId, TextStyle};
use micro_lvgl::NativeUi;

/// SDL/LVGL keep process-global state; creating and destroying two native
/// bridges concurrently segfaults. Each test owns the one bridge at a time.
static NATIVE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn native_pointer_click_activates_button() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let mut bridge = NativeBridge::create(320, 240, true).unwrap();
    bridge.create_column(NodeId(0), None).unwrap();
    bridge
        .create_button(
            NodeId(1),
            Some(NodeId(0)),
            "Add",
            FunctionId(7),
            Some(&TextStyle::DEFAULT_BUTTON),
            None,
        )
        .unwrap();
    let _ = bridge.timer();
    bridge.queue_click(NodeId(1)).unwrap();
    assert!(bridge.poll());
    assert_eq!(bridge.take_activation(), Some(FunctionId(7)));
    drop(bridge);
}

#[test]
fn orientation_switches_viewport_and_pointer_coordinates() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let bridge = NativeBridge::create(320, 240, true).unwrap();
    assert!(!bridge.portrait());
    assert_eq!(bridge.rotation(), 0);

    bridge.set_orientation(true).unwrap();
    assert!(bridge.portrait());
    assert_eq!(bridge.rotation(), 90);

    // Portrait swaps the logical canvas to 240x320; fitting that into the
    // 320x240 window gives 0.75 scale and a 70 px pillarbox on each side.
    let center = bridge.logical_to_window(120.0, 160.0);
    assert!((center.0 - 160.0).abs() < 0.75, "center x = {center:?}");
    assert!((center.1 - 120.0).abs() < 0.75, "center y = {center:?}");
    let roundtrip = bridge
        .window_to_logical(center.0, center.1)
        .expect("center is inside the canvas");
    assert!((roundtrip.0 - 120.0).abs() < 0.75, "center = {roundtrip:?}");
    assert!((roundtrip.1 - 160.0).abs() < 0.75, "center = {roundtrip:?}");

    // The logical top-left is still the canvas top-left (no bitmap rotation).
    let top_left = bridge.logical_to_window(0.0, 0.0);
    assert!((top_left.0 - 70.0).abs() < 1.0, "top-left = {top_left:?}");
    assert!(top_left.1.abs() < 1.0, "top-left = {top_left:?}");
    let roundtrip = bridge
        .window_to_logical(top_left.0, top_left.1)
        .expect("corner is inside the canvas");
    assert!(roundtrip.0.abs() < 1.0, "corner = {roundtrip:?}");
    assert!(roundtrip.1.abs() < 1.0, "corner = {roundtrip:?}");

    // Points in the pillarbox are rejected rather than clicking an edge.
    assert_eq!(bridge.window_to_logical(10.0, 120.0), None);
    assert_eq!(bridge.window_to_logical(310.0, 120.0), None);

    bridge.set_orientation(false).unwrap();
    assert!(!bridge.portrait());
    let center = bridge.logical_to_window(160.0, 120.0);
    assert!((center.0 - 160.0).abs() < 0.75, "center = {center:?}");
    assert!((center.1 - 120.0).abs() < 0.75, "center = {center:?}");
}

#[test]
fn every_orientation_keeps_the_center_centered() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let bridge = NativeBridge::create(320, 240, true).unwrap();
    for portrait in [false, true] {
        bridge.set_orientation(portrait).unwrap();
        let (design_w, design_h) = if portrait {
            (240.0, 320.0)
        } else {
            (320.0, 240.0)
        };
        let window = bridge.logical_to_window(design_w / 2.0, design_h / 2.0);
        assert!((window.0 - 160.0).abs() < 0.75, "{portrait}: {window:?}");
        assert!((window.1 - 120.0).abs() < 0.75, "{portrait}: {window:?}");
        let logical = bridge.window_to_logical(window.0, window.1).unwrap();
        assert!(
            (logical.0 - design_w / 2.0).abs() < 0.75,
            "{portrait}: {logical:?}"
        );
        assert!(
            (logical.1 - design_h / 2.0).abs() < 0.75,
            "{portrait}: {logical:?}"
        );
    }
}

#[test]
fn zoom_fit_and_percent_are_reported() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let bridge = NativeBridge::create(320, 240, true).unwrap();
    bridge.set_zoom(150).unwrap();
    assert_eq!(bridge.zoom_percent(), 150);
    bridge.set_zoom_fit().unwrap();
    // The hidden dummy window is exactly the design size, so fit is 100%.
    assert_eq!(bridge.zoom_percent(), 100);
    bridge.set_zoom(10).unwrap();
    assert_eq!(bridge.zoom_percent(), 25);
}

#[test]
fn orientation_reflows_anchored_children_in_place() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let mut bridge = NativeBridge::create(320, 240, true).unwrap();
    bridge.create_column(NodeId(0), None).unwrap();
    bridge.create_column(NodeId(1), Some(NodeId(0))).unwrap();
    bridge
        .set_layout_spec(
            NodeId(1),
            LayoutSpec {
                left: None,
                top: Some(10.0),
                width: Some(80.0),
                height: Some(40.0),
                anchor: AnchorSpec {
                    left: None,
                    top: None,
                    right: Some(20.0),
                    bottom: None,
                },
            },
        )
        .unwrap();
    bridge.apply_delphi_layout(NodeId(0), &[NodeId(1)]).unwrap();
    let (landscape_x, _, width, _) = bridge.object_rect(NodeId(1)).expect("mounted");
    assert_eq!((landscape_x, width), (220, 80));
    let (_, root_y, root_w, root_h) = bridge.object_rect(NodeId(0)).expect("root mounted");
    assert_eq!((root_y, root_w, root_h), (40, 320, 200));

    // Switching orientation keeps the same mounted tree and re-runs anchors
    // against the 240 px portrait width: x = 240 - 20 - 80.
    bridge.set_orientation(true).unwrap();
    let (portrait_x, _, width, _) = bridge.object_rect(NodeId(1)).expect("still mounted");
    assert_eq!((portrait_x, width), (140, 80));
    let (_, root_y, root_w, root_h) = bridge.object_rect(NodeId(0)).expect("root still mounted");
    assert_eq!((root_y, root_w, root_h), (40, 240, 280));
}

#[test]
fn mouse_wheel_scrolls_the_active_tab_page() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let mut bridge = NativeBridge::create(320, 240, true).unwrap();
    bridge
        .create_tabview(NodeId(0), None, &["One".to_owned(), "Two".to_owned()])
        .unwrap();
    bridge.create_tab_content(0).unwrap();
    bridge.create_column(NodeId(1), Some(NodeId(0))).unwrap();
    bridge
        .create_label(
            NodeId(2),
            Some(NodeId(1)),
            "top",
            Some(&TextStyle::DEFAULT_TEXT),
        )
        .unwrap();
    bridge
        .create_label(
            NodeId(3),
            Some(NodeId(1)),
            "bottom",
            Some(&TextStyle::DEFAULT_TEXT),
        )
        .unwrap();
    bridge
        .set_layout_spec(
            NodeId(2),
            LayoutSpec {
                left: Some(0.0),
                top: Some(0.0),
                width: Some(200.0),
                height: Some(40.0),
                anchor: AnchorSpec::default(),
            },
        )
        .unwrap();
    bridge
        .set_layout_spec(
            NodeId(3),
            LayoutSpec {
                left: Some(0.0),
                top: Some(600.0),
                width: Some(200.0),
                height: Some(40.0),
                anchor: AnchorSpec::default(),
            },
        )
        .unwrap();
    bridge
        .apply_delphi_layout(NodeId(1), &[NodeId(2), NodeId(3)])
        .unwrap();
    let _ = bridge.timer();

    let (_, _, _, tab_h) = bridge.object_rect(NodeId(1)).expect("tab content mounted");
    assert_eq!(
        tab_h, 152,
        "tab content should fill the page below the 48 px tab bar"
    );
    let (_, top_y, _, _) = bridge.object_rect(NodeId(2)).expect("top label mounted");
    // One wheel notch down (SDL's negative Y) over the tab page must move the
    // page content and its scrollbar, exactly like a finger drag.
    bridge.queue_wheel(160.0, 150.0, 0.0, -1.0).unwrap();
    assert!(bridge.poll());
    let _ = bridge.timer();
    let (_, scrolled_y, _, _) = bridge.object_rect(NodeId(2)).expect("top label mounted");
    assert!(
        scrolled_y < top_y,
        "wheel down should scroll content up: {top_y} -> {scrolled_y}"
    );

    // A wheel-up notch returns to the top instead of leaving the page stuck.
    bridge.queue_wheel(160.0, 150.0, 0.0, 1.0).unwrap();
    assert!(bridge.poll());
    let _ = bridge.timer();
    let (_, restored_y, _, _) = bridge.object_rect(NodeId(2)).expect("top label mounted");
    assert_eq!(restored_y, top_y, "wheel up should restore the top");

    // Trackpads report fractional ticks. Twelve 0.1-tick events must add up to
    // a visible scroll instead of rounding every event back to zero.
    for _ in 0..12 {
        bridge.queue_wheel(160.0, 150.0, 0.0, -0.1).unwrap();
    }
    assert!(bridge.poll());
    let _ = bridge.timer();
    let (_, trackpad_y, _, _) = bridge.object_rect(NodeId(2)).expect("top label mounted");
    assert!(
        trackpad_y < top_y,
        "fractional trackpad ticks should accumulate: {top_y} -> {trackpad_y}"
    );
    drop(bridge);
}

#[test]
fn portrait_click_still_activates_button() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let mut bridge = NativeBridge::create(320, 240, true).unwrap();
    bridge.set_orientation(true).unwrap();
    bridge.create_column(NodeId(0), None).unwrap();
    bridge
        .create_button(
            NodeId(1),
            Some(NodeId(0)),
            "Add",
            FunctionId(11),
            Some(&TextStyle::DEFAULT_BUTTON),
            None,
        )
        .unwrap();
    bridge
        .set_layout_spec(
            NodeId(1),
            LayoutSpec {
                left: Some(10.0),
                top: Some(10.0),
                width: Some(80.0),
                height: Some(40.0),
                anchor: AnchorSpec::default(),
            },
        )
        .unwrap();
    bridge.apply_delphi_layout(NodeId(0), &[NodeId(1)]).unwrap();
    let _ = bridge.timer();
    bridge.queue_click(NodeId(1)).unwrap();
    assert!(bridge.poll());
    assert_eq!(bridge.take_activation(), Some(FunctionId(11)));
    drop(bridge);
}

#[test]
fn host_menu_actions_queue_and_state_snapshot_updates() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let mut bridge = NativeBridge::create(800, 480, true).unwrap();
    assert!(!bridge.host_state().expect("visible state").powered_off);
    // Headless windows have no native menu; the API must report that instead
    // of trying to pop AppKit chrome.
    assert!(!bridge.open_host_menu());

    // The platform menu posts through an SDL user event; poll moves it into
    // the host queue exactly once.
    NativeBridge::request_host_action(HostAction::PowerOff);
    assert!(bridge.poll());
    assert_eq!(bridge.take_host_action(), Some(HostAction::PowerOff));
    assert_eq!(bridge.take_host_action(), None);

    bridge.power_off();
    assert!(bridge.host_state().expect("off state").powered_off);
    NativeBridge::request_host_action(HostAction::PowerOn);
    assert!(bridge.poll());
    assert_eq!(bridge.take_host_action(), Some(HostAction::PowerOn));
    bridge.wake();
    assert!(!bridge.host_state().expect("on state").powered_off);

    bridge.set_zoom_fit().unwrap();
    let fit = bridge.host_state().expect("fit state");
    assert!(fit.zoom_fit);
    bridge.set_zoom(125).unwrap();
    let zoom = bridge.host_state().expect("zoom state");
    assert!(!zoom.zoom_fit);
    assert_eq!(zoom.zoom_percent, 125);

    bridge.set_orientation(true).unwrap();
    assert!(bridge.host_state().expect("portrait").portrait);
    bridge.set_orientation(false).unwrap();
    assert!(!bridge.host_state().expect("landscape").portrait);
    drop(bridge);
}
