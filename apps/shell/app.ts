// micro-os shell — the OS home screen, written as a normal Micro App MBC so the
// same bytes render identically on ESP32 (LVGL), native SDL and the Web Player.
//
// The shell owns the OS launcher: a vertical list of installed apps. The layout
// is written with edge anchors so the same tree works in landscape (800x440
// content) and portrait (480x760 content): the cards stretch to the new width
// and the OS app bar/content container resize around them. There is no
// per-orientation UI tree and no runtime restart.
//
// The MBC UI tree is compile-time static, so the launcher declares one row per
// installed app; the current image installs Counter (index 0), Settings
// (index 1) and momo (index 2). Each row's tappable area is a `ui.surface` with
// an accessible `label`, while the visible title/caption are decorative text
// nodes layered above it (text and icons are pointer-transparent, so taps reach
// the card). App icons come from the shared OS icon catalog through the
// `os.appIcon` manifest value; a legacy single-glyph manifest still works.

app({ id: "shell", name: "Home", icon: "apps" });

ui.mount(
  ui.column([
    ui.place(ui.column([
      // --- page canvas: the warm OS background behind the launcher content.
      // Anchored on all four edges so it fills the content box in either
      // orientation instead of staying at the old 800x440 size.
      ui.place(ui.surface("canvas"), {
        anchor: { left: 0, right: 0, top: 0, bottom: 0 },
      }),

      // --- masthead: shared "apps" glyph + page title + hairline rule.
      ui.place(ui.icon("apps", {
        textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 },
        tone: "muted",
      }), { left: 24, top: 24, width: 24, height: 24 }),
      ui.place(ui.text("Applications", {
        font: "uiSans", size: 24, weight: "regular", lineHeight: 32,
      }), { left: 60, top: 20, width: 360, height: 32 }),
      ui.place(ui.surface("line"), {
        height: 1, anchor: { left: 24, right: 24, top: 76 },
      }),

      // --- app 0: Counter --------------------------------------------------
      ui.place(ui.surface("paper", {
        onClick: () => { os.launchIndex(0); },
        label: "Open Counter",
      }), { top: 108, height: 96, anchor: { left: 24, right: 24 } }),
      ui.place(ui.icon(bind(() => os.appIcon(0)), {
        textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
        tone: "accent", variant: "badge",
      }), { left: 42, top: 124, width: 56, height: 56 }),
      ui.place(ui.text(bind(() => os.appName(0)), {
        font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
      }), { left: 112, top: 124, width: 300, height: 24 }),
      ui.place(ui.text("Quick tally", {
        font: "uiSans", size: 12, weight: "regular", lineHeight: 14,
      }), { left: 112, top: 152, width: 300, height: 18 }),

      // --- app 1: Settings -------------------------------------------------
      ui.place(ui.surface("paper", {
        onClick: () => { os.launchIndex(1); },
        label: "Open Settings",
      }), { top: 216, height: 96, anchor: { left: 24, right: 24 } }),
      ui.place(ui.icon(bind(() => os.appIcon(1)), {
        textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
        tone: "accent", variant: "badge",
      }), { left: 42, top: 232, width: 56, height: 56 }),
      ui.place(ui.text(bind(() => os.appName(1)), {
        font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
      }), { left: 112, top: 232, width: 300, height: 24 }),
      ui.place(ui.text("Apps & device", {
        font: "uiSans", size: 12, weight: "regular", lineHeight: 14,
      }), { left: 112, top: 260, width: 300, height: 18 }),

      // --- app 2: momo -----------------------------------------------------
      ui.place(ui.surface("paper", {
        onClick: () => { os.launchIndex(2); },
        label: "Open momo",
      }), { top: 324, height: 96, anchor: { left: 24, right: 24 } }),
      ui.place(ui.icon(bind(() => os.appIcon(2)), {
        textStyle: { font: "uiSans", size: 32, weight: "regular", lineHeight: 40 },
        tone: "accent", variant: "badge",
      }), { left: 42, top: 340, width: 56, height: 56 }),
      ui.place(ui.text(bind(() => os.appName(2)), {
        font: "uiSans", size: 18, weight: "regular", lineHeight: 24,
      }), { left: 112, top: 340, width: 300, height: 24 }),
      ui.place(ui.text("Daily notes", {
        font: "uiSans", size: 12, weight: "regular", lineHeight: 14,
      }), { left: 112, top: 368, width: 300, height: 18 }),

      // --- footer hint -----------------------------------------------------
      ui.place(ui.text("Tap a card to open an app", {
        font: "uiSans", size: 12, weight: "regular", lineHeight: 14,
      }), { height: 14, width: 360, anchor: { left: 24, bottom: 4 } }),
    ]), { anchor: { left: 0, right: 0, top: 0, bottom: 0 } }),
  ]),
);
