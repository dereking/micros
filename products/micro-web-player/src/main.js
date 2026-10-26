import init, { MicroWebRuntime, decode_app_manifest } from "./generated/micro_web.js";
import {
  loadExternalApps,
  mergeAppRegistry,
  parseManifestString,
} from "./app-registry.js";
import {
  BOARD_MONITOR,
  fitDeviceCanvas,
  getDeviceSize,
  mapTouch,
  setDeviceOrientation,
} from "./device-shell.js";
import { createRuntimeLoop } from "./runtime-loop.js";
import "./style.css";

// The OS shell is itself an MBC (apps/shell/app.ts), so the Web Player boots it
// resident and only creates an app runtime when the shell's `os.launchIndex`
// fires. The shell reads the installed-app registry (decoded from each app's
// MBC metadata) through `os.appName/Icon` and boots apps via `os.launchIndex`;
// apps return to the shell via `os.goBack`. This mirrors the ESP32 firmware.
// External Apps come from the simulated card at `/sdcard/apps` (index.json +
// MBC files); built-ins always win an id collision.
// The host-drawn app bar wraps both: the shell gets the same title/Wi-Fi/clock/
// close chrome as every app, with "Home" as its title.
const SHELL_URL = "/apps/shell.mbc";
const APP_URLS = ["/apps/counter.mbc", "/apps/settings.mbc", "/apps/momo.mbc"];

// Edge-swipe back gesture (Android gesture-nav style): a drag that starts
// within EDGE_ZONE px of the left/right edge of the active logical canvas and
// moves inward past SWIPE_THRESHOLD returns to the shell while an app runs.
const EDGE_ZONE = 64;
const SWIPE_THRESHOLD = 80;
let swipe = null; // { edge: "left"|"right", startX, startY, dx, dy }

const device = document.querySelector("[data-device-screen]");
const deviceCanvas = document.querySelector("[data-device-canvas]");
const systemScreen = document.querySelector("#system-screen");
const appShell = document.querySelector("#app-shell");
const appScreen = document.querySelector("#app-screen");
const appBar = document.querySelector(".app-bar");
const appContent = document.querySelector(".app-content");
const appBarTitle = document.querySelector("[data-appbar-title]");
const appBarStatus = document.querySelector(".app-bar-status");
const appBarWifi = document.querySelector("[data-appbar-wifi]");
const appBarWifiTooltip = document.querySelector("[data-appbar-wifi-tooltip]");
const appBarClock = document.querySelector("[data-appbar-clock]");
const deviceOff = document.querySelector("[data-device-off]");
const hostStatus = document.querySelector("[data-host-status]");
const hostMenuButton = document.querySelector("[data-host-menu]");
const hostMenu = document.querySelector("[data-host-menu-panel]");
const hostClose = document.querySelector("[data-host-close]");
const hostMenuStatus = document.querySelector("[data-host-menu-status]");
const hostStatusText = document.querySelector("[data-host-status-text]");
const hostMenuStatusText = document.querySelector("[data-host-menu-status-text]");
const hostPowerOff = document.querySelector("[data-host-power-off]");
const hostPowerOn = document.querySelector("[data-host-power-on]");
const hostRestart = document.querySelector("[data-host-restart]");
const hostRestartHint = document.querySelector("[data-host-restart-hint]");
const hostOrientation = document.querySelector("[data-host-orientation]");
const deviceSizeLabel = document.querySelector("[data-device-size]");
const hostDisplay = document.querySelector("[data-host-display]");

let registry = []; // { url, id, name, icon, external } from built-ins + simulated SD
let shellRuntime, shellLoop;
let appRuntime, appLoop;
let appBarClockTimer;
let currentView = "Home";
let orientation = "landscape";
let restartArmed = false;
let restartTimer;

function updateAppBarClock() {
  const now = new Date();
  const pad = (value) => String(value).padStart(2, "0");
  appBarClock.textContent = `${pad(now.getHours())}:${pad(now.getMinutes())}`;
}

function setAppBarWifi(state, ssid) {
  appBarWifi.dataset.state = state;
  appBarWifiTooltip.textContent = ssid || state;
  appBarWifi.setAttribute(
    "aria-label",
    ssid ? `Wi-Fi: ${state}, ${ssid}` : `Wi-Fi: ${state}`,
  );
  appBarWifi.setAttribute("aria-expanded", "false");
  appBarStatus.dataset.open = "false";
}

function closeAppBarWifi() {
  appBarStatus.dataset.open = "false";
  appBarWifi.setAttribute("aria-expanded", "false");
}

