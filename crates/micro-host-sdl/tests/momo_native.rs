//! macOS native smoke for the momo learning flow.
//!
//! Web has a Playwright flow test; this mirrors the same sequence through the
//! real LVGL renderer and SDL-backed bridge in headless mode. SDL pointer
//! events need a visible window, so the test injects the same handler
//! activations the bridge emits after a click. It is gated on the `native`
//! feature and needs `npm run build:app` to have produced the MBC.

#![cfg(feature = "native")]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use micro_core::{Event, Runtime};
use micro_host_sdl::{NativeBridge, host::NativeHost};
use micro_ir::{AppImage, Constant, NodeId, StateId, TextSource, UiKind, decode};
use micro_lvgl::LvglRenderer;
use micro_store::{KvStore, KvValue, NativeConfigStore, NativeStore};
use micro_vm::Value;

/// SDL/LVGL has process-global state and is not safe to initialize from
/// several test threads at once.
static NATIVE_TEST_LOCK: Mutex<()> = Mutex::new(());

fn button(image: &AppImage, label: &str) -> NodeId {
    image
        .nodes
        .iter()
        .find(|node| {
            if node.kind != UiKind::Button {
                return false;
            }
            match node.text {
                Some(TextSource::Constant(index)) => {
                    matches!(
                        image.constants.get(index as usize),
                        Some(Constant::String(text)) if text == label
                    )
                }
                _ => false,
            }
        })
        .map(|node| node.id)
        .unwrap_or_else(|| panic!("button `{label}`"))
}

fn pump(runtime: &mut Runtime<LvglRenderer<NativeBridge>>) {
    runtime.renderer_mut().bridge_mut().poll();
    while let Some(handler) = runtime.renderer_mut().bridge_mut().take_activation() {
        runtime.enqueue(Event::Activate(handler));
    }
    while let Some((handler, text)) = runtime.renderer_mut().bridge_mut().take_input_change() {
        runtime.enqueue(Event::InputChanged(handler, text));
    }
    while let Some((handler, value)) = runtime.renderer_mut().bridge_mut().take_slider_change() {
        runtime.enqueue(Event::SliderChanged(handler, value));
    }
    while let Some((handler, checked)) = runtime.renderer_mut().bridge_mut().take_checkbox_change()
    {
        runtime.enqueue(Event::CheckedChanged(handler, checked));
    }
    while let Some((handler, index)) = runtime.renderer_mut().bridge_mut().take_dropdown_change() {
        runtime.enqueue(Event::SelectionChanged(handler, index));
    }
    while let Some((handler, index)) = runtime.renderer_mut().bridge_mut().take_roller_change() {
        runtime.enqueue(Event::SelectionChanged(handler, index));
    }
    loop {
        match runtime.tick() {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => panic!("runtime error: {error}"),
        }
    }
    runtime.enqueue_host_results();
    let _ = runtime.renderer_mut().bridge_mut().timer();
}

fn number(runtime: &Runtime<LvglRenderer<NativeBridge>>, id: u32) -> f64 {
    match runtime.state(StateId(id)) {
        Some(Value::Number(value)) => *value,
        other => panic!("state {id} is not a number: {other:?}"),
    }
}

