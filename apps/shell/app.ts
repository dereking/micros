// micro-os shell — the OS home screen, written as a normal Micro App MBC so the
// same bytes render identically on ESP32 (LVGL), native SDL and the Web Player.
//
// The shell owns the OS launcher: a vertical list of installed apps. The MBC UI
// tree is compile-time static, so the launcher declares eight fixed slots; each
// slot binds its icon/name to the host registry and hides itself when no app
// occupies that index. The host registry holds built-in Apps first, followed by
// external Apps discovered on the SD card (`/apps/*.mbc` on ESP32, a simulated
// volume on native/Web). Eight slots match the host launcher limit, so no app
// is silently off-screen.
//
// The layout is written with edge anchors so the same tree works in landscape
// (800x440 content) and portrait (480x760 content): rows stretch to the new
// width and the OS app bar/content container resize around them. Rows are
// compact (44 px) so all eight fit in the 440 px landscape content box.

app({ id: "shell", name: "Home", icon: "apps", capabilities: ["os"] });

ui.mount(
  ui.column([
    ui.place(ui.column([
      // --- page canvas: the warm OS background behind the launcher content.
      ui.place(ui.surface("canvas"), {
        anchor: { left: 0, right: 0, top: 0, bottom: 0 },
      }),

      // --- masthead: shared "apps" glyph + page title + hairline rule.
      ui.place(ui.icon("apps", {
        textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 },
        tone: "muted",
      }), { left: 24, top: 10, width: 24, height: 24 }),
      ui.place(ui.text("Applications", {
        font: "uiSans", size: 24, weight: "regular", lineHeight: 32,
      }), { left: 60, top: 6, width: 360, height: 32 }),
      ui.place(ui.surface("line"), {
        height: 1, anchor: { left: 24, right: 24, top: 52 },
      }),

      // --- installed app slot 0 -------------------------------------------
      // Each slot is a group (card + icon + name) hidden when its registry
      // entry is empty. Rows start at y=60, are 44 px tall, 3 px apart.
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(0); },
          label: "Open Counter",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(0);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(0)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(0) !== "")), {
        top: 60, height: 44, anchor: { left: 24, right: 24 },
      }),

      // --- installed app slot 1 -------------------------------------------
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(1); },
          label: "Open Settings",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(1);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(1)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(1) !== "")), {
        top: 107, height: 44, anchor: { left: 24, right: 24 },
      }),

      // --- installed app slot 2 -------------------------------------------
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(2); },
          label: "Open momo",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(2);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(2)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(2) !== "")), {
        top: 154, height: 44, anchor: { left: 24, right: 24 },
      }),

      // --- installed app slot 3 (first external app, when present) --------
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(3); },
          label: "Open app 4",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(3);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(3)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(3) !== "")), {
        top: 201, height: 44, anchor: { left: 24, right: 24 },
      }),

      // --- installed app slot 4 -------------------------------------------
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(4); },
          label: "Open app 5",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(4);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(4)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(4) !== "")), {
        top: 248, height: 44, anchor: { left: 24, right: 24 },
      }),

      // --- installed app slot 5 -------------------------------------------
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(5); },
          label: "Open app 6",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(5);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(5)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(5) !== "")), {
        top: 295, height: 44, anchor: { left: 24, right: 24 },
      }),

      // --- installed app slot 6 -------------------------------------------
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(6); },
          label: "Open app 7",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(6);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(6)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(6) !== "")), {
        top: 342, height: 44, anchor: { left: 24, right: 24 },
      }),

      // --- installed app slot 7 -------------------------------------------
      ui.place(ui.show(ui.column([
        ui.place(ui.surface("paper", {
          onClick: () => { os.launchIndex(7); },
          label: "Open app 8",
        }), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
        ui.place(ui.icon(bind(() => {
          const icon = os.appIcon(7);
          if (icon === "") { return "apps"; }
          return icon;
        }), {
          textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
          tone: "accent", variant: "badge",
        }), { left: 12, top: 2, width: 40, height: 40 }),
        ui.place(ui.text(bind(() => os.appName(7)), {
          font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
        }), { top: 10, height: 24, anchor: { left: 64, right: 12 } }),
      ]), bind(() => os.appName(7) !== "")), {
        top: 389, height: 44, anchor: { left: 24, right: 24 },
      }),
    ]), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
  ]),
);
