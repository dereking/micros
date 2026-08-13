import init, { MicroWebRuntime } from "./generated/micro_web.js";
import "./style.css";

let runtime;
let frameHandle;

async function start() {
  await init();
  const response = await fetch("/apps/counter.mbc");
  if (!response.ok) throw new Error(`MBC download failed: ${response.status}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  runtime = new MicroWebRuntime("micro-device", bytes, 10_000n);

  function frame() {
    runtime.tick();
    frameHandle = requestAnimationFrame(frame);
  }
  frameHandle = requestAnimationFrame(frame);
}

function reportError(error) {
  if (frameHandle !== undefined) cancelAnimationFrame(frameHandle);
  const output = document.querySelector("[data-runtime-error]");
  output.hidden = false;
  output.textContent = String(error);
}

window.addEventListener("pagehide", () => {
  if (frameHandle !== undefined) cancelAnimationFrame(frameHandle);
  runtime?.dispose();
});

start().catch(reportError);