#[test]
fn momo_flow_renders_and_grades_on_macos() {
    let _guard = NATIVE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/momo/dist/momo.mbc");
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; run `npm run build:app` first",
            path.display()
        )
    });
    let image = decode(&bytes).expect("momo MBC");
    let start = button(&image, "开始今日学习");
    let unknown = button(&image, "不认识");
    let known = button(&image, "认识");
    let start_handler = image.nodes[start.0 as usize]
        .on_click
        .expect("start handler");
    let unknown_handler = image.nodes[unknown.0 as usize]
        .on_click
        .expect("unknown handler");
    let known_handler = image.nodes[known.0 as usize]
        .on_click
        .expect("known handler");
    let blank_handler = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Surface && node.on_click.is_some())
        .and_then(|node| node.on_click)
        .expect("tappable blank surface");

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("micros-momo-flow-{}-{stamp}", std::process::id()));
    let bridge = NativeBridge::create(800, 480, true).expect("headless SDL/LVGL bridge");
    let renderer = LvglRenderer::new(bridge);
    let host = NativeHost::new_for_app_at(&root, "momo").silent();
    let mut runtime =
        Runtime::new_with_host(image, renderer, 10_000, Box::new(host)).expect("boot momo");
    pump(&mut runtime);

    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(start_handler);
    pump(&mut runtime);
    assert_eq!(number(&runtime, 0), 1.0, "mode should be studying");
    assert_eq!(number(&runtime, 1), 0.0, "phase should be listen");

    for step in 1..=3 {
        runtime
            .renderer_mut()
            .bridge_mut()
            .inject_activation(unknown_handler);
        pump(&mut runtime);
        assert_eq!(
            number(&runtime, 1),
            step as f64,
            "phase after reveal {step}"
        );
    }
    assert_eq!(number(&runtime, 1), 3.0, "definition phase");
    assert_eq!(number(&runtime, 0), 1.0, "still studying before grading");

    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(known_handler);
    pump(&mut runtime);
    assert_eq!(number(&runtime, 0), 1.0, "advances to the next word");
    assert_eq!(number(&runtime, 1), 0.0, "next word starts at listen");
    assert_ne!(number(&runtime, 2), 0.0, "current word index advanced");
    assert_eq!(
        number(&runtime, 19),
        1.0,
        "definition waits for a blank tap"
    );

    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(blank_handler);
    pump(&mut runtime);
    assert_eq!(number(&runtime, 19), 0.0, "blank tap advances the review");
    assert_eq!(number(&runtime, 1), 0.0, "next word still starts at listen");
    drop(runtime);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn native_input_accepts_typed_text_and_persists_it() {
    let _guard = NATIVE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/momo/dist/momo.mbc");
    let bytes = std::fs::read(&path).expect("momo MBC; run `npm run build:app` first");
    let image = decode(&bytes).expect("momo MBC");
    let input = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Input)
        .map(|node| node.id)
        .expect("momo profile name input");
    let save = button(&image, "保存");
    let save_handler = image.nodes[save.0 as usize].on_click.expect("save handler");
    let edit = button(&image, "编辑");
    let edit_handler = image.nodes[edit.0 as usize].on_click.expect("edit handler");

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("micros-momo-input-{}-{stamp}", std::process::id()));
    let bridge = NativeBridge::create(480, 320, true).expect("headless SDL/LVGL bridge");
    let renderer = LvglRenderer::new(bridge);
    let host = NativeHost::new_for_app_at(&root, "momo").silent();
    let mut runtime =
        Runtime::new_with_host(image, renderer, 10_000, Box::new(host)).expect("boot momo");
    pump(&mut runtime);
    // The profile starts read-only; entering edit mode materializes the name
    // and Token fields before typing.
    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(edit_handler);
    pump(&mut runtime);
    runtime
        .renderer_mut()
        .bridge_mut()
        .focus_textarea(input)
        .expect("focus name input");
    runtime
        .renderer_mut()
        .bridge_mut()
        .queue_text_input("abc")
        .expect("queue text input");
    pump(&mut runtime);
    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(save_handler);
    pump(&mut runtime);

    let config = NativeConfigStore::new(&root)
        .open("momo")
        .expect("momo config");
    assert_eq!(
        config.get("name").expect("read name"),
        Some(KvValue::String("默认用户abc".into()))
    );
    assert!(
        root.join("momo/config.json").is_file(),
        "the profile goes through the unified config file"
    );

    drop(runtime);
    let _ = std::fs::remove_dir_all(&root);
}

