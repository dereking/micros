// micro-os shell — the OS home screen, written as a normal Micro App MBC so the
// same bytes render identically on ESP32 (LVGL), native SDL and the Web Player.
//
// The shell owns the OS launcher: a status bar and an icon-grid home screen.
// Settings is a separate installed app (apps/settings) launched from the grid,
// so this file deliberately has no page navigation — the grid is the home.
// It reads OS state through `os.*` / `net.*` host calls and commands the OS
// through `os.launchIndex` / `os.goBack`.
//
// The MBC UI tree is compile-time static, so the launcher declares one tile
// per installed app (button + name label); the current image installs Counter
// (index 0) and Settings (index 1). Empty slots are not rendered at all.

app({ id: "shell", name: "Home", icon: "H" });

ui.mount(
  ui.column([
    // --- status bar (full width, 40px, top): small Wi-Fi status icon + state
    // left, brand right. The icon is a state dot (LED) with an animated
    // spinner overlaid while connecting: green = connected, gray = off/error,
    // spinning ring = connecting. The right label is anchored to keep the two
    // halves from overlapping. ---
    ui.place(ui.row([
      ui.place(ui.spinner(bind(() => net.wifiState() === "connecting")),
        { left: 0, top: 0, width: 20, height: 20 }),
      ui.place(ui.led(bind(() => net.wifiState() === "connected")),
        { left: 3, top: 3, width: 14, height: 14 }),
      ui.place(ui.text(bind(() => {
        // Show the state word ("off" | "connecting" | "error") unless the
        // radio is connected, when the SSID is the useful label.
        const state = net.wifiState();
        if (state !== "connected") { return state; }
        return net.wifiSsid();
      }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
      { left: 30, top: 1, width: 460, height: 18 }),
      ui.place(ui.text("micro-os", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
        { left: 680, top: 1, width: 120, height: 18 }),
    ]), { height: 40, anchor: { left: 0, right: 0, top: 0 } }),

    // --- launcher: fills everything below the status bar. Two tiles, centered.
    // Slot 0 -> Counter, slot 1 -> Settings. ---
    ui.place(ui.column([
      ui.place(ui.button(bind(() => os.appIcon(0)), {
        textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
        onClick: () => { os.launchIndex(0); },
      }), { left: 268, top: 172, width: 120, height: 72 }),
      ui.place(ui.text(bind(() => os.appName(0)), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
        { left: 268, top: 248, width: 120, height: 24 }),
      ui.place(ui.button(bind(() => os.appIcon(1)), {
        textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
        onClick: () => { os.launchIndex(1); },
      }), { left: 412, top: 172, width: 120, height: 72 }),
      ui.place(ui.text(bind(() => os.appName(1)), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
        { left: 412, top: 248, width: 120, height: 24 }),
    ]), { height: 440, anchor: { left: 0, right: 0, top: 40 } }),
  ]),
);
