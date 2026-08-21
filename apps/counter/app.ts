// Counter Studio — richer Micro App example.
// Demonstrates multiple states, styled text, template-literal and
// control-flow bindings, and several handlers.

const count = state(0);
const presses = state(0);

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
  ]),
);
