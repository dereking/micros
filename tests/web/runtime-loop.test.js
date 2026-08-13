import assert from "node:assert/strict";
import test from "node:test";

import { createRuntimeLoop } from "../../products/micro-web-player/src/runtime-loop.js";

test("runtime loop reports tick failures and stops scheduling frames", () => {
  const scheduled = [];
  const errors = [];
  const loop = createRuntimeLoop({
    runtime: {
      tick() {
        throw new Error("instruction budget exceeded");
      },
    },
    requestFrame(callback) {
      scheduled.push(callback);
      return scheduled.length;
    },
    cancelFrame() {},
    onError(error) {
      errors.push(error);
    },
  });

  loop.start();
  assert.equal(scheduled.length, 1);
  scheduled.shift()();

  assert.equal(scheduled.length, 0);
  assert.equal(errors.length, 1);
  assert.match(String(errors[0]), /instruction budget exceeded/);
});

test("stopping the runtime loop cancels its pending frame", () => {
  const cancelled = [];
  const loop = createRuntimeLoop({
    runtime: { tick() {} },
    requestFrame() {
      return 42;
    },
    cancelFrame(handle) {
      cancelled.push(handle);
    },
    onError() {},
  });

  loop.start();
  loop.stop();

  assert.deepEqual(cancelled, [42]);
});
