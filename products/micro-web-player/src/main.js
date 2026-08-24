import init, { MicroWebRuntime, decode_app_metadata } from "./generated/micro_web.js";
import { BOARD_MONITOR, fitDeviceCanvas, mapTouch } from "./device-shell.js";
import { createRuntimeLoop } from "./runtime-loop.js";
import "./style.css";

// The OS shell is itself an MBC (apps/shell/app.ts), so the Web Player boots it
// resident and only creates an app runtime when the shell's `os.launchIndex`
// fires. The shell reads the installed-app registry (decoded from each app's
// MBC metadata) through `os.appName/Icon` and boots apps via `os.launchIndex`;
// apps return to the shell via `os.goBack`. This mirrors the ESP32 firmware.
const SHELL_URL = "/apps/shell.mbc";
const APP_URLS = ["/apps/counter.mbc", "/apps/settings.mbc"];

// Edge-swipe back gesture (Android gesture-nav style): a drag that starts
// within EDGE_ZONE px of the left/right edge of the 800-wide canvas and moves
// inward past SWIPE_THRESHOLD returns to the shell while an app is running.
const EDGE_ZONE = 64;
const SWIPE_THRESHOLD = 80;
let swipe = null; // { edge: "left"|"right", startX, startY, dx, dy }

const device = document.querySelector("[data-device-screen]");
const deviceCanvas = document.querySelector("[data-device-canvas]");
const systemScreen = document.querySelector("#system-screen");
const appShell = document.querySelector("#app-shell");
const appScreen = document.querySelector("#app-screen");

let registry = []; // { url, name, icon } decoded from the installed app MBCs
let shellRuntime, shellLoop;
let appRuntime, appLoop;

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
  // Reveal the target screen before materializing: the delphi layout engine
  // reads clientWidth/Height during create, which is 0 inside a display:none
  // subtree — a hidden container collapses every left+right stretch to width 0.
  appShell.hidden = true;
  systemScreen.hidden = false;
  shellRuntime = new MicroWebRuntime(
    "system-screen",
    bytes,
    BigInt(BOARD_MONITOR.instructionBudget),
    registry.map((app) => `${app.name}|${app.icon}`).join("\n"),
  );
  shellLoop = loopFor(shellRuntime, (shell) => {
    const launch = shell.take_nav_launch();
    if (launch >= 0) launchApp(launch);
  });
  shellLoop.start();
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
      appRuntime = new MicroWebRuntime("app-screen", bytes, BigInt(BOARD_MONITOR.instructionBudget), "");
      appLoop = loopFor(appRuntime, (app) => {
        if (app.take_nav_back()) goBackToShell();
      });
      appLoop.start();
    })
    .catch(reportError);
}

function goBackToShell() {
  appLoop?.stop();
  appRuntime?.dispose();
  appRuntime = undefined;
  appScreen.replaceChildren();
  appShell.hidden = true;
  bootShell().catch(reportError);
}

async function start() {
  await init();

  // Decode each installed app's manifest so the shell's launcher registry is
  // derived from the MBC metadata, not a hardcoded list.
  for (const url of APP_URLS) {
    const meta = decode_app_metadata(await fetchMbc(url)).split("|");
    registry.push({ url, name: meta[0], icon: meta[1] });
  }

  document.querySelector(".app-back").addEventListener("click", goBackToShell);
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
  set("display", `RGB565 · 800×480 · 16 MHz`);
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
  const edge = point.x < EDGE_ZONE ? "left" : point.x > 800 - EDGE_ZONE ? "right" : null;
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
  const output = document.querySelector("[data-runtime-error]");
  output.hidden = false;
  output.textContent = String(error);
}

window.addEventListener("pagehide", () => { shellLoop?.stop(); appLoop?.stop(); });
start().catch(reportError);
