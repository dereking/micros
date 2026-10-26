// micro-os Settings — the OS settings surface, written as a normal Micro App
// MBC so the same bytes render identically on ESP32 (LVGL), native SDL and the
// Web Player.
//
// Information architecture follows a mobile OS: a small set of top-level
// categories, each a scrollable page of grouped setting rows. The platform has
// no in-app navigation stack (the OS app bar owns Back, and per-node
// `ui.show` visibility is button-only on the device hosts), so the categories
// are a root `ui.tabview` instead of pushed detail pages — the same pattern
// momo uses.
//
//   Network  — live Wi-Fi status, scan results, manual connect
//   Display  — panel backlight (device.setBacklight), appearance placeholders
//   Sound    — speech rate (audio.setRate / speak), volume placeholders
//   Security — lock screen / biometrics / permissions placeholders
//   System   — storage card status, battery / update / time / locale placeholders
//   About    — hardware facts from `device.*`, license / privacy placeholders
//
// Every capability the OS does not expose yet is an explicit placeholder row:
// a dimmed glyph plus a "Coming soon" line where a live value would sit. There
// are no dead controls that look interactive but do nothing.
//
// Layout is pure Delphi ltwh + anchors (no flex): cards stretch with the page
// width, rows keep a fixed 56 px rhythm, and the tab page owns vertical
// scrolling. Metrics used below:
//
//   page margin 20 | card pad 16 | icon badge 40 | text column 68
//   row 56 | section header 14, +22 to the card | card bottom +18 to the next
//
// Text boxes stretch (left+right anchors) instead of carrying fixed widths, so
// the same tree reflows when the content box narrows (the native host's
// portrait mode) instead of spilling past the cards.
//
// State is only what the UI really drives: the backlight level, the speech-rate
// slider, the Wi-Fi form, the last scan trigger and a boot guard that samples
// persisted values plus host facts exactly once.

app({
  id: "settings",
  name: "Settings",
  icon: "settings",
  capabilities: ["audio", "config", "device", "net", "os", "storage"],
});

// --- reactive state -------------------------------------------------------
// `booted` makes the first binding a one-shot initialiser: it samples the
// hardware backlight, the saved speech rate and the current SSID before any
// other binding paints. `tick` is bumped after Wi-Fi actions so the status
// bindings re-read the host (host reads are not reactive on their own).
// `scanList` is the redraw trigger for the scanned-AP name bindings.
const booted = state(0);
const bright = state(3);
const rate10 = state(9);
const ssid = state("");
const pass = state("");
const scanList = state("");
const tick = state(0);

