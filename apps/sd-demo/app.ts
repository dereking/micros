// Sample external App for the SD-card `/apps` directory.
//
// It is deliberately not part of the built-in registry: copy
// `apps/sd-demo/dist/sd-demo.mbc` onto a card as `/apps/sd-demo.mbc` (or serve
// it under `/sdcard/apps/` in the Web Player) to see an external program appear
// after the installed Apps. Keep it tiny; the launcher and runtime policy are
// the subject here, not the UI.

app({ id: "sd-demo", name: "SD Demo", icon: "folder", capabilities: [] });

ui.mount(
  ui.column([
    ui.place(ui.surface("canvas"), {
      anchor: { left: 0, right: 0, top: 0, bottom: 0 },
    }),
    ui.place(ui.text("SD Demo", {
      font: "uiSans", size: 24, weight: "regular", lineHeight: 32,
    }), { left: 32, top: 32, width: 360, height: 32 }),
    ui.place(ui.text("Loaded from SD", {
      font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
    }), { left: 32, top: 84, width: 360, height: 24 }),
  ]),
);