function showAppBar(app) {
  // The simulator's `net.*` host reports this fixed connected network, the
  // same values the Apps see. The system bar mirrors them.
  appBarTitle.textContent = app.name;
  setAppBarWifi("connected", "micro-demo");
  updateAppBarClock();
  currentView = app.name;
  setHostStatus(`Running · ${app.name}`);
  syncHostMenu();
}

/** Apply a pending `os.setTitle` from the running App; `undefined` keeps it. */
function applyAppBarTitle(runtime) {
  const title = runtime.take_nav_title();
  if (title !== undefined) appBarTitle.textContent = title;
}

function setHostStatus(label, state = "running") {
  hostStatus.dataset.state = state;
  hostMenuStatus.dataset.state = state;
  hostStatusText.textContent = label;
  hostMenuStatusText.textContent = label;
}

/** Refresh the Device dialog without changing the live status line. */
function syncHostMenu() {
  const poweredOff = !deviceOff.hidden;
  hostPowerOff.disabled = poweredOff;
  hostPowerOn.disabled = !poweredOff;
  hostRestart.disabled = poweredOff;
  hostOrientation.value = orientation;
  const size = getDeviceSize();
  const display = `RGB565 · ${size.width} × ${size.height} · 16 MHz`;
  deviceSizeLabel.textContent = `${size.width} × ${size.height}`;
  hostDisplay.textContent = display;
  document.querySelector('[data-monitor="display"]').textContent = display;
}

function resetRestartConfirm() {
  window.clearTimeout(restartTimer);
  restartTimer = undefined;
  restartArmed = false;
  hostRestart.dataset.armed = "false";
  hostRestart.textContent = "Force Restart";
  hostRestartHint.hidden = true;
}

function armRestartConfirm() {
  restartArmed = true;
  hostRestart.dataset.armed = "true";
  hostRestart.textContent = "Confirm Restart";
  hostRestartHint.hidden = false;
  window.clearTimeout(restartTimer);
  restartTimer = window.setTimeout(resetRestartConfirm, 5000);
}

function stopRuntimes() {
  swipe = null;
  shellLoop?.stop();
  appLoop?.stop();
  shellLoop = undefined;
  appLoop = undefined;
  shellRuntime?.dispose();
  appRuntime?.dispose();
  shellRuntime = undefined;
  appRuntime = undefined;
  systemScreen.replaceChildren();
  appScreen.replaceChildren();
}

async function coldBoot() {
  stopRuntimes();
  deviceOff.hidden = true;
  appBar.hidden = false;
  appContent.hidden = false;
  appShell.hidden = true;
  systemScreen.hidden = false;
  closeAppBarWifi();
  await bootShell();
}

function openHostMenu() {
  if (hostMenu.open) return;
  closeAppBarWifi();
  resetRestartConfirm();
  syncHostMenu();
  hostMenu.showModal();
  hostMenuButton.setAttribute("aria-expanded", "true");
  (hostPowerOff.disabled ? hostPowerOn : hostPowerOff).focus();
}

function closeHostMenu() {
  if (hostMenu.open) hostMenu.close();
}

function setMonitorState(value) {
  document.querySelector('[data-monitor="state"]').textContent = value;
}

/** Home is up: the shell screen is revealed and no app shell is showing. */
/** Host power-off: stop whichever runtime is live and cover the panel. */
function powerOff() {
  if (!deviceOff.hidden) return;
  stopRuntimes();
  systemScreen.hidden = true;
  appShell.hidden = true;
  closeAppBarWifi();
  appBar.hidden = true;
  appContent.hidden = true;
  setMonitorState("Powered off");
  deviceOff.hidden = false;
  deviceOff.focus();
  setHostStatus("Powered off", "off");
  syncHostMenu();
}

/** Tapping the powered-off panel (or the menu) boots a fresh Home. */
async function powerOn() {
  if (deviceOff.hidden) return;
  setHostStatus("Booting…", "busy");
  deviceOff.hidden = true;
  appBar.hidden = false;
  appContent.hidden = false;
  try {
    await bootShell();
  } catch (error) {
    reportError(error);
  }
}


