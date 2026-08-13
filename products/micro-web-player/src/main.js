import init, { MicroWebRuntime } from "./generated/micro_web.js";
import { createRuntimeLoop } from "./runtime-loop.js";
import "./style.css";

let runtime;
let runtimeLoop;

async function start() {
  await init();
  const response = await fetch("/apps/counter.mbc");
  if (!response.ok) throw new Error(`MBC download failed: ${response.status}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  runtime = new MicroWebRuntime("micro-device", bytes, 10_000n);
  runtimeLoop = createRuntimeLoop({
    runtime,
    requestFrame: requestAnimationFrame,
    cancelFrame: cancelAnimationFrame,
    onError: reportError,
  });
  runtimeLoop.start();
}

function reportError(error) {
  runtimeLoop?.stop();
  const output = document.querySelector("[data-runtime-error]");
  output.hidden = false;
  output.textContent = String(error);
}

window.addEventListener("pagehide", () => {
  runtimeLoop?.stop();
  runtime?.dispose();
});

start().catch(reportError);
