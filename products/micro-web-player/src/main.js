import init, { MicroWebRuntime, MicroWebSystem } from "./generated/micro_web.js";
import { BOARD_MONITOR, createDeviceShell, mapTouch } from "./device-shell.js";
import { createRuntimeLoop } from "./runtime-loop.js";
import "./style.css";

let runtime; let runtimeLoop; let shell;
const device = document.querySelector("[data-device-screen]");
const systemScreen = document.querySelector("#system-screen");
const appShell = document.querySelector("#app-shell");
const appScreen = document.querySelector("#app-screen");

async function start() {
  await init();
  shell = createDeviceShell({ system: new MicroWebSystem(), startRuntime, stopRuntime, render });
  systemScreen.addEventListener("click", onSystemClick);
  document.querySelector(".app-back").addEventListener("click", () => shell.intent("back"));
  device.addEventListener("pointermove", updateTouch);
  device.addEventListener("pointerdown", updateTouch);
  render(shell.snapshot());
}
async function startRuntime() {
  const response = await fetch("/apps/counter.mbc");
  if (!response.ok) throw new Error(`MBC download failed: ${response.status}`);
  systemScreen.hidden = true; appShell.hidden = false;
  runtime = new MicroWebRuntime("app-screen", new Uint8Array(await response.arrayBuffer()), BigInt(BOARD_MONITOR.instructionBudget));
  runtimeLoop = createRuntimeLoop({ runtime, requestFrame: requestAnimationFrame, cancelFrame: cancelAnimationFrame, onError: reportError });
  runtimeLoop.start();
}
function stopRuntime() { runtimeLoop?.stop(); runtime?.dispose(); runtimeLoop = undefined; runtime = undefined; appScreen.replaceChildren(); appShell.hidden = true; }
function onSystemClick(event) { const button = event.target.closest("button[data-intent]"); if (button) shell.intent(button.dataset.intent).catch(reportError); }
function updateTouch(event) { const touch = mapTouch(event, device.getBoundingClientRect()); if (touch) document.querySelector('[data-monitor="touch"]').textContent = `GT911 · ${touch.x}, ${touch.y}`; }
function render(snapshot) {
  const set = (name, value) => { document.querySelector(`[data-monitor="${name}"]`).textContent = value; };
  set("display", `RGB565 · 800×480 · 16 MHz`); set("memory", `8 MiB Flash · 8 MiB PSRAM`); set("touch", document.querySelector('[data-monitor="touch"]').textContent || "GT911 · awaiting touch"); set("backlight", `Backlight: ${snapshot.backlight}`); set("wifi", snapshot.wifi); set("state", snapshot.screen); set("runtime", `FIFO · ${BOARD_MONITOR.instructionBudget.toLocaleString()} instructions`); set("expander", "CH422G · EXIO1 reset / EXIO2 light"); set("last-action", snapshot.last_action);
  document.querySelector("[data-action-log]").replaceChildren(...snapshot.actions.slice(-6).reverse().map((action) => { const item = document.createElement("li"); item.textContent = action; return item; }));
  if (snapshot.screen.startsWith("AppRunning") || snapshot.screen.startsWith("AppStarting")) return;
  appShell.hidden = true; systemScreen.hidden = false; systemScreen.innerHTML = systemMarkup(snapshot);
}
function systemMarkup(snapshot) {
  if (snapshot.screen === "Launcher") return `<div class="screen-topline">MICRO OS <span>Launcher</span></div><div class="launcher-copy"><p>Good morning</p><h2>Choose a runtime</h2></div><div class="app-grid"><button data-intent="open-counter"><b>01</b><strong>Counter</strong><small>TS → MBC → Rust</small></button><button data-intent="open-settings"><b>02</b><strong>Settings</strong><small>Board &amp; network</small></button></div>`;
  if (snapshot.screen === "Settings") return `<div class="screen-topline"><button class="text-button" data-intent="back">← Back</button><span>Settings</span></div><div class="settings-copy"><p>System controls</p><h2>Device layer</h2></div><div class="settings-actions"><button data-intent="backlight-toggle">Toggle backlight</button><button data-intent="wifi-scan">Scan Wi-Fi</button><button data-intent="wifi-connect">Connect Wi-Fi</button><button data-intent="wifi-connected">Link connected</button><button data-intent="wifi-persisted">Persist network</button><button class="danger" data-intent="safe-mode">Safe Mode reboot</button></div>`;
  if (snapshot.screen === "SafeMode") return `<div class="safe-screen"><p>RECOVERY BOOT</p><h2>Safe Mode</h2><span>Apps are locked until a normal reboot.</span></div>`;
  return `<div class="safe-screen"><p>MICRO OS</p><h2>${escapeHtml(snapshot.screen)}</h2><button data-intent="back">Return</button></div>`;
}
function escapeHtml(value) { return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;"); }
function reportError(error) { runtimeLoop?.stop(); const output = document.querySelector("[data-runtime-error]"); output.hidden = false; output.textContent = String(error); }
window.addEventListener("pagehide", stopRuntime); start().catch(reportError);
