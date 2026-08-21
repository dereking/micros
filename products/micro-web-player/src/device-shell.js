export const BOARD_MONITOR = Object.freeze({
  width: 800,
  height: 480,
  pixelClockHz: 16_000_000,
  touch: "GT911",
  expander: "CH422G",
  flashMiB: 8,
  psramMiB: 8,
  instructionBudget: 10_000,
});

export function mapTouch(event, bounds) {
  if (bounds.width <= 0 || bounds.height <= 0) return null;
  const x = Math.floor(((event.clientX - bounds.left) / bounds.width) * BOARD_MONITOR.width);
  const y = Math.floor(((event.clientY - bounds.top) / bounds.height) * BOARD_MONITOR.height);
  return {
    x: Math.max(0, Math.min(BOARD_MONITOR.width - 1, x)),
    y: Math.max(0, Math.min(BOARD_MONITOR.height - 1, y)),
  };
}

export function fitDeviceCanvas(canvas, bounds) {
  const scale = Math.min(bounds.width / BOARD_MONITOR.width, bounds.height / BOARD_MONITOR.height);
  canvas.style.transform = `scale(${Math.max(0, scale)})`;
}

export function createDeviceShell({ system, startRuntime, stopRuntime, render }) {
  let current = readSnapshot(system);

  async function intent(name) {
    current = readSnapshot(system, name);

    if (name === "open-counter" && current.counter_session != null) {
      await startRuntime();
      current = readSnapshot(system, "counter-started");
    }

    if (name === "back" && current.screen.startsWith("AppStopping")) {
      stopRuntime();
      current = readSnapshot(system, "counter-stopped");
    }

    render(current);
    return current;
  }

  return {
    intent,
    snapshot() {
      return current;
    },
  };
}

function readSnapshot(system, intent) {
  const serialized = intent === undefined ? system.snapshot() : system.dispatch(intent);
  return JSON.parse(serialized);
}
