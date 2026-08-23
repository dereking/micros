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

ui.mount(
  ui.tabview([
    {
      title: "counter",
      content: ui.column([
        ui.text("Counter Studio", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }),

        ui.text(
          bind(() => `Count: ${count.value}`),
          { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 },
        ),

        ui.text(bind(() => {
          if (count.value > 0) { return "positive"; }
          if (count.value < 0) { return "negative"; }
          return "zero";
        })),

        ui.button("Add", {
          onClick: () => {
            count.value++;
            presses.value++;
          },
        }),
        ui.button("Reset", {
          onClick: () => {
            count.value = 0;
          },
        }),
        ui.button("Double", {
          onClick: () => {
            count.value = count.value * 2;
            presses.value++;
          },
        }),

        ui.text(bind(() => {
          if (power.value === 1) { return "power: on"; }
          return "power: off";
        })),

        ui.row([
          ui.text(bind(() => `battery: ${level.value / 10}`)),
          ui.progress(bind(() => level.value / 10)),
          ui.button("-", {
            onClick: () => {
              if (power.value === 1) {
                if (level.value > 0) { level.value = level.value - 1; }
              }
            },
          }),
          ui.button("+", {
            onClick: () => {
              if (power.value === 1) {
                if (level.value < 10) { level.value = level.value + 1; }
              }
            },
          }),
        ]),

        ui.switch(bind(() => power.value === 1), {
          onToggle: () => {
            power.value = 1 - power.value;
          },
        }),
      ]),
    },
    {
      title: "inputs",
      content: ui.column([
        ui.input(bind(() => note.value), {
          placeholder: "type a note",
          onChange: (s) => { note.value = s; },
        }),
        ui.text(bind(() => `note: ${note.value}`)),

        ui.slider(bind(() => volume.value), {
          min: 0,
          max: 100,
          onChange: (v) => { volume.value = v; },
        }),
        ui.text(bind(() => `volume: ${volume.value}`)),

        ui.checkbox("alarm", bind(() => alarm.value === 1), {
          onChange: (v) => {
            if (v === true) { alarm.value = 1; } else { alarm.value = 0; }
          },
        }),
        ui.text(bind(() => {
          if (alarm.value === 1) { return "alarm: on"; }
          return "alarm: off";
        })),

        ui.dropdown(["red", "green", "blue"], bind(() => color.value), {
          onChange: (i) => { color.value = i; },
        }),

        ui.roller(["S", "M", "L"], bind(() => size.value), {
          onChange: (i) => { size.value = i; },
        }),
      ]),
    },
    {
      title: "display",
      content: ui.column([
        ui.row([
          ui.led(bind(() => powerOn.value === 1)),
          ui.spinner(bind(() => loading.value === 1)),
          ui.scale(bind(() => gauge.value), { min: 0, max: 100 }),
        ]),
        ui.button("power", {
          onClick: () => { powerOn.value = 1 - powerOn.value; },
        }),
        ui.button("loading", {
          onClick: () => { loading.value = 1 - loading.value; },
        }),
        ui.button("gauge", {
          onClick: () => { gauge.value = (gauge.value + 10) % 100; },
        }),

        ui.place(ui.list([
          { text: "reset count", onClick: () => { count.value = 0; } },
          { text: "double count", onClick: () => { count.value = count.value * 2; } },
          { text: "level up", onClick: () => { if (level.value < 10) { level.value = level.value + 1; } } },
        ]), { align: "bottom" }),
      ]),
    },
  ]),
);