/// Real Maimemo round trip. Ignored by default because it needs network access
/// and a token; run it with `MAIMEMO_TOKEN=... cargo test ... -- --ignored`.
#[test]
#[ignore = "requires MAIMEMO_TOKEN and network access"]
fn native_sync_loads_today_items_from_maimemo() {
    let _guard = NATIVE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let token = std::env::var("MAIMEMO_TOKEN").expect("MAIMEMO_TOKEN");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/momo/dist/momo.mbc");
    let bytes = std::fs::read(&path).expect("momo MBC; run `npm run build:app` first");
    let image = decode(&bytes).expect("momo MBC");
    let input = image
        .nodes
        .iter()
        .filter(|node| node.kind == UiKind::Input)
        .nth(1)
        .map(|node| node.id)
        .expect("momo profile token input");
    let save = button(&image, "保存");
    let save_handler = image.nodes[save.0 as usize].on_click.expect("save handler");
    let edit = button(&image, "编辑");
    let edit_handler = image.nodes[edit.0 as usize].on_click.expect("edit handler");
    let sync = button(&image, "同步墨墨");
    let sync_handler = image.nodes[sync.0 as usize].on_click.expect("sync handler");
    let start = button(&image, "开始今日学习");
    let start_handler = image.nodes[start.0 as usize]
        .on_click
        .expect("start handler");

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("micros-momo-sync-{}-{stamp}", std::process::id()));
    let bridge = NativeBridge::create(480, 320, true).expect("headless SDL/LVGL bridge");
    let renderer = LvglRenderer::new(bridge);
    let host = NativeHost::new_for_app_at(&root, "momo").silent();
    let mut runtime =
        Runtime::new_with_host(image, renderer, 10_000, Box::new(host)).expect("boot momo");
    pump(&mut runtime);

    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(edit_handler);
    pump(&mut runtime);
    runtime
        .renderer_mut()
        .bridge_mut()
        .focus_textarea(input)
        .expect("focus token input");
    runtime
        .renderer_mut()
        .bridge_mut()
        .queue_text_input(&token)
        .expect("queue token");
    pump(&mut runtime);
    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(save_handler);
    pump(&mut runtime);
    assert_eq!(
        runtime.state(StateId(9)),
        Some(&Value::String("".into())),
        "token input should not sync by itself"
    );

    runtime
        .renderer_mut()
        .bridge_mut()
        .inject_activation(sync_handler);
    let mut status = String::new();
    for _ in 0..300 {
        pump(&mut runtime);
        status = match runtime.state(StateId(9)) {
            Some(Value::String(value)) => value.clone(),
            _ => String::new(),
        };
        if status.starts_with("已导入") && !status.contains("正在") {
            break;
        }
        if status.starts_with("同步失败")
            || status == "今日没有未完成单词。"
            || status == "墨墨今日列表为空：先在墨墨 App 打开今日学习并开启自动同步后重试。"
            || status == "词表已导入，但释义暂不可用。"
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        status.starts_with("已导入")
            || status == "今日没有未完成单词。"
            || status == "墨墨今日列表为空：先在墨墨 App 打开今日学习并开启自动同步后重试。",
        "unexpected sync status: {status}"
    );
    if status.starts_with("已导入") {
        assert!(
            number(&runtime, 10) > 0.0,
            "sync should report today's items"
        );
        assert!(
            number(&runtime, 15) > 0.0,
            "get_study_progress should report a planned total"
        );
        let kv = NativeStore::new(&root).open("momo").expect("momo KV");
        match kv.get("w0").expect("read word 0") {
            Some(KvValue::String(word)) => assert!(!word.is_empty(), "imported word is empty"),
            other => panic!("word 0 was not persisted: {other:?}"),
        }
        match kv.get("v0").expect("read voc id 0") {
            Some(KvValue::String(id)) => assert!(!id.is_empty(), "voc id 0 is empty"),
            other => panic!("voc id 0 was not persisted: {other:?}"),
        }
        match kv.get("m0").expect("read study count 0") {
            Some(KvValue::Number(_)) => {}
            other => panic!("study count 0 was not persisted: {other:?}"),
        }
        match kv.get("d0").expect("read definition 0") {
            Some(KvValue::String(definition)) => {
                assert!(!definition.is_empty(), "imported definition is empty")
            }
            other => panic!("definition 0 was not persisted: {other:?}"),
        }
        let pool_count = match kv.get("poolCount").expect("read pool count") {
            Some(KvValue::Number(value)) => value,
            other => panic!("pool count was not persisted: {other:?}"),
        };
        assert!(pool_count >= 1.0, "sync should cache a compact pool");
        let pool_next = match kv.get("poolNext").expect("read pool cursor") {
            Some(KvValue::Number(value)) => value,
            other => panic!("pool cursor was not persisted: {other:?}"),
        };
        assert!(
            pool_next >= 1.0 && pool_next <= pool_count,
            "pool cursor must point inside the cached list"
        );
        match kv.get("pool").expect("read pool") {
            Some(KvValue::String(pool)) => {
                assert!(pool.starts_with('['), "pool should be a JSON array");
                assert!(
                    pool.len() < 4096,
                    "pool must fit one ESP32 NVS value read buffer"
                );
            }
            other => panic!("pool was not persisted: {other:?}"),
        }
        runtime
            .renderer_mut()
            .bridge_mut()
            .inject_activation(start_handler);
        pump(&mut runtime);
        assert_eq!(number(&runtime, 0), 1.0, "study should start after sync");
        assert_eq!(number(&runtime, 2), 0.0, "first imported slot is current");
    }

    drop(runtime);
    let _ = std::fs::remove_dir_all(&root);
}
