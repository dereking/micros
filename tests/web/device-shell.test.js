import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  createDeviceShell,
  fitDeviceCanvas,
  mapTouch,
} from "../../products/micro-web-player/src/device-shell.js";

test("web player serves the generated MicroUiSans Regular face", async () => {
  const css = await readFile(
    new URL("../../products/micro-web-player/src/style.css", import.meta.url),
    "utf8",
  );
  const font = await readFile(
    new URL(
      "../../products/micro-web-player/public/fonts/micro-ui-sans-common.woff2",
      import.meta.url,
    ),
  );

  assert.match(css, /@font-face\s*{[^}]*font-family:\s*"MicroUiSans";/s);
  assert.match(css, /src:\s*url\("\/fonts\/micro-ui-sans-common\.woff2"\) format\("woff2"\);/);
  assert.match(css, /font-weight:\s*400;/);
  assert.match(css, /\.micro-text[^}]*white-space:\s*pre-wrap;/s);
  assert.match(css, /\.micro-button[^}]*white-space:\s*pre-wrap;/s);
  assert.match(css, /\.device-canvas[^}]*width:\s*800px;[^}]*height:\s*480px;/s);
  assert.ok(font.length > 0);
});

test("Counter starts after the reducer allocates its session", async () => {
  const calls = [];
  let snapshot = {
    screen: "Launcher",
    counter_session: null,
  };
  const system = {
    dispatch(intent) {
      if (intent === "open-counter") {
        snapshot = { screen: "AppStarting(Counter, 1)", counter_session: 1 };
      }
      if (intent === "counter-started") {
        snapshot = { screen: "AppRunning(Counter, 1)", counter_session: 1 };
      }
      return JSON.stringify(snapshot);
    },
    snapshot() {
      return JSON.stringify(snapshot);
    },
  };
  const shell = createDeviceShell({
    system,
    startRuntime: async () => calls.push("start"),
    stopRuntime: () => calls.push("stop"),
    render() {},
  });

  await shell.intent("open-counter");

  assert.deepEqual(calls, ["start"]);
  assert.equal(shell.snapshot().screen, "AppRunning(Counter, 1)");
});

test("pointer coordinates map into the 800 by 480 screen", () => {
  assert.deepEqual(
    mapTouch(
      { clientX: 210, clientY: 120 },
      { left: 10, top: 20, width: 400, height: 240 },
    ),
    { x: 400, y: 200 },
  );
  assert.deepEqual(
    mapTouch(
      { clientX: 900, clientY: -20 },
      { left: 10, top: 20, width: 400, height: 240 },
    ),
    { x: 799, y: 0 },
  );
});

test("fixed logical canvas scales uniformly into the responsive device bounds", () => {
  const canvas = { style: {} };
  fitDeviceCanvas(canvas, { width: 400, height: 240 });
  assert.equal(canvas.style.transform, "scale(0.5)");

  fitDeviceCanvas(canvas, { width: 300, height: 300 });
  assert.equal(canvas.style.transform, "scale(0.375)");
});
