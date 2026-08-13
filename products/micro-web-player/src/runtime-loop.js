export function createRuntimeLoop({
  runtime,
  requestFrame,
  cancelFrame,
  onError,
}) {
  let frameHandle;
  let stopped = true;

  function frame() {
    if (stopped) return;

    try {
      runtime.tick();
      frameHandle = requestFrame(frame);
    } catch (error) {
      stopped = true;
      frameHandle = undefined;
      onError(error);
    }
  }

  return {
    start() {
      if (!stopped) return;
      stopped = false;
      frameHandle = requestFrame(frame);
    },
    stop() {
      stopped = true;
      if (frameHandle !== undefined) {
        cancelFrame(frameHandle);
        frameHandle = undefined;
      }
    },
  };
}
