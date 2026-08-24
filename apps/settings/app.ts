// micro-os Settings — the OS settings screen, written as a normal Micro App MBC
// so the same bytes render identically on ESP32 (LVGL), native SDL and the Web
// Player.
//
// The shell launcher (apps/shell) exposes this app as a separate entry in its
// icon grid; tapping it boots this MBC and `os.goBack` returns to the shell.
// A status bar mirrors the shell's, and the settings pages live in a tabview
// that fills the screen below it — Wi-Fi provisioning is one of the tabs.

app({ id: "settings", name: "Settings", icon: "S" });

// Wi-Fi state is dynamic and arrives via async host callbacks. The status bar
// binds to the live `net.*` values; `wifiState` is only a refresh trigger that
// the Connect/Refresh flows re-assign so the bindings re-run after the radio
// transitions (bindings only re-run when a state they read changes).
const wifiState = state("off");
const scanList = state("");
const ssid = state("");
const pass = state("");

ui.mount(
  ui.column([
    // --- status bar (full width, 40px, top): small Wi-Fi status icon + state
    // left, screen title right — mirrors the shell's signal bar. The icon is a
    // state dot (LED) with an animated spinner overlaid while connecting:
    // green = connected, gray = off/error, spinning ring = connecting. ---
    ui.place(ui.row([
      ui.place(ui.spinner(bind(() => {
        wifiState.value;
        return net.wifiState() === "connecting";
      })), { left: 0, top: 10, width: 20, height: 20 }),
      ui.place(ui.led(bind(() => {
        wifiState.value;
        return net.wifiState() === "connected";
      })), { left: 3, top: 13, width: 14, height: 14 }),
      ui.place(ui.text(bind(() => {
        wifiState.value;
        const current = net.wifiSsid();
        if (current !== "") { return current; }
        return net.wifiState();
      }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { left: 30, top: 11, width: 460, height: 18 }),
      ui.place(ui.text("Settings", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
        { left: 680, top: 11, width: 120, height: 18 }),
    ]), { height: 40, anchor: { left: 0, right: 0, top: 0 } }),

    // --- settings pages: tabview fills everything below the status bar. ---
    ui.place(ui.tabview([
      {
        title: "Device",
        content: ui.column([
          ui.place(ui.text(bind(() => `chip: ${device.chip()}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 24, anchor: { left: 0, right: 0, top: 8 } }),
          ui.place(ui.text(bind(() => `flash: ${device.flashBytes()} B`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 24, anchor: { left: 0, right: 0, top: 40 } }),
          ui.place(ui.text(bind(() => `psram: ${device.psramBytes()} B`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 24, anchor: { left: 0, right: 0, top: 72 } }),
          ui.place(ui.text(bind(() => `reset: ${device.resetReason()}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 24, anchor: { left: 0, right: 0, top: 104 } }),
          ui.place(ui.text(bind(() => `backlight: ${device.backlight()}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 24, anchor: { left: 0, right: 0, top: 136 } }),
        ]),
      },
      {
        title: "Wi-Fi",
        content: ui.column([
          // Row 1: Scan + Refresh stacked on the left; the scanned AP list on
          // the right — tapping an AP fills its SSID into the field below.
          // Each row reads `scanList` only to re-run when a scan completes.
          ui.place(ui.button("Scan", {
            onClick: () => { net.scanWifi((list) => { scanList.value = list; }); },
          }), { left: 0, top: 0, width: 96, height: 48 }),
          ui.place(ui.button("Refresh", {
            onClick: () => { wifiState.value = net.wifiState(); },
          }), { left: 0, top: 56, width: 96, height: 48 }),
          ui.place(ui.button(bind(() => {
            scanList.value;
            return net.wifiApName(0);
          }), {
            onClick: () => { ssid.value = net.wifiApName(0); },
          }), { top: 0, height: 32, anchor: { left: 112, right: 0 } }),
          ui.place(ui.button(bind(() => {
            scanList.value;
            return net.wifiApName(1);
          }), {
            onClick: () => { ssid.value = net.wifiApName(1); },
          }), { top: 36, height: 32, anchor: { left: 112, right: 0 } }),
          ui.place(ui.button(bind(() => {
            scanList.value;
            return net.wifiApName(2);
          }), {
            onClick: () => { ssid.value = net.wifiApName(2); },
          }), { top: 72, height: 32, anchor: { left: 112, right: 0 } }),
          ui.place(ui.button(bind(() => {
            scanList.value;
            return net.wifiApName(3);
          }), {
            onClick: () => { ssid.value = net.wifiApName(3); },
          }), { top: 108, height: 32, anchor: { left: 112, right: 0 } }),

          // Row 2: SSID + Password inputs (two columns) with Connect /
          // Disconnect on the right.
          ui.place(ui.input(bind(() => ssid.value), {
            placeholder: "SSID",
            onChange: (s) => { ssid.value = s; },
          }), { left: 0, top: 168, width: 250, height: 48 }),
          ui.place(ui.input(bind(() => pass.value), {
            placeholder: "Password",
            onChange: (s) => { pass.value = s; },
          }), { left: 270, top: 168, width: 250, height: 48 }),
          ui.place(ui.button("Connect", {
            onClick: () => {
              net.wifiConnect(ssid.value, pass.value);
              wifiState.value = "connecting";
              // One-shot refresh chain so the status updates as the radio
              // transitions to connected/error without a poll loop.
              os.delay(1000, (s) => {
                wifiState.value = net.wifiState();
                os.delay(1500, (s) => {
                  wifiState.value = net.wifiState();
                });
              });
            },
          }), { left: 540, top: 168, width: 110, height: 48 }),
          ui.place(ui.button("Disconnect", {
            onClick: () => {
              net.wifiDisconnect();
              wifiState.value = net.wifiState();
            },
          }), { left: 660, top: 168, width: 140, height: 48 }),
        ]),
      },
    ]), { height: 440, anchor: { left: 0, right: 0, top: 40 } }),
  ]),
);
