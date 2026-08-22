// Counter Studio — richer Micro App example.
// Demonstrates multiple states, styled text, template-literal and
// control-flow bindings, several handlers, the row/progress/switch widgets,
// and an editable text field (ui.input) with an onChange handler.

const count = state(0);
const presses = state(0);
const level = state(3);
const power = state(0);
const note = state("micro");

ui.mount(
  ui.column([
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

    ui.text(bind(() => `total presses: ${presses.value}`)),

    ui.input(bind(() => note.value), {
      placeholder: "type a note",
      onChange: (s) => { note.value = s; },
    }),
    ui.text(bind(() => `note: ${note.value}`)),

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
);
