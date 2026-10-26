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
use micro_host_sdl::{HostAction, NativeBridge, host::NativeHost, host::ShellState};
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
#[derive(Clone, Copy, Debug)]
struct Presentation {
    /// Logical device orientation. The OS chrome and App layout reflow.
    portrait: bool,
    /// Device scale in percent; `None` fits the display.
    zoom: Option<u16>,
}

#[cfg(feature = "native")]
impl Default for Presentation {
    fn default() -> Self {
        Self {
            portrait: false,
            zoom: Some(100),
        }
    }
}

#[cfg(feature = "native")]
const USAGE: &str = "\
usage:
  micro-host-sdl [--smoke] [--orientation landscape|portrait] [--zoom 25..400|fit] <app.mbc>
  micro-host-sdl [--smoke] [--orientation landscape|portrait] [--zoom ...] --os <shell.mbc> <app.mbc>...

host window:
  toolbar             top icon bar: Landscape / Portrait / Rotate
  Device menu         macOS gear: Power On/Off, Force Restart, orientation,
                      zoom presets and emulator info (Cmd/Ctrl+,)
  Cmd/Ctrl+R          toggle device orientation (landscape <-> portrait)
  Cmd/Ctrl+0          fit the device view to the current display
  Cmd/Ctrl+1          reset to 100%
  Cmd/Ctrl+= / -      zoom in / out
  Cmd/Ctrl+wheel      zoom in / out

Orientation switches the LVGL device resolution (800x480 in OS mode) and
reflows the OS app bar and App layout in place; App state is preserved.
--rotate 90/270 remains as a compatibility alias for --orientation portrait.";

#[cfg(feature = "native")]
fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{USAGE}");
        return Ok(());
    }
    let (smoke, launch, presentation) = parse_arguments(arguments)?;
    match launch {
        Launch::App(path) => run_single(smoke, &path, presentation),
        Launch::Os { shell, apps } if smoke => run_os_smoke(&shell, &apps, presentation),
        Launch::Os { shell, apps } => run_os(false, &shell, &apps, presentation),
    }
}

#[cfg(feature = "native")]
fn parse_arguments(
    mut arguments: Vec<std::ffi::OsString>,
) -> Result<(bool, Launch, Presentation), String> {
    let mut smoke = false;
    let mut presentation = Presentation::default();

    /* Presentation flags are host options, not launch targets. Pull them out
     * first so they work before or after `--os <shell> <app...>` (npm appends
     * `npm run demo -- --rotate 90` after the script's own arguments). */
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].to_str() {
            Some("--smoke") => {
                smoke = true;
                arguments.remove(index);
            }
            Some("--orientation") => {
                let Some(value) = arguments.get(index + 1) else {
                    return Err("--orientation needs landscape or portrait".into());
                };
                presentation.portrait = match value.to_str() {
                    Some("landscape") => false,
                    Some("portrait") => true,
                    _ => return Err("--orientation needs landscape or portrait".into()),
                };
                arguments.drain(index..index + 2);
            }
            Some("--portrait") => {
                presentation.portrait = true;
                arguments.remove(index);
            }
            Some("--landscape") => {
                presentation.portrait = false;
                arguments.remove(index);
            }
            Some("--rotate") => {
                let Some(value) = arguments.get(index + 1) else {
                    return Err("--rotate needs 0, 90, 180 or 270".into());
                };
                let degrees = value
                    .to_str()
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|degrees| matches!(degrees, 0 | 90 | 180 | 270))
                    .ok_or_else(|| "--rotate needs 0, 90, 180 or 270".to_owned())?;
                presentation.portrait = degrees % 180 == 90;
                arguments.drain(index..index + 2);
            }
            Some("--zoom") => {
                let Some(value) = arguments.get(index + 1) else {
                    return Err("--zoom needs fit or a percentage (25..400)".into());
                };
                presentation.zoom = match value.to_str() {
                    Some("fit") => None,
                    Some(value) => Some(
                        value
                            .parse::<u16>()
                            .ok()
                            .filter(|percent| (25..=400).contains(percent))
                            .ok_or_else(|| {
                                "--zoom needs fit or a percentage (25..400)".to_owned()
                            })?,
                    ),
                    None => return Err("--zoom needs fit or a percentage (25..400)".into()),
                };
                arguments.drain(index..index + 2);
            }
            _ => index += 1,
        }
    }

    let launch = match arguments.as_slice() {
        [path] => {
            if path.to_str().is_some_and(|path| path.starts_with("--")) {
                return Err(format!(
                    "unknown option {}\n\n{USAGE}",
                    path.to_string_lossy()
                ));
            }
            Launch::App(PathBuf::from(path))
        }
        [flag, shell, apps @ ..] if flag == "--os" || flag == "--os-smoke" => {
            if apps.is_empty() {
                return Err("--os needs at least one installed-app MBC".into());
            }
            smoke |= flag == "--os-smoke";
            Launch::Os {
                shell: PathBuf::from(shell),
                apps: apps.iter().map(PathBuf::from).collect(),
            }
        }
        [] => return Err(USAGE.to_owned()),
        [flag, ..] if flag.to_str().is_some_and(|flag| flag.starts_with("--")) => {
            return Err(format!(
                "unknown or incomplete option {}\n\n{USAGE}",
                flag.to_string_lossy()
            ));
        }
        _ => return Err(format!("only one launch target may be given\n\n{USAGE}")),
    };
    Ok((smoke, launch, presentation))
}

