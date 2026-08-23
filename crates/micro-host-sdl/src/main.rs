#[cfg(feature = "native")]
use std::time::Duration;

#[cfg(feature = "native")]
use micro_core::{Event, Runtime};
#[cfg(feature = "native")]
use micro_host_sdl::{NativeBridge, host::NativeHost};
#[cfg(feature = "native")]
use micro_ir::{StateId, decode};
#[cfg(feature = "native")]
use micro_lvgl::LvglRenderer;
#[cfg(feature = "native")]
use micro_vm::Value;

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("micro-host-sdl requires --features native");
    std::process::exit(1);
}

#[cfg(feature = "native")]
fn main() {
    if let Err(error) = run() {
        eprintln!("micro-host-sdl: {error}");
        std::process::exit(1);
    }
}

#[cfg(feature = "native")]
fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let (smoke, path) = match arguments.as_slice() {
        [path] => (false, path),
        [flag, path] if flag == "--smoke" => (true, path),
        _ => return Err("usage: micro-host-sdl [--smoke] <app.mbc>".into()),
    };
    let bytes = std::fs::read(path).map_err(|error| format!("cannot read MBC: {error}"))?;
    let image = decode(&bytes).map_err(|error| format!("cannot load MBC: {error}"))?;
    let first_button = image
        .nodes
        .iter()
        .find_map(|node| match node.kind {
            micro_ir::UiKind::Button => node.on_click.map(|handler| (node.id, handler)),
            _ => None,
        });
    let bridge = NativeBridge::create(480, 320, smoke)?;
    let renderer = LvglRenderer::new(bridge);
    let mut runtime = Runtime::new_with_host(image, renderer, 10_000, Box::new(NativeHost::new()))
        .map_err(|error| error.to_string())?;

    if smoke {
        let (button, _handler) =
            first_button.ok_or_else(|| "smoke App has no button handler".to_owned())?;
        runtime.renderer_mut().bridge_mut().queue_click(button)?;
        runtime.renderer_mut().bridge_mut().queue_click(button)?;
        if !runtime.renderer_mut().bridge_mut().poll() {
            return Err("smoke native host quit while processing clicks".into());
        }
        host_iteration(&mut runtime)?;
        if runtime.state(StateId(0)) != Some(&Value::Number(2.0)) {
            return Err("smoke Counter did not reach state 2".into());
        }
        return Ok(());
    }

    while runtime.renderer_mut().bridge_mut().poll() {
        host_iteration(&mut runtime)?;
        let delay = runtime.renderer_mut().bridge_mut().timer().clamp(1, 16);
        std::thread::sleep(Duration::from_millis(u64::from(delay)));
    }
    Ok(())
}

#[cfg(feature = "native")]
fn host_iteration(runtime: &mut Runtime<LvglRenderer<NativeBridge>>) -> Result<(), String> {
    while let Some(handler) = runtime.renderer_mut().bridge_mut().take_activation() {
        runtime.enqueue(Event::Activate(handler));
    }
    loop {
        match runtime.tick() {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("micro runtime event error: {error}"),
        }
    }
    /* Async host requests (net.scanWifi / net.httpGet) complete one tick
     * later with the simulated result. */
    runtime.enqueue_host_results();
    let _ = runtime.renderer_mut().bridge_mut().timer();
    Ok(())
}