ui.mount(
  ui.tabview([
    {
      title: "Network",
      content: ui.column([
        // One-shot initialiser (1x1, invisible). It is the first node of the
        // first tab, so every later binding already sees restored values.
        ui.place(
          ui.text(
            bind(() => {
              if (booted.value === 0) {
                bright.value = device.backlight();
                let saved = config.getNumber("rate10");
                if (saved >= 5) {
                  if (saved <= 20) {
                    rate10.value = saved;
                  }
                }
                ssid.value = net.wifiSsid();
                booted.value = 1;
              }
              return "";
            }),
            { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 },
          ),
          { left: 0, top: 0, width: 1, height: 1 },
        ),

        // --- Wi-Fi hero: the live state line, re-read after every action.
        ui.place(ui.surface("paper"), { top: 18, height: 104, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("wifi", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 34, width: 40, height: 40 }),
        ui.place(ui.text("Wi-Fi", { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }), { top: 32, height: 32, anchor: { left: 88, right: 36 } }),
        ui.place(
          ui.text(bind(() => {
            tick.value;
            let wifi = net.wifiState();
            if (wifi === "connected") {
              let name = net.wifiSsid();
              if (name === "") {
                return "Connected";
              }
              return `Connected to ${name}`;
            }
            if (wifi === "connecting") {
              return "Connecting to a network";
            }
            if (wifi === "error") {
              return "Connection failed";
            }
            return "Not connected";
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 68, height: 18, anchor: { left: 88, right: 36 } },
        ),
        ui.place(ui.text("Scan for nearby networks, then pick one below.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 92, height: 14, anchor: { left: 88, right: 36 } }),

        // --- available networks: Scan sits on the section line, results as tiles.
        ui.place(ui.text("AVAILABLE NETWORKS", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 140, height: 14, anchor: { left: 36, right: 124 } }),
        ui.place(
          ui.button("Scan", {
            onClick: () => {
              net.scanWifi((list) => {
                scanList.value = list;
                tick.value = tick.value + 1;
              });
            },
            textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
          }),
          { top: 132, width: 92, height: 36, anchor: { right: 20 } },
        ),

        // AP slot 1: always present so the list keeps its shape on
        // hosts without per-node visibility; it reads empty until a scan fills it.
        ui.place(
          ui.surface("paper", { onClick: () => {
            let found = net.wifiApName(0);
            if (found !== "") {
              ssid.value = found;
            }
          }, label: "Use network 1" }),
          { top: 162, height: 52, anchor: { left: 20, right: 20 } },
        ),
        ui.place(ui.icon("wifi", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted", variant: "badge" }), { left: 36, top: 174, width: 28, height: 28 }),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            let found = net.wifiApName(0);
            if (found === "") {
              return "No network found";
            }
            return found;
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 172, height: 18, anchor: { left: 68, right: 62 } },
        ),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            if (net.wifiApName(0) === "") {
              return "Run Scan to look for networks";
            }
            return "Tap to use this name";
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 190, height: 14, anchor: { left: 68, right: 62 } },
        ),
        ui.place(ui.icon("right", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted" }), { top: 179, width: 18, height: 18, anchor: { right: 34 } }),

        // AP slot 2: always present so the list keeps its shape on
        // hosts without per-node visibility; it reads empty until a scan fills it.
        ui.place(
          ui.surface("paper", { onClick: () => {
            let found = net.wifiApName(1);
            if (found !== "") {
              ssid.value = found;
            }
          }, label: "Use network 2" }),
          { top: 222, height: 52, anchor: { left: 20, right: 20 } },
        ),
        ui.place(ui.icon("wifi", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted", variant: "badge" }), { left: 36, top: 234, width: 28, height: 28 }),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            let found = net.wifiApName(1);
            if (found === "") {
              return "No network found";
            }
            return found;
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 232, height: 18, anchor: { left: 68, right: 62 } },
        ),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            if (net.wifiApName(1) === "") {
              return "Run Scan to look for networks";
            }
            return "Tap to use this name";
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 250, height: 14, anchor: { left: 68, right: 62 } },
        ),
        ui.place(ui.icon("right", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted" }), { top: 239, width: 18, height: 18, anchor: { right: 34 } }),

        // AP slot 3: always present so the list keeps its shape on
        // hosts without per-node visibility; it reads empty until a scan fills it.
        ui.place(
          ui.surface("paper", { onClick: () => {
            let found = net.wifiApName(2);
            if (found !== "") {
              ssid.value = found;
            }
          }, label: "Use network 3" }),
          { top: 282, height: 52, anchor: { left: 20, right: 20 } },
        ),
        ui.place(ui.icon("wifi", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted", variant: "badge" }), { left: 36, top: 294, width: 28, height: 28 }),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            let found = net.wifiApName(2);
            if (found === "") {
              return "No network found";
            }
            return found;
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 292, height: 18, anchor: { left: 68, right: 62 } },
        ),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            if (net.wifiApName(2) === "") {
              return "Run Scan to look for networks";
            }
            return "Tap to use this name";
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 310, height: 14, anchor: { left: 68, right: 62 } },
        ),
        ui.place(ui.icon("right", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted" }), { top: 299, width: 18, height: 18, anchor: { right: 34 } }),

        // AP slot 4: always present so the list keeps its shape on
        // hosts without per-node visibility; it reads empty until a scan fills it.
        ui.place(
          ui.surface("paper", { onClick: () => {
            let found = net.wifiApName(3);
            if (found !== "") {
              ssid.value = found;
            }
          }, label: "Use network 4" }),
          { top: 342, height: 52, anchor: { left: 20, right: 20 } },
        ),
        ui.place(ui.icon("wifi", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted", variant: "badge" }), { left: 36, top: 354, width: 28, height: 28 }),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            let found = net.wifiApName(3);
            if (found === "") {
              return "No network found";
            }
            return found;
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 352, height: 18, anchor: { left: 68, right: 62 } },
        ),
        ui.place(
          ui.text(bind(() => {
            scanList.value;
            if (net.wifiApName(3) === "") {
              return "Run Scan to look for networks";
            }
            return "Tap to use this name";
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 370, height: 14, anchor: { left: 68, right: 62 } },
        ),
        ui.place(ui.icon("right", { textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }, tone: "muted" }), { top: 359, width: 18, height: 18, anchor: { right: 34 } }),

        // --- manual setup: SSID + password, then connect / disconnect.
        ui.place(ui.text("CONNECT MANUALLY", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 412, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 434, height: 172, anchor: { left: 20, right: 20 } }),
        ui.place(
          ui.input(bind(() => ssid.value), {
            placeholder: "Network name (SSID)",
            textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
            onChange: (s) => { ssid.value = s; },
          }),
          { top: 448, height: 40, anchor: { left: 36, right: 36 } },
        ),
        ui.place(
          ui.input(bind(() => pass.value), {
            placeholder: "Password",
            textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
            onChange: (s) => { pass.value = s; },
          }),
          { top: 496, height: 40, anchor: { left: 36, right: 36 } },
        ),
        ui.place(
          ui.button("Connect", {
            onClick: () => {
              net.wifiConnect(ssid.value, pass.value);
              tick.value = tick.value + 1;
              os.delay(1500, (result) => { tick.value = tick.value + 1; });
            },
            textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
          }),
          { left: 36, top: 550, width: 200, height: 42 },
        ),
        ui.place(
          ui.button("Disconnect", {
            onClick: () => {
              net.wifiDisconnect();
              tick.value = tick.value + 1;
            },
            textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 },
          }),
          { top: 550, width: 160, height: 42, anchor: { right: 36 } },
        ),
        ui.place(ui.text("Wi-Fi drives the real radio on the device; the Web Player answers with simulated networks.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 622, height: 30, anchor: { left: 36, right: 36 } }),
      ]),
    },
    {
      title: "Display",
      content: ui.column([
        // --- backlight: the one display control the device really exposes.
        ui.place(ui.surface("paper"), { top: 18, height: 140, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("eye", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 30, width: 40, height: 40 }),
        ui.place(ui.text("Brightness", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }), { top: 38, height: 24, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            bright.value;
            return `${bright.value} / 4`;
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 41, width: 120, height: 18, anchor: { right: 44 } },
        ),
        ui.place(
          ui.slider(bind(() => bright.value), {
            min: 0,
            max: 4,
            onChange: (level) => {
              bright.value = level;
              device.setBacklight(level);
            },
          }),
          { top: 88, height: 36, anchor: { left: 44, right: 44 } },
        ),
        ui.place(ui.text("Applies to the panel through device.setBacklight.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 130, height: 14, anchor: { left: 44, right: 44 } }),

        // --- appearance placeholders.
        ui.place(ui.text("APPEARANCE", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 176, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 198, height: 168, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("eye", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 206, width: 40, height: 40 }),
        ui.place(ui.text("Dark mode", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 217, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 219, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 254, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("image", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 262, width: 40, height: 40 }),
        ui.place(ui.text("Wallpaper", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 273, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 275, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 310, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("edit", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 318, width: 40, height: 40 }),
        ui.place(ui.text("Text size", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 329, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 331, width: 160, height: 14, anchor: { right: 44 } }),

        // --- screen placeholders.
        ui.place(ui.text("SCREEN", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 384, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 406, height: 112, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("power", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 414, width: 40, height: 40 }),
        ui.place(ui.text("Auto brightness", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 425, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 427, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 462, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("refresh", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 470, width: 40, height: 40 }),
        ui.place(ui.text("Screen timeout", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 481, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 483, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.text("Only brightness is wired to the panel; the rest waits on OS display support.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 534, height: 30, anchor: { left: 36, right: 36 } }),
      ]),
    },
    {
      title: "Sound",
      content: ui.column([
        // --- speech rate: audio.setRate is the one audio knob every host shares.
        ui.place(ui.surface("paper"), { top: 18, height: 196, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("volume", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 30, width: 40, height: 40 }),
        ui.place(ui.text("Speech rate", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }), { top: 38, height: 24, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            rate10.value;
            return `${rate10.value / 10}x`;
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 41, width: 120, height: 18, anchor: { right: 44 } },
        ),
        ui.place(
          ui.slider(bind(() => rate10.value), {
            min: 5,
            max: 20,
            onChange: (level) => {
              rate10.value = level;
              config.setNumber("rate10", level);
            },
          }),
          { top: 88, height: 36, anchor: { left: 44, right: 44 } },
        ),
        ui.place(ui.text("Used by audio.speak: 0.5x is slow, 2x is fast.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 130, height: 14, anchor: { left: 44, right: 44 } }),
        ui.place(
          ui.button("Preview voice", { onClick: () => {
            audio.setRate(rate10.value / 10);
            audio.speak("Micro OS settings. This is the speech rate preview.");
          }, textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 } }),
          { left: 44, top: 158, width: 196, height: 42 },
        ),

        // --- volume placeholders: no mixer / volume host call exists yet.
        ui.place(ui.text("VOLUME", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 232, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 254, height: 168, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("volume", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 262, width: 40, height: 40 }),
        ui.place(ui.text("Media volume", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 273, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 275, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 310, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("music", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 318, width: 40, height: 40 }),
        ui.place(ui.text("Ringtone", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 329, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 331, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 366, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("mute", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 374, width: 40, height: 40 }),
        ui.place(ui.text("Do not disturb", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 385, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 387, width: 160, height: 14, anchor: { right: 44 } }),

        // --- feedback placeholders.
        ui.place(ui.text("FEEDBACK", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 440, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 462, height: 112, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("bell", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 470, width: 40, height: 40 }),
        ui.place(ui.text("Touch sounds", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 481, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 483, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 518, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("keyboard", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 526, width: 40, height: 40 }),
        ui.place(ui.text("Haptic feedback", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 537, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 539, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.text("Only speech rate is wired; volume and haptics need a mixer and a vibration driver.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 590, height: 30, anchor: { left: 36, right: 36 } }),
      ]),
    },
    {
      title: "Security",
      content: ui.column([
        // --- lock screen and credentials: the OS has no credential store yet.
        ui.place(ui.text("SECURITY", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 18, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 40, height: 168, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("eye", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 48, width: 40, height: 40 }),
        ui.place(ui.text("Screen lock", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 59, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 61, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 96, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("check", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 104, width: 40, height: 40 }),
        ui.place(ui.text("Biometric unlock", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 115, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 117, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 152, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("save", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 160, width: 40, height: 40 }),
        ui.place(ui.text("Device encryption", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 171, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 173, width: 160, height: 14, anchor: { right: 44 } }),

        // --- privacy placeholders: no per-App permission model yet.
        ui.place(ui.text("PRIVACY", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 226, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 248, height: 168, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("list", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 256, width: 40, height: 40 }),
        ui.place(ui.text("App permissions", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 267, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 269, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 304, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("location", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 312, width: 40, height: 40 }),
        ui.place(ui.text("Location access", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 323, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 325, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 360, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("trash", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 368, width: 40, height: 40 }),
        ui.place(ui.text("Clear app data", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 379, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 381, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.text("Lock screens, biometrics and per-App permissions need an OS credential store.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 434, height: 30, anchor: { left: 36, right: 36 } }),
      ]),
    },
    {
      title: "System",
      content: ui.column([
        // --- device: storage is a real host read, the rest is planned.
        ui.place(ui.text("DEVICE", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 18, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 40, height: 224, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("drive", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 48, width: 40, height: 40 }),
        ui.place(ui.text("Storage card", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 50, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            if (storage.available()) {
              return "Mounted - app data on the removable card";
            }
            return "Not inserted - app data unavailable";
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 70, height: 14, anchor: { left: 88, right: 36 } },
        ),
        ui.place(ui.surface("line"), { top: 96, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("bell", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 104, width: 40, height: 40 }),
        ui.place(ui.text("Notifications", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 115, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 117, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 152, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("battery", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 160, width: 40, height: 40 }),
        ui.place(ui.text("Battery", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 171, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 173, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 208, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("download", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 216, width: 40, height: 40 }),
        ui.place(ui.text("Software update", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 227, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 229, width: 160, height: 14, anchor: { right: 44 } }),

        // --- time & language placeholders: no locale / clock-setting API yet.
        ui.place(ui.text("TIME & LANGUAGE", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 282, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 304, height: 112, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("refresh", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 312, width: 40, height: 40 }),
        ui.place(ui.text("Date & time", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 323, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 325, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 360, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("keyboard", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 368, width: 40, height: 40 }),
        ui.place(ui.text("Language & input", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 379, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 381, width: 160, height: 14, anchor: { right: 44 } }),

        // --- maintenance placeholders: reset / update hooks are not exposed.
        ui.place(ui.text("MAINTENANCE", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 434, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 456, height: 112, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("trash", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 464, width: 40, height: 40 }),
        ui.place(ui.text("Reset options", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 475, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 477, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 512, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("edit", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 520, width: 40, height: 40 }),
        ui.place(ui.text("Developer options", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 531, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 533, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.text("System actions appear once the OS exposes update, reset and locale hooks.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 584, height: 30, anchor: { left: 36, right: 36 } }),
      ]),
    },
    {
      title: "About",
      content: ui.column([
        // --- identity hero: host-reported name + chip, no invented strings.
        ui.place(ui.surface("sage"), { top: 18, height: 104, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("settings", { textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 }, tone: "accent", variant: "badge" }), { left: 40, top: 38, width: 48, height: 48 }),
        ui.place(ui.text("Micro OS", { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }), { top: 36, height: 32, anchor: { left: 96, right: 36 } }),
        ui.place(
          ui.text(bind(() => {
            return `${device.name()} - ${device.chip()}`;
          }), { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }),
          { top: 68, height: 18, anchor: { left: 96, right: 36 } },
        ),
        ui.place(ui.text("One MBC binary, three hosts.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 90, height: 14, anchor: { left: 96, right: 36 } }),

        // --- hardware facts straight from the host (`device.*`).
        ui.place(ui.text("HARDWARE", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 140, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 162, height: 280, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("list", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 170, width: 40, height: 40 }),
        ui.place(ui.text("Chip", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 181, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            return device.chip();
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 183, width: 160, height: 14, anchor: { right: 44 } },
        ),
        ui.place(ui.surface("line"), { top: 218, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("drive", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 226, width: 40, height: 40 }),
        ui.place(ui.text("Flash", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 237, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            return `${device.flashBytes() / 1048576} MB`;
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 239, width: 160, height: 14, anchor: { right: 44 } },
        ),
        ui.place(ui.surface("line"), { top: 274, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("save", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 282, width: 40, height: 40 }),
        ui.place(ui.text("PSRAM", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 293, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            return `${device.psramBytes() / 1048576} MB`;
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 295, width: 160, height: 14, anchor: { right: 44 } },
        ),
        ui.place(ui.surface("line"), { top: 330, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("refresh", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 338, width: 40, height: 40 }),
        ui.place(ui.text("Reset reason", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 349, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            return device.resetReason();
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 351, width: 160, height: 14, anchor: { right: 44 } },
        ),
        ui.place(ui.surface("line"), { top: 386, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("eye", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "accent", variant: "badge" }), { left: 36, top: 394, width: 40, height: 40 }),
        ui.place(ui.text("Backlight", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 405, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(
          ui.text(bind(() => {
            bright.value;
            return `${device.backlight()} / 4`;
          }), { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }),
          { top: 407, width: 160, height: 14, anchor: { right: 44 } },
        ),

        // --- legal placeholders: the OS has no license/privacy surface yet.
        ui.place(ui.text("LEGAL", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 460, height: 14, anchor: { left: 36, right: 36 } }),
        ui.place(ui.surface("paper"), { top: 482, height: 112, anchor: { left: 20, right: 20 } }),
        ui.place(ui.icon("file", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 490, width: 40, height: 40 }),
        ui.place(ui.text("Open-source licenses", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 501, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 503, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.surface("line"), { top: 538, height: 1, anchor: { left: 88, right: 20 } }),
        ui.place(ui.icon("eye", { textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }, tone: "muted", variant: "badge" }), { left: 36, top: 546, width: 40, height: 40 }),
        ui.place(ui.text("Privacy policy", { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 }), { top: 557, height: 18, anchor: { left: 88, right: 216 } }),
        ui.place(ui.text("Coming soon", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 559, width: 160, height: 14, anchor: { right: 44 } }),
        ui.place(ui.text("Hardware values come straight from device.* host calls; nothing here is hardcoded.", { font: "uiSans", size: 12, weight: "regular", lineHeight: 14 }), { top: 610, height: 30, anchor: { left: 36, right: 36 } }),
      ]),
    },
  ]),
);