#[cfg(feature = "native")]
fn create_bridge(
    width: u32,
    height: u32,
    hidden: bool,
    presentation: Presentation,
) -> Result<NativeBridge, String> {
    let bridge = NativeBridge::create(width, height, hidden)?;
    if presentation.portrait {
        bridge.set_orientation(true)?;
    }
    match presentation.zoom {
        Some(percent) => bridge.set_zoom(percent)?,
        None => bridge.set_zoom_fit()?,
    }
    Ok(bridge)
}

/// Headless OS-mode smoke: boot the shell, script an `os.launchIndex(0)` tap,
/// assert the app runtime replaces the shell, script an `os.goBack`, and assert
/// the shell returns. Exercises the shared-bridge runtime switching without a
/// visible window.
#[cfg(feature = "native")]
fn run_os_smoke(
    shell_path: &PathBuf,
    app_paths: &[PathBuf],
    presentation: Presentation,
) -> Result<(), String> {
    if app_paths.is_empty() {
        return Err("smoke OS mode needs at least one app MBC".into());
    }
    let registry = app_paths
        .iter()
        .map(|path| {
            let image = decode_mbc(path)?;
            Ok((
                path.clone(),
                image.metadata.name.clone(),
                image.metadata.icon.clone(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut bridge = create_bridge(800, 480, true, presentation)?;
    let nav = Rc::new(RefCell::new(ShellState {
        apps: registry
            .iter()
            .map(|(_, name, icon)| (name.clone(), icon.clone()))
            .collect(),
        silent: true,
        ..Default::default()
    }));

    let shell_image = decode_mbc(shell_path)?;
    let shell_name = shell_image.metadata.name.clone();
    let mut runtime = boot_runtime(&bridge, shell_image, nav.clone(), Some(&shell_name))?;
    let mut is_shell = true;
    if !bridge.appbar_visible() {
        return Err("smoke OS: shell must show the app bar".into());
    }

    // Launcher tap: the shell's os.launchIndex(0).
    nav.borrow_mut().pending_launch = Some(0);
    host_iteration(&mut runtime)?;
    let launch = nav.borrow_mut().pending_launch.take();
    if launch == Some(0) {
        let (path, name, _) = &registry[0];
        drop(runtime);
        runtime = boot_runtime(&bridge, decode_mbc(path)?, nav.clone(), Some(name))?;
        is_shell = false;
    }
    if is_shell {
        return Err("smoke OS: launcher tap did not switch to the app".into());
    }
    if !bridge.appbar_visible() {
        return Err("smoke OS: launched app must show the app bar".into());
    }

    // App back: os.goBack returns to the shell.
    nav.borrow_mut().pending_back = true;
    host_iteration(&mut runtime)?;
    nav.borrow_mut().pending_back = false;
    drop(runtime);
    runtime = boot_runtime(
        &bridge,
        decode_mbc(shell_path)?,
        nav.clone(),
        Some(&shell_name),
    )?;
    is_shell = true;
    if !is_shell {
        return Err("smoke OS: goBack did not return to the shell".into());
    }
    if !bridge.appbar_visible() {
        return Err("smoke OS: returning to the shell must keep the app bar".into());
    }

    // Home close powers the system view off: the bar/content give way to the
    // "Powered off" surface. Tapping that surface boots Home again.
    bridge.power_off();
    if !bridge.power_off_active() {
        return Err("smoke OS: closing Home must power the system off".into());
    }
    bridge.press_power_on();
    if !bridge.take_power_on() {
        return Err("smoke OS: tapping the off surface must queue power on".into());
    }
    bridge.wake();
    drop(runtime);
    runtime = boot_runtime(
        &bridge,
        decode_mbc(shell_path)?,
        nav.clone(),
        Some(&shell_name),
    )?;
    if !bridge.appbar_visible() {
        return Err("smoke OS: power on must restore the app bar".into());
    }

    let _ = &bridge;
    let _ = &runtime;
    Ok(())
}

#[cfg(feature = "native")]
fn run_single(smoke: bool, path: &PathBuf, presentation: Presentation) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("cannot read MBC: {error}"))?;
    let image = decode(&bytes).map_err(|error| format!("cannot load MBC: {error}"))?;
    let first_button = image.nodes.iter().find_map(|node| match node.kind {
        micro_ir::UiKind::Button => node.on_click.map(|handler| (node.id, handler)),
        _ => None,
    });
    let mut bridge = create_bridge(480, 320, smoke, presentation)?;
    let nav = Rc::new(RefCell::new(ShellState {
        silent: smoke,
        ..Default::default()
    }));
    // Direct single-App mode has no OS shell and no OS app bar; the 480x320
    // window is a debug viewport for the App content alone.
    let mut runtime = boot_runtime(&bridge, image, nav.clone(), None)?;

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

    while bridge.poll() {
        // The platform Device menu is available in single-App debug mode too.
        // Power and restart cold boot the same image; orientation/zoom use the
        // shared presentation APIs.
        while let Some(action) = bridge.take_host_action() {
            match action {
                HostAction::PowerOff => {
                    if !bridge.power_off_active() {
                        bridge.power_off();
                    }
                }
                HostAction::PowerOn => {
                    if bridge.power_off_active() {
                        bridge.wake();
                        drop(runtime);
                        runtime = boot_single_runtime(&bridge, &bytes, nav.clone())?;
                    }
                }
                HostAction::ForceRestart => {
                    if bridge.power_off_active() {
                        bridge.wake();
                    }
                    drop(runtime);
                    runtime = boot_single_runtime(&bridge, &bytes, nav.clone())?;
                }
                HostAction::SetOrientation { portrait } => {
                    bridge.set_orientation(portrait)?;
                }
                HostAction::ZoomFit => bridge.set_zoom_fit()?,
                HostAction::Zoom(percent) => bridge.set_zoom(percent)?,
            }
        }

        // Tapping the "Powered off" surface cold boots the same image.
        if bridge.power_off_active() {
            if bridge.take_power_on() {
                bridge.wake();
                drop(runtime);
                runtime = boot_single_runtime(&bridge, &bytes, nav.clone())?;
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
            continue;
        }

        host_iteration(&mut runtime)?;
        let delay = runtime.renderer_mut().bridge_mut().timer().clamp(1, 16);
        std::thread::sleep(Duration::from_millis(u64::from(delay)));
    }
    Ok(())
}

#[cfg(feature = "native")]
fn boot_single_runtime(
    bridge: &NativeBridge,
    bytes: &[u8],
    nav: Rc<RefCell<ShellState>>,
) -> Result<Runtime<LvglRenderer<NativeBridge>>, String> {
    let image = decode(bytes).map_err(|error| format!("cannot load MBC: {error}"))?;
    boot_runtime(bridge, image, nav, None)
}

/// Boot the OS shell MBC (index 0 of the partition on device) and swap between
/// the shell and installed-app runtimes on `os.launchIndex` / `os.goBack`.
///
/// The SDL/LVGL environment is created once and shared across runtimes via an
/// `Rc`-shared `NativeBridge`; switching runtimes drops the previous renderer
/// (which clears its LVGL object tree) before booting the next image into the
/// same display.
#[cfg(feature = "native")]
fn run_os(
    smoke: bool,
    shell_path: &PathBuf,
    app_paths: &[PathBuf],
    presentation: Presentation,
) -> Result<(), String> {
    let registry = app_paths
        .iter()
        .map(|path| {
            let image = decode_mbc(path)?;
            Ok((
                path.clone(),
                image.metadata.name.clone(),
                image.metadata.icon.clone(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut bridge = create_bridge(800, 480, smoke, presentation)?;
    let nav = Rc::new(RefCell::new(ShellState {
        apps: registry
            .iter()
            .map(|(_, name, icon)| (name.clone(), icon.clone()))
            .collect(),
        silent: smoke,
        ..Default::default()
    }));

    let shell_image = decode_mbc(shell_path)?;
    let shell_name = shell_image.metadata.name.clone();
    let mut runtime = boot_runtime(&bridge, shell_image, nav.clone(), Some(&shell_name))?;
    let mut is_shell = true;
    let mut preview_menu = std::env::var_os("MICRO_HOST_MENU_PREVIEW").is_some();

    while bridge.poll() {
        // Actions from the platform device menu are host commands, not App
        // input. Handle them before the normal runtime tick so power and
        // restart are reflected in the same frame.
        while let Some(action) = bridge.take_host_action() {
            match action {
                HostAction::PowerOff => {
                    if !bridge.power_off_active() {
                        bridge.power_off();
                    }
                }
                HostAction::PowerOn => {
                    if bridge.power_off_active() {
                        bridge.wake();
                        drop(runtime);
                        runtime = boot_runtime(
                            &bridge,
                            decode_mbc(shell_path)?,
                            nav.clone(),
                            Some(&shell_name),
                        )?;
                        is_shell = true;
                    }
                }
                HostAction::ForceRestart => {
                    if bridge.power_off_active() {
                        bridge.wake();
                    }
                    drop(runtime);
                    runtime = boot_runtime(
                        &bridge,
                        decode_mbc(shell_path)?,
                        nav.clone(),
                        Some(&shell_name),
                    )?;
                    is_shell = true;
                }
                HostAction::SetOrientation { portrait } => {
                    bridge.set_orientation(portrait)?;
                }
                HostAction::ZoomFit => bridge.set_zoom_fit()?,
                HostAction::Zoom(percent) => bridge.set_zoom(percent)?,
            }
        }

        // The shell's close control powers the OS view off. A tap on the off
        // surface wakes the same fresh Home that the back gesture would boot.
        if bridge.power_off_active() {
            if bridge.take_power_on() {
                bridge.wake();
                drop(runtime);
                runtime = boot_runtime(
                    &bridge,
                    decode_mbc(shell_path)?,
                    nav.clone(),
                    Some(&shell_name),
                )?;
                is_shell = true;
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
            continue;
        }

        host_iteration(&mut runtime)?;
        if preview_menu {
            preview_menu = false;
            eprintln!("preview open menu: {}", bridge.open_host_menu());
        }

        let launch = nav.borrow_mut().pending_launch.take();
        let close = bridge.take_appbar_close();
        let swipe = bridge.take_swipe_back();
        let back = nav.borrow_mut().pending_back || swipe;
        nav.borrow_mut().pending_back = false;

        if is_shell {
            // Only the bar's close control powers Home off; an edge swipe on
            // Home has nowhere to go, matching the Web and ESP32 hosts.
            if close {
                bridge.power_off();
            } else if let Some(index) = launch {
                let index = index as usize;
                if let Some((path, name, _)) = registry.get(index) {
                    // Drop the shell renderer (clears its LVGL tree) before the
                    // app runtime builds its tree into the same display.
                    drop(runtime);
                    runtime = boot_runtime(&bridge, decode_mbc(path)?, nav.clone(), Some(name))?;
                    is_shell = false;
                }
            }
        } else if back || close {
            drop(runtime);
            runtime = boot_runtime(
                &bridge,
                decode_mbc(shell_path)?,
                nav.clone(),
                Some(&shell_name),
            )?;
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
    chrome_title: Option<&str>,
) -> Result<Runtime<LvglRenderer<NativeBridge>>, String> {
    /* The native simulator reports a fixed connected link, matching the
     * `net.*` host values used by the Apps. */
    bridge.set_app_chrome(
        chrome_title.is_some(),
        chrome_title.unwrap_or(""),
        "connected",
        "micro-demo",
    );
    let renderer = LvglRenderer::new(bridge.clone());
    let silent = nav.borrow().silent;
    let mut host = NativeHost::new_for_app(&image.metadata.id);
    if silent {
        host = host.silent();
    }
    host.nav = nav;
    Runtime::new_with_host(image, renderer, 10_000, Box::new(host))
        .map_err(|error| error.to_string())
}

#[cfg(feature = "native")]
fn host_iteration(runtime: &mut Runtime<LvglRenderer<NativeBridge>>) -> Result<(), String> {
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
            Err(error) => eprintln!("micro runtime event error: {error}"),
        }
    }
    /* Async host requests (net.scanWifi / net.httpGet / os.delay) complete one
     * tick later with the simulated result. */
    runtime.enqueue_host_results();
    let _ = runtime.renderer_mut().bridge_mut().timer();
    Ok(())
}
