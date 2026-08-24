#[cfg(feature = "native")]
use std::cell::RefCell;
#[cfg(feature = "native")]
use std::path::PathBuf;
#[cfg(feature = "native")]
use std::rc::Rc;
#[cfg(feature = "native")]
use std::time::Duration;

#[cfg(feature = "native")]
use micro_core::{Event, Runtime};
#[cfg(feature = "native")]
use micro_host_sdl::{NativeBridge, host::NativeHost, host::ShellState};
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
enum Launch {
    /// Boot a single app directly (no OS shell) — the historical demo mode.
    App(PathBuf),
    /// Boot the OS shell MBC with the given installed-app MBCs; the shell's
    /// `os.launchIndex` / `os.goBack` drive runtime switching.
    Os { shell: PathBuf, apps: Vec<PathBuf> },
}

#[cfg(feature = "native")]
fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let (smoke, launch) = match arguments.as_slice() {
        [path] => (false, Launch::App(PathBuf::from(path))),
        [flag, path] if flag == "--smoke" => (true, Launch::App(PathBuf::from(path))),
        [flag, shell, apps @ ..] if flag == "--os" => (
            false,
            Launch::Os {
                shell: PathBuf::from(shell),
                apps: apps.iter().map(PathBuf::from).collect(),
            },
        ),
        [flag, shell, apps @ ..] if flag == "--os-smoke" => {
            (true, Launch::Os { shell: PathBuf::from(shell), apps: apps.iter().map(PathBuf::from).collect() })
        }
        _ => {
            return Err(
                "usage: micro-host-sdl [--smoke] <app.mbc> | --os <shell.mbc> <app.mbc>...".into(),
            )
        }
    };
    match launch {
        Launch::App(path) => run_single(smoke, &path),
        Launch::Os { shell, apps } if smoke => run_os_smoke(&shell, &apps),
        Launch::Os { shell, apps } => run_os(false, &shell, &apps),
    }
}

