// micro-os shell — the OS home screen, written as a normal Micro App MBC so the
// same bytes render identically on ESP32 (LVGL), native SDL and the Web Player.
//
// The shell owns the OS UI: a status bar, an icon-grid launcher, and Settings /
// Wi-Fi config pages. It reads OS state through `os.*` / `net.*` / `device.*`
// host calls and commands the OS through `os.launchIndex` / `os.goBack`.
//
// Because the MBC UI tree is compile-time static, the launcher renders a fixed
// grid of slots: each slot's label binds to `os.appName(i)` / `os.appIcon(i)`
// and its (static) tap handler calls `os.launchIndex(i)`. Empty slots render a
// blank tile. Page navigation uses a tabview (Home / Settings / Wi-Fi) as the
// bottom-level nav, matching what the static tree can express.

app({ id: "shell", name: "Home", icon: "H" });

// Wi-Fi state is dynamic and arrives via async host callbacks, so the shell
// mirrors it into states that bindings can depend on.
const wifiState = state("off");
const wifiSsid = state("");
const scanList = state("");
const ssid = state("");
const pass = state("");

ui.mount(
  ui.column([
    // --- status bar (full width, 40px, top) ---
    ui.place(ui.row([
      ui.place(ui.text(bind(() => {
        if (wifiSsid.value !== "") { return `${wifiState.value} ${wifiSsid.value}`; }
        return wifiState.value;
      }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { left: 12, top: 10, width: 260, height: 20 }),
      ui.place(ui.text("micro-os", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
        { left: 190, top: 10, width: 100, height: 20 }),
    ]), { height: 40, anchor: { left: 0, right: 0, top: 0 } }),

    ui.place(ui.tabview([
      {
        title: "Home",
        content: ui.column([
          // Icon grid: 4 columns x 2 rows, tile 96x52 + name below.
          // Slot k -> os.appName/Icon/launchIndex(k).
          ui.place(ui.button(bind(() => os.appIcon(0)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(0); },
          }), { left: 24, top: 8, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(0)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 24, top: 64, width: 96, height: 20 }),
          ui.place(ui.button(bind(() => os.appIcon(1)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(1); },
          }), { left: 136, top: 8, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(1)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 136, top: 64, width: 96, height: 20 }),
          ui.place(ui.button(bind(() => os.appIcon(2)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(2); },
          }), { left: 248, top: 8, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(2)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 248, top: 64, width: 96, height: 20 }),
          ui.place(ui.button(bind(() => os.appIcon(3)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(3); },
          }), { left: 360, top: 8, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(3)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 360, top: 64, width: 96, height: 20 }),
          ui.place(ui.button(bind(() => os.appIcon(4)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(4); },
          }), { left: 24, top: 112, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(4)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 24, top: 168, width: 96, height: 20 }),
          ui.place(ui.button(bind(() => os.appIcon(5)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(5); },
          }), { left: 136, top: 112, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(5)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 136, top: 168, width: 96, height: 20 }),
          ui.place(ui.button(bind(() => os.appIcon(6)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(6); },
          }), { left: 248, top: 112, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(6)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 248, top: 168, width: 96, height: 20 }),
          ui.place(ui.button(bind(() => os.appIcon(7)), {
            textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
            onClick: () => { os.launchIndex(7); },
          }), { left: 360, top: 112, width: 96, height: 52 }),
          ui.place(ui.text(bind(() => os.appName(7)), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { left: 360, top: 168, width: 96, height: 20 }),
        ]),
      },
      {
        title: "Settings",
        content: ui.column([
          ui.place(ui.text(bind(() => `chip: ${device.chip()}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 20, anchor: { left: 0, right: 0, top: 4 } }),
          ui.place(ui.text(bind(() => `flash: ${device.flashBytes()} B`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 20, anchor: { left: 0, right: 0, top: 28 } }),
          ui.place(ui.text(bind(() => `psram: ${device.psramBytes()} B`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 20, anchor: { left: 0, right: 0, top: 52 } }),
          ui.place(ui.text(bind(() => `reset: ${device.resetReason()}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 20, anchor: { left: 0, right: 0, top: 76 } }),
          ui.place(ui.text(bind(() => `backlight: ${device.backlight()}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 20, anchor: { left: 0, right: 0, top: 100 } }),
          ui.place(ui.row([
            ui.place(ui.button("dim", {
              onClick: () => { device.setBacklight(1); },
            }), { left: 0, width: 68, height: 40 }),
            ui.place(ui.button("bright", {
              onClick: () => { device.setBacklight(4); },
            }), { left: 76, width: 84, height: 40 }),
          ]), { top: 128, height: 40, anchor: { left: 0, right: 0 } }),
          ui.place(ui.text(bind(() => `wifi: ${wifiState.value} ${wifiSsid.value}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 20, anchor: { left: 0, right: 0, top: 176 } }),
        ]),
      },
      {
        title: "WiFi",
        content: ui.column([
          ui.place(ui.text(bind(() => `wifi: ${wifiState.value} ${wifiSsid.value}`), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
            { height: 24, anchor: { left: 0, right: 0, top: 0 } }),
          ui.place(ui.row([
            ui.place(ui.button("Scan", {
              onClick: () => {
                net.scanWifi((list) => { scanList.value = list; });
              },
            }), { left: 0, width: 88, height: 40 }),
            ui.place(ui.button("Refresh", {
              onClick: () => {
                wifiState.value = net.wifiState();
                wifiSsid.value = net.wifiSsid();
              },
            }), { left: 96, width: 96, height: 40 }),
          ]), { top: 30, height: 40, anchor: { left: 0, right: 0 } }),
          ui.place(ui.text(bind(() => `APs:\n${scanList.value}`), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
            { height: 56, anchor: { left: 0, right: 0, top: 78 } }),
          ui.place(ui.input(bind(() => ssid.value), {
            placeholder: "SSID",
            onChange: (s) => { ssid.value = s; },
          }), { height: 40, anchor: { left: 0, right: 0, top: 140 } }),
          ui.place(ui.input(bind(() => pass.value), {
            placeholder: "Password",
            onChange: (s) => { pass.value = s; },
          }), { height: 40, anchor: { left: 0, right: 0, top: 186 } }),
          ui.place(ui.row([
            ui.place(ui.button("Connect", {
              onClick: () => {
                net.wifiConnect(ssid.value, pass.value);
                wifiState.value = "connecting";
                // One-shot refresh chain so the status updates as the radio
                // transitions to connected/error without a poll loop.
                os.delay(1000, (s) => {
                  wifiState.value = net.wifiState();
                  wifiSsid.value = net.wifiSsid();
                  os.delay(1500, (s) => {
                    wifiState.value = net.wifiState();
                    wifiSsid.value = net.wifiSsid();
                  });
                });
              },
            }), { left: 0, width: 110, height: 40 }),
            ui.place(ui.button("Disconnect", {
              onClick: () => {
                net.wifiDisconnect();
                wifiState.value = net.wifiState();
                wifiSsid.value = net.wifiSsid();
              },
            }), { left: 118, width: 140, height: 40 }),
          ]), { top: 232, height: 40, anchor: { left: 0, right: 0 } }),
        ]),
      },
    ]), { height: 280, anchor: { left: 0, right: 0, top: 40 } }),
  ]),
);
