import assert from "node:assert/strict";
import test from "node:test";

import { createDeviceShell, mapTouch } from "../../products/micro-web-player/src/device-shell.js";

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