/// Headless OS-mode smoke: boot the shell, script an `os.launchIndex(0)` tap,
/// assert the app runtime replaces the shell, script an `os.goBack`, and assert
/// the shell returns. Exercises the shared-bridge runtime switching without a
/// visible window.
#[cfg(feature = "native")]
fn run_os_smoke(shell_path: &PathBuf, app_paths: &[PathBuf]) -> Result<(), String> {
    if app_paths.is_empty() {
        return Err("smoke OS mode needs at least one app MBC".into());
    }
    let registry = app_paths
        .iter()
        .map(|path| {
            let image = decode_mbc(path)?;
            Ok((path.clone(), image.metadata.name.clone(), image.metadata.icon.clone()))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let bridge = NativeBridge::create(800, 480, true)?;
    let nav = Rc::new(RefCell::new(ShellState {
        apps: registry
            .iter()
            .map(|(_, name, icon)| (name.clone(), icon.clone()))
            .collect(),
        ..Default::default()
    }));

    let shell_image = decode_mbc(shell_path)?;
    let mut runtime = boot_runtime(&bridge, shell_image, nav.clone())?;
    let mut is_shell = true;

    // Launcher tap: the shell's os.launchIndex(0).
    nav.borrow_mut().pending_launch = Some(0);
    host_iteration(&mut runtime)?;
    let launch = nav.borrow_mut().pending_launch.take();
    if launch == Some(0) {
        let (path, _, _) = &registry[0];
        drop(runtime);
        runtime = boot_runtime(&bridge, decode_mbc(path)?, nav.clone())?;
        is_shell = false;
    }
    if is_shell {
        return Err("smoke OS: launcher tap did not switch to the app".into());
    }

    // App back: os.goBack returns to the shell.
    nav.borrow_mut().pending_back = true;
    host_iteration(&mut runtime)?;
    nav.borrow_mut().pending_back = false;
    drop(runtime);
    runtime = boot_runtime(&bridge, decode_mbc(shell_path)?, nav.clone())?;
    is_shell = true;
    if !is_shell {
        return Err("smoke OS: goBack did not return to the shell".into());
    }

    let _ = &bridge;
    let _ = &runtime;
    Ok(())
}

#[cfg(feature = "native")]
fn run_single(smoke: bool, path: &PathBuf) -> Result<(), String> {
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
    let nav = Rc::new(RefCell::new(ShellState::default()));
    let mut runtime = boot_runtime(&bridge, image, nav)?;

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

/// Boot the OS shell MBC (index 0 of the partition on device) and swap between
/// the shell and installed-app runtimes on `os.launchIndex` / `os.goBack`.
///
/// The SDL/LVGL environment is created once and shared across runtimes via an
/// `Rc`-shared `NativeBridge`; switching runtimes drops the previous renderer
/// (which clears its LVGL object tree) before booting the next image into the
/// same display.
#[cfg(feature = "native")]
fn run_os(smoke: bool, shell_path: &PathBuf, app_paths: &[PathBuf]) -> Result<(), String> {
    let registry = app_paths
        .iter()
        .map(|path| {
            let image = decode_mbc(path)?;
            Ok((path.clone(), image.metadata.name.clone(), image.metadata.icon.clone()))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut bridge = NativeBridge::create(800, 480, smoke)?;
    let nav = Rc::new(RefCell::new(ShellState {
        apps: registry
            .iter()
            .map(|(_, name, icon)| (name.clone(), icon.clone()))
            .collect(),
        ..Default::default()
    }));

    let shell_image = decode_mbc(shell_path)?;
    let mut runtime = boot_runtime(&bridge, shell_image, nav.clone())?;
    let mut is_shell = true;

    while bridge.poll() {
        host_iteration(&mut runtime)?;

        let launch = nav.borrow_mut().pending_launch.take();
        let back = nav.borrow_mut().pending_back || bridge.take_back_gesture();
        nav.borrow_mut().pending_back = false;

        if is_shell {
            if let Some(index) = launch {
                let index = index as usize;
                if let Some((path, _, _)) = registry.get(index) {
                    // Drop the shell renderer (clears its LVGL tree) before the
                    // app runtime builds its tree into the same display.
                    drop(runtime);
                    runtime = boot_runtime(&bridge, decode_mbc(path)?, nav.clone())?;
                    is_shell = false;
                }
            }
        } else if back {
            drop(runtime);
            runtime = boot_runtime(&bridge, decode_mbc(shell_path)?, nav.clone())?;
            is_shell = true;
        }

        let delay = runtime.renderer_mut().bridge_mut().timer().clamp(1, 16);
        std::thread::sleep(Duration::from_millis(u64::from(delay)));
    }
    Ok(())
}

#[cfg(feature = "native")]
fn read_mbc(path: &PathBuf) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("cannot read MBC: {error}"))
}

#[cfg(feature = "native")]
fn decode_mbc(path: &PathBuf) -> Result<micro_ir::AppImage, String> {
    decode(&read_mbc(path)?).map_err(|error| format!("cannot decode MBC: {error}"))
}

#[cfg(feature = "native")]
fn boot_runtime(
    bridge: &NativeBridge,
    image: micro_ir::AppImage,
    nav: Rc<RefCell<ShellState>>,
) -> Result<Runtime<LvglRenderer<NativeBridge>>, String> {
    let renderer = LvglRenderer::new(bridge.clone());
    let mut host = NativeHost::new();
    host.nav = nav;
    Runtime::new_with_host(image, renderer, 10_000, Box::new(host)).map_err(|error| error.to_string())
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
    /* Async host requests (net.scanWifi / net.httpGet / os.delay) complete one
     * tick later with the simulated result. */
    runtime.enqueue_host_results();
    let _ = runtime.renderer_mut().bridge_mut().timer();
    Ok(())
}
