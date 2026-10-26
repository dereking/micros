import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  createDeviceShell,
  fitDeviceCanvas,
  getDeviceSize,
  mapTouch,
  setDeviceOrientation,
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
  const ipaFont = await readFile(
    new URL(
      "../../products/micro-web-player/public/fonts/micro-ui-ipa.woff2",
      import.meta.url,
    ),
  );

  assert.match(css, /@font-face\s*{[^}]*font-family:\s*"MicroUiSans";/s);
  assert.match(css, /src:\s*url\("\/fonts\/micro-ui-sans-common\.woff2"\) format\("woff2"\);/);
  assert.match(css, /@font-face\s*{[^}]*font-family:\s*"MicroUiIpa";/s);
  assert.match(css, /src:\s*url\("\/fonts\/micro-ui-ipa\.woff2"\) format\("woff2"\);/);
  assert.match(css, /"MicroUiSans", "MicroUiIpa", sans-serif/);
  assert.match(css, /font-weight:\s*400;/);
  assert.match(css, /\.micro-tab-title[^}]*font:\s*400 14px\/18px[^}]*"MicroUiIpa"/s);
  assert.match(css, /\.micro-text[^}]*white-space:\s*pre-wrap;/s);
  assert.match(css, /\.micro-button[^}]*white-space:\s*pre-wrap;/s);
  assert.match(css, /\.device-canvas[^}]*grid-template-rows:\s*40px minmax\(0, 1fr\)/s);
  assert.match(css, /\.device-canvas[^}]*width:\s*800px;[^}]*height:\s*480px;/s);
  assert.match(css, /\.host-bar[^}]*grid-column:\s*1 \/ -1;/s);
  assert.match(css, /dialog\.host-menu/);
  assert.match(css, /\.device-canvas\[data-orientation="portrait"\][^}]*width:\s*480px;[^}]*height:\s*800px;/s);
  assert.match(css, /\.app-content[^}]*position:\s*relative;/s);
  assert.match(css, /\.app-bar[^}]*height:\s*40px;/s);
  assert.match(css, /\.app-bar[^}]*background:\s*#e7eae4;/s);
  assert.match(css, /\.app-bar[^}]*grid-template-columns:\s*96px minmax\(0, 1fr\) 96px;/s);
  assert.match(css, /\.app-bar-title[^}]*text-align:\s*center;/s);
  assert.match(css, /\.app-bar-wifi[^}]*width:\s*28px;[^}]*height:\s*28px;/s);
  assert.match(css, /\.app-bar-tooltip[^}]*visibility:\s*hidden;/s);
  assert.match(css, /\.app-bar-status\[data-open="true"\]\s+\.app-bar-tooltip[^}]*visibility:\s*visible;/s);
  assert.match(css, /\.app-bar-clock[^}]*font-variant-numeric:\s*tabular-nums;/s);
  assert.match(css, /\.app-back[^}]*width:\s*28px;[^}]*height:\s*28px;/s);
  assert.match(css, /\.app-back::before[^}]*inset:\s*-8px;/s);
  assert.match(css, /\.device-off[^}]*position:\s*absolute;[^}]*inset:\s*0;/s);
  assert.match(css, /\.device-off[^}]*background:\s*#101820;/s);
  assert.ok(font.length > 0);
  assert.ok(ipaFont.length > 0);
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


test("orientation switches the logical canvas and touch mapping", () => {
  assert.deepEqual(getDeviceSize(), { width: 800, height: 480 });
  assert.deepEqual(
    mapTouch(
      { clientX: 210, clientY: 120 },
      { left: 10, top: 20, width: 400, height: 240 },
    ),
    { x: 400, y: 200 },
  );

  assert.deepEqual(setDeviceOrientation("portrait"), { width: 480, height: 800 });
  assert.deepEqual(getDeviceSize(), { width: 480, height: 800 });
  assert.deepEqual(
    mapTouch(
      { clientX: 210, clientY: 120 },
      { left: 10, top: 20, width: 400, height: 240 },
    ),
    { x: 240, y: 333 },
  );
  assert.deepEqual(
    mapTouch(
      { clientX: 900, clientY: -20 },
      { left: 10, top: 20, width: 400, height: 240 },
    ),
    { x: 479, y: 0 },
  );

  const canvas = { style: {} };
  fitDeviceCanvas(canvas, { width: 480, height: 800 });
  assert.equal(canvas.style.transform, "scale(1)");
  fitDeviceCanvas(canvas, { width: 240, height: 300 });
  assert.equal(canvas.style.transform, "scale(0.375)");

  // Restore the default for any later test in this process.
  setDeviceOrientation("landscape");
});
