const count = state(0);

ui.mount(
  ui.column([
    ui.text(bind(() => `Count: ${count.value}`)),
    ui.button("Add", {
      onClick: () => count.value++,
    }),
  ]),
);