async function fetchMbc(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`MBC download failed: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

function loopFor(runtime, afterTick) {
  return createRuntimeLoop({
    runtime: {
      tick: () => {
        runtime.tick();
        afterTick(runtime);
      },
    },
    requestFrame: requestAnimationFrame,
    cancelFrame: cancelAnimationFrame,
    onError: reportError,
  });
}

async function bootShell() {
  const bytes = await fetchMbc(SHELL_URL);
  const meta = parseManifestString(decode_app_manifest(bytes));
  if (!meta) throw new Error("shell MBC has no manifest");
  // The shell is an App too: it sits under the same OS app bar and carries its
  // manifest name ("Home") as the title.
  // Reveal the target screen before materializing: the delphi layout engine
  // reads clientWidth/Height during create, which is 0 inside a display:none
  // subtree — a hidden container collapses every left+right stretch to width 0.
  appShell.hidden = true;
  systemScreen.hidden = false;
  showAppBar({ name: meta.name });
  shellRuntime = new MicroWebRuntime(
    "system-screen",
    bytes,
    BigInt(BOARD_MONITOR.instructionBudget),
    registry.map((app) => `${app.name}|${app.icon}`).join("\n"),
  );
  shellLoop = loopFor(shellRuntime, (shell) => {
    applyAppBarTitle(shell);
    const launch = shell.take_nav_launch();
    if (launch >= 0) launchApp(launch);
  });
  shellLoop.start();
  setMonitorState("OS shell");
}

function launchApp(index) {
  if (index < 0 || index >= registry.length) return;
  shellLoop.stop();
  shellRuntime?.dispose();
  shellRuntime = undefined;
  systemScreen.replaceChildren();
  systemScreen.hidden = true;

  fetchMbc(registry[index].url)
    .then((bytes) => {
      // Reveal before constructing the runtime (see bootShell) so the app's
      // layout engine measures real container widths during materialization.
      appShell.hidden = false;
      showAppBar(registry[index]);
      appRuntime = new MicroWebRuntime("app-screen", bytes, BigInt(BOARD_MONITOR.instructionBudget), "");
      appLoop = loopFor(appRuntime, (app) => {
        applyAppBarTitle(app);
        if (app.take_nav_back()) goBackToShell();
      });
      appLoop.start();
      setMonitorState(registry[index].name);
    })
    .catch(reportError);
}

function goBackToShell() {
  if (!deviceOff.hidden) return;
  // The close control is the app-bar equivalent of the back gesture. On Home
  // it has nowhere further back to go, so it powers the system view off.
  if (appShell.hidden) {
    powerOff();
    return;
  }
  appLoop?.stop();
  appRuntime?.dispose();
  appRuntime = undefined;
  appScreen.replaceChildren();
  appShell.hidden = true;
  bootShell().catch(reportError);
}

async function forceRestart() {
  if (!deviceOff.hidden) return;
  setHostStatus("Restarting…", "busy");
  try {
    await coldBoot();
  } catch (error) {
    reportError(error);
  }
}

/** Orientation is a device profile, not a CSS rotation: switch the logical
 * canvas, then cold-boot the guest so the DOM renderer materializes at the new
 * size. Native reflows in place; the Web renderer's layout is first-paint
 * driven, so a fresh boot is the deterministic equivalent. */
async function changeOrientation(next) {
  if (next !== "landscape" && next !== "portrait") next = "landscape";
  if (next === orientation) return;
  orientation = next;
  setDeviceOrientation(next);
  device.dataset.orientation = next;
  deviceCanvas.dataset.orientation = next;
  syncHostMenu();
  fitDeviceCanvas(deviceCanvas, device.getBoundingClientRect());
  if (deviceOff.hidden) {
    setHostStatus("Restarting…", "busy");
    try {
      await coldBoot();
    } catch (error) {
      reportError(error);
    }
  }
}

async function start() {
  setHostStatus("Booting…", "busy");
  await init();

  // Decode each built-in App's manifest so the shell's launcher registry is
  // derived from the MBC metadata, not a hardcoded list. Then append external
  // Apps from the simulated SD card (`/sdcard/apps`); built-ins win id
  // collisions, matching the ESP32 and native hosts.
  const builtin = [];
  for (const url of APP_URLS) {
    const manifest = parseManifestString(decode_app_manifest(await fetchMbc(url)));
    if (!manifest) throw new Error(`${url} has no manifest`);
    builtin.push({ url, ...manifest });
  }
  const external = await loadExternalApps({
    builtin,
    fetchImpl: (url, init) => fetch(url, init),
    decodeManifest: decode_app_manifest,
  });
  registry = mergeAppRegistry(builtin, external);

  document.querySelector(".app-back").addEventListener("click", goBackToShell);
  deviceOff.addEventListener("click", powerOn);
  hostMenuButton.addEventListener("click", () => {
    if (hostMenu.open) closeHostMenu();
    else openHostMenu();
  });
  hostClose.addEventListener("click", closeHostMenu);
  hostPowerOff.addEventListener("click", () => {
    closeHostMenu();
    powerOff();
  });
  hostPowerOn.addEventListener("click", () => {
    closeHostMenu();
    powerOn();
  });
  hostRestart.addEventListener("click", () => {
    if (!restartArmed) {
      armRestartConfirm();
      return;
    }
    resetRestartConfirm();
    closeHostMenu();
    forceRestart();
  });
  hostOrientation.addEventListener("change", () => {
    const next = hostOrientation.value;
    closeHostMenu();
    changeOrientation(next).catch(reportError);
  });
  hostMenu.addEventListener("close", () => {
    hostMenuButton.setAttribute("aria-expanded", "false");
    resetRestartConfirm();
  });
  hostMenu.addEventListener("click", (event) => {
    if (event.target === hostMenu) closeHostMenu();
  });
  appBarWifi.addEventListener("click", () => {
    const open = appBarStatus.dataset.open === "true";
    appBarStatus.dataset.open = String(!open);
    appBarWifi.setAttribute("aria-expanded", String(!open));
  });
  document.addEventListener("pointerdown", (event) => {
    if (!appBarStatus.contains(event.target)) closeAppBarWifi();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") closeAppBarWifi();
  });
  updateAppBarClock();
  appBarClockTimer = window.setInterval(updateAppBarClock, 1000);
  device.addEventListener("pointermove", updateTouch);
  device.addEventListener("pointerdown", updateTouch);
  device.addEventListener("pointerdown", onSwipeStart);
  device.addEventListener("pointermove", onSwipeMove);
  device.addEventListener("pointerup", onSwipeEnd);
  device.addEventListener("pointercancel", onSwipeEnd);
  const resizeObserver = new ResizeObserver(() => fitDeviceCanvas(deviceCanvas, device.getBoundingClientRect()));
  resizeObserver.observe(device);
  fitDeviceCanvas(deviceCanvas, device.getBoundingClientRect());
  window.addEventListener("pagehide", () => resizeObserver.disconnect(), { once: true });

  renderMonitorStatic();
  await bootShell();
}

function renderMonitorStatic() {
  const set = (name, value) => { document.querySelector(`[data-monitor="${name}"]`).textContent = value; };
  const size = getDeviceSize();
  set("display", `RGB565 · ${size.width}×${size.height} · 16 MHz`);
  set("memory", `8 MiB Flash · 8 MiB PSRAM`);
  set("touch", `GT911 · awaiting touch`);
  set("backlight", `via shell`);
  set("wifi", `via shell`);
  set("state", `OS shell`);
  set("runtime", `FIFO · ${BOARD_MONITOR.instructionBudget.toLocaleString()} instructions`);
  set("expander", "CH422G · EXIO1 reset / EXIO2 light");
}

function updateTouch(event) {
  const touch = mapTouch(event, device.getBoundingClientRect());
  if (touch) document.querySelector('[data-monitor="touch"]').textContent = `GT911 · ${touch.x}, ${touch.y}`;
}

function onSwipeStart(event) {
  // The gesture only navigates while an app is on screen.
  if (appShell.hidden) return;
  const point = mapTouch(event, device.getBoundingClientRect());
  if (!point) return;
  const width = getDeviceSize().width;
  const edge = point.x < EDGE_ZONE ? "left" : point.x > width - EDGE_ZONE ? "right" : null;
  if (!edge) return;
  swipe = { edge, startX: point.x, startY: point.y, dx: 0, dy: 0 };
}

function onSwipeMove(event) {
  if (!swipe) return;
  const point = mapTouch(event, device.getBoundingClientRect());
  if (!point) return;
  swipe.dx = point.x - swipe.startX;
  swipe.dy = point.y - swipe.startY;
}

function onSwipeEnd() {
  if (!swipe) return;
  const gesture = swipe;
  swipe = null;
  const inward = gesture.edge === "left" ? gesture.dx : -gesture.dx;
  if (inward >= SWIPE_THRESHOLD && Math.abs(gesture.dx) >= 2 * Math.abs(gesture.dy)) {
    goBackToShell();
  }
}

function reportError(error) {
  shellLoop?.stop();
  appLoop?.stop();
  closeHostMenu();
  setHostStatus("Runtime error", "error");
  const output = document.querySelector("[data-runtime-error]");
  output.hidden = false;
  output.textContent = String(error);
}

window.addEventListener("pagehide", () => {
  window.clearInterval(appBarClockTimer);
  stopRuntimes();
});
start().catch(reportError);
