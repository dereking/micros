// Counter Studio — Micro App example showcasing every SDK widget.
// The demo is a tabview: one tab per widget family, so each group fits on
// screen without scrolling a giant column (a very tall scrollable column
// overwhelms the LVGL refresh on the ESP32-S3 display).

const count = state(0);
const presses = state(0);
const level = state(3);
const power = state(0);
const note = state("micro");
const volume = state(50);
const alarm = state(0);
const color = state(0);
const size = state(1);
const powerOn = state(0);
const loading = state(0);
const gauge = state(50);
const wifiList = state("");
const httpRes = state("");

ui.mount(
  ui.tabview([
    {
      title: "counter",
      content: ui.column([
        ui.place(ui.text("Counter Studio", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }),
          { height: 24, anchor: { left: 0, right: 0, top: 0 } }),

        ui.place(ui.text(
          bind(() => `Count: ${count.value}`),
          { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 },
        ), { height: 32, anchor: { left: 0, right: 0, top: 30 } }),

        ui.place(ui.text(bind(() => {
          if (count.value > 0) { return "positive"; }
          if (count.value < 0) { return "negative"; }
          return "zero";
        })), { height: 32, anchor: { left: 0, right: 0, top: 68 } }),

        ui.place(ui.button("Add", {
          onClick: () => {
            count.value++;
            presses.value++;
          },
        }), { width: 200, height: 40, anchor: { left: 0, top: 106 } }),
        ui.place(ui.button("Reset", {
          onClick: () => {
            count.value = 0;
          },
        }), { width: 200, height: 40, anchor: { left: 0, top: 152 } }),
        ui.place(ui.button("Double", {
          onClick: () => {
            count.value = count.value * 2;
            presses.value++;
          },
        }), { width: 200, height: 40, anchor: { left: 0, top: 198 } }),

        ui.place(ui.text(bind(() => {
          if (power.value === 1) { return "power: on"; }
          return "power: off";
        })), { height: 32, anchor: { left: 0, right: 0, top: 244 } }),

        ui.place(ui.row([
          ui.place(ui.text(bind(() => `battery: ${level.value / 10}`)),
            { left: 0, width: 150, height: 40 }),
          ui.place(ui.progress(bind(() => level.value / 10)),
            { top: 24, height: 12, anchor: { left: 160, right: 106 } }),
          ui.place(ui.button("-", {
            onClick: () => {
              if (power.value === 1) {
                if (level.value > 0) { level.value = level.value - 1; }
              }
            },
          }), { width: 40, height: 40, anchor: { right: 56 } }),
          ui.place(ui.button("+", {
            onClick: () => {
              if (power.value === 1) {
                if (level.value < 10) { level.value = level.value + 1; }
              }
            },
          }), { width: 40, height: 40, anchor: { right: 0 } }),
        ]), { height: 60, anchor: { left: 0, right: 0, top: 282 } }),

        ui.place(ui.switch(bind(() => power.value === 1), {
          onToggle: () => {
            power.value = 1 - power.value;
          },
        }), { width: 52, height: 30, anchor: { left: 0, top: 348 } }),
      ]),
    },
    {
      title: "inputs",
      content: ui.column([
        ui.place(ui.input(bind(() => note.value), {
          placeholder: "type a note",
          onChange: (s) => { note.value = s; },
        }), { height: 40, anchor: { left: 0, right: 0, top: 0 } }),
        ui.place(ui.text(bind(() => `note: ${note.value}`)),
          { height: 32, anchor: { left: 0, right: 0, top: 46 } }),

        ui.place(ui.slider(bind(() => volume.value), {
          min: 0,
          max: 100,
          onChange: (v) => { volume.value = v; },
        }), { height: 40, anchor: { left: 0, right: 0, top: 84 } }),
        ui.place(ui.text(bind(() => `volume: ${volume.value}`)),
          { height: 32, anchor: { left: 0, right: 0, top: 130 } }),

        ui.place(ui.checkbox("alarm", bind(() => alarm.value === 1), {
          onChange: (v) => {
            if (v === true) { alarm.value = 1; } else { alarm.value = 0; }
          },
        }), { height: 40, anchor: { left: 0, top: 168 } }),
        ui.place(ui.text(bind(() => {
          if (alarm.value === 1) { return "alarm: on"; }
          return "alarm: off";
        })), { height: 32, anchor: { left: 0, right: 0, top: 214 } }),

        ui.place(ui.dropdown(["red", "green", "blue"], bind(() => color.value), {
          onChange: (i) => { color.value = i; },
        }), { height: 40, anchor: { left: 0, right: 0, top: 252 } }),

        ui.place(ui.roller(["S", "M", "L"], bind(() => size.value), {
          onChange: (i) => { size.value = i; },
        }), { height: 90, anchor: { left: 0, right: 0, top: 298 } }),
      ]),
    },
    {
      title: "display",
      content: ui.column([
        ui.place(ui.row([
          ui.place(ui.led(bind(() => powerOn.value === 1)),
            { left: 0, top: 70, width: 20, height: 20 }),
          ui.place(ui.spinner(bind(() => loading.value === 1)),
            { left: 36, top: 56, width: 48, height: 48 }),
          ui.place(ui.scale(bind(() => gauge.value), { min: 0, max: 100 }),
            { left: 100, top: 20, width: 120, height: 120 }),
        ]), { height: 160, anchor: { left: 0, right: 0, top: 0 } }),
        ui.place(ui.row([
          ui.place(ui.button("power", {
            onClick: () => { powerOn.value = 1 - powerOn.value; },
          }), { left: 0, top: 20, width: 100, height: 40 }),
          ui.place(ui.button("loading", {
            onClick: () => { loading.value = 1 - loading.value; },
          }), { left: 116, top: 20, width: 100, height: 40 }),
          ui.place(ui.button("gauge", {
            onClick: () => { gauge.value = (gauge.value + 10) % 100; },
          }), { left: 232, top: 20, width: 100, height: 40 }),
        ]), { top: 166, height: 80, anchor: { left: 0, right: 0, top: 166 } }),
        ui.place(ui.list([
          { text: "reset count", onClick: () => { count.value = 0; } },
          { text: "double count", onClick: () => { count.value = count.value * 2; } },
          { text: "level up", onClick: () => { if (level.value < 10) { level.value = level.value + 1; } } },
        ]), { height: 131, anchor: { left: 0, right: 0, bottom: 0 } }),
      ]),
    },
    {
      title: "system",
      content: ui.column([
        ui.place(ui.text(bind(() => `chip: ${device.chip()}`)),
          { height: 24, anchor: { left: 0, right: 0, top: 0 } }),
        ui.place(ui.text(bind(() => `flash: ${device.flashBytes()} B`)),
          { height: 24, anchor: { left: 0, right: 0, top: 28 } }),
        ui.place(ui.text(bind(() => `psram: ${device.psramBytes()} B`)),
          { height: 24, anchor: { left: 0, right: 0, top: 56 } }),
        ui.place(ui.text(bind(() => `reset: ${device.resetReason()}`)),
          { height: 24, anchor: { left: 0, right: 0, top: 84 } }),
        ui.place(ui.text(bind(() => `backlight: ${device.backlight()}`)),
          { height: 24, anchor: { left: 0, right: 0, top: 112 } }),
        ui.place(ui.row([
          ui.place(ui.button("dim", {
            onClick: () => { device.setBacklight(1); },
          }), { left: 0, width: 68, height: 40 }),
          ui.place(ui.button("bright", {
            onClick: () => { device.setBacklight(4); },
          }), { left: 76, width: 84, height: 40 }),
          ui.place(ui.button("scan", {
            onClick: () => {
              net.scanWifi((list) => { wifiList.value = list; });
            },
          }), { left: 168, width: 84, height: 40 }),
          ui.place(ui.button("http", {
            onClick: () => {
              net.httpGet("http://example.com", (res) => { httpRes.value = res; });
            },
          }), { left: 260, width: 84, height: 40 }),
        ]), { top: 144, height: 40, anchor: { left: 0, right: 0 } }),
        ui.place(ui.text(bind(() => `wifi: ${net.wifiState()} ${net.wifiSsid()}`)),
          { height: 24, anchor: { left: 0, right: 0, top: 190 } }),
        ui.place(ui.text(bind(() => `APs: ${wifiList.value}`)),
          { height: 48, anchor: { left: 0, right: 0, top: 218 } }),
        ui.place(ui.text(bind(() => `http: ${httpRes.value}`)),
          { height: 48, anchor: { left: 0, right: 0, top: 270 } }),
      ]),
    },
  ]),
);
