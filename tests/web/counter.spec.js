import { expect, test } from "@playwright/test";

test("edge swipe from the right edge returns an app to the shell", async ({ page }) => {
  await page.goto("/");

  // Boot the Counter app from the launcher.
  const iconTile = page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true });
  await iconTile.click();
  await expect(page.getByText("Count: 0")).toBeVisible();

  // Swipe inward from the right edge of the device canvas (Android gesture
  // nav style): down near the right edge, drag left past the threshold, lift.
  const box = await page.locator("[data-device-screen]").boundingBox();
  const start = { x: box.x + (770 / 800) * box.width, y: box.y + (100 / 480) * box.height };
  const end = { x: box.x + (690 / 800) * box.width, y: box.y + (100 / 480) * box.height };
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 6 });
  await page.mouse.up();

  // The gesture returns to the shell launcher.
  await expect(page.locator("#system-screen")).toBeVisible();
  await expect(page.locator("#system-screen").getByText("Counter")).toBeVisible();
});

test("the Home shell and every launched App share the default OS app bar", async ({ page }) => {
  await page.goto("/");

  // Home is a normal App under the OS app bar: title "Home", Wi-Fi, clock and
  // the close control, with the launcher grid inset to the 440px content box.
  const appBar = page.locator(".app-bar");
  await expect(appBar).toBeVisible();
  await expect(page.locator("[data-appbar-title]")).toHaveText("Home");
  await expect(page.locator("#system-screen")).toBeVisible();
  await expect(page.getByRole("button", { name: "Close app" })).toBeVisible();

  // The title is centred on the bar itself, not just inside a middle column
  // whose side columns happen to have equal weights.
  const titleOffset = await page.evaluate(() => {
    const title = document.querySelector("[data-appbar-title]");
    const range = document.createRange();
    range.selectNodeContents(title);
    const rect = range.getBoundingClientRect();
    const bar = document.querySelector(".app-bar").getBoundingClientRect();
    return (rect.left + rect.right) / 2 - (bar.left + bar.right) / 2;
  });
  expect(Math.abs(titleOffset)).toBeLessThanOrEqual(1);

  await page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true }).click();

  await expect(appBar).toBeVisible();
  await expect(page.locator("[data-appbar-title]")).toHaveText("Counter");
  const wifi = page.getByRole("button", { name: "Wi-Fi: connected, micro-demo" });
  const wifiTooltip = page.locator("[data-appbar-wifi-tooltip]");
  await expect(wifi).toBeVisible();
  await expect(wifi).toHaveText("");
  await expect(wifi).toHaveAttribute("data-state", "connected");
  await expect(wifiTooltip).toHaveText("micro-demo");
  await expect(wifiTooltip).toBeHidden();

  // Hover and click both reveal the SSID; click keeps it open for touch.
  await wifi.hover();
  await expect(wifiTooltip).toBeVisible();
  await wifi.click();
  await expect(wifi).toHaveAttribute("aria-expanded", "true");
  await expect(wifiTooltip).toBeVisible();
  await expect(page.locator("[data-appbar-clock]")).toHaveText(/^\d{2}:\d{2}$/);

  // The App UI is inset below the bar instead of drawing over it.
  await expect(page.locator("#app-screen")).toHaveCSS("height", "440px");

  // The close control is the app-bar equivalent of the back gesture.
  await page.getByRole("button", { name: "Close app" }).click();
  await expect(page.locator("#system-screen")).toBeVisible();
  await expect(page.locator("[data-appbar-title]")).toHaveText("Home");
  await expect(appBar).toBeVisible();

  // Closing Home powers the system off: the bar/content give way to the
  // full-screen off panel, and tapping that panel boots Home again.
  await page.getByRole("button", { name: "Close app" }).click();
  await expect(page.locator("[data-device-off]")).toBeVisible();
  await expect(page.locator("[data-device-off]")).toContainText("Powered off");
  await expect(appBar).toBeHidden();
  await expect(page.locator("#system-screen")).toBeHidden();

  await page.locator("[data-device-off]").click();
  await expect(page.locator("[data-device-off]")).toBeHidden();
  await expect(appBar).toBeVisible();
  await expect(page.locator("[data-appbar-title]")).toHaveText("Home");
  await expect(page.locator("#system-screen")).toBeVisible();
});

test("shell launcher boots the real Counter MBC and returns home", async ({ page }) => {
  await page.goto("/");

  // The OS shell (itself an MBC) renders the launcher grid: an icon tile for
  // the installed Counter app plus its name label.
  await expect(page.locator("#system-screen")).toBeVisible();
  await expect(page.locator("#system-screen").getByText("Counter")).toBeVisible();
  const iconTile = page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true });
  await expect(iconTile).toBeVisible();

  // Tapping the tile boots the Counter app into the app screen.
  await iconTile.click();
  await expect(page.getByText("Count: 0")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 1")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 2")).toBeVisible();

  // The app-shell Back control returns to the shell launcher.
  await page.locator(".app-back").click();
  await expect(page.locator("#system-screen").getByText("Counter")).toBeVisible();
  const runtimeError = page.locator("[data-runtime-error]");
  await expect(runtimeError).toBeHidden();
});

test("launcher cards are labelled buttons and taps pass through decorative labels", async ({ page }) => {
  await page.goto("/");

  const card = page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true });
  await expect(card).toHaveAttribute("aria-label", "Open Counter");

  // The visible title is a decorative sibling. A real tap at its centre must
  // reach the card underneath instead of being swallowed by the text node.
  const label = page.locator("#system-screen").getByText("Counter", { exact: true });
  const box = await label.boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.getByText("Count: 0")).toBeVisible();

  // Keyboard activation works because the card is a real button.
  await page.locator(".app-back").click();
  await card.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByText("Count: 0")).toBeVisible();
});

test("Counter app and shell use fixed logical typography on the device canvas", async ({ page }) => {
  await page.goto("/");
  await page.locator("html").evaluate((element) => { element.style.fontSize = "20px"; });

  // Shell launcher typography: each card is a labelled tappable surface, the
  // app glyph is a 32 px inline SVG inside a 64 px badge, and the visible
  // app name is an 18 px label.
  const card = page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true });
  await expect(card).toHaveAttribute("aria-label", "Open Counter");
  const badge = page.locator('#system-screen .micro-icon[data-variant="badge"]').first();
  await expect(badge.locator("svg")).toHaveAttribute("width", "32");
  await expect(badge.locator("svg")).toHaveAttribute("height", "32");
  await expect(page.locator("#system-screen").getByText("Counter")).toHaveCSS("font-size", "18px");

  // Boot the Counter app.
  await card.click();
  const systemScreen = page.locator("#system-screen");
  const appShell = page.locator("#app-shell");
  await expect(systemScreen).toBeHidden();
  await expect(appShell).toBeVisible();

  await expect(page.getByText("Counter Studio")).toHaveCSS("font-size", "18px");
  await expect(page.getByText("Counter Studio")).toHaveCSS("line-height", "24px");
  await expect(page.getByText("Count: 0")).toHaveCSS("font-size", "24px");
  await expect(page.getByText("Count: 0")).toHaveCSS("line-height", "32px");
  await expect(page.getByRole("button", { name: "Add" })).toHaveCSS("font-size", "14px");
  await expect(page.getByRole("button", { name: "Add" })).toHaveCSS("line-height", "18px");
});

test("shell launcher is an icon grid; Settings boots as a separate tabbed app", async ({ page }) => {
  await page.goto("/");

  // The shell is a launcher grid directly (no tab bar): Counter and Settings
  // tiles are separate entries.
  await expect(page.locator("#system-screen")).toBeVisible();
  await expect(page.getByRole("button", { name: "Home" })).toBeHidden();
  await expect(page.getByRole("button", { name: "WiFi" })).toBeHidden();
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Settings", exact: true })).toBeVisible();
  await expect(page.locator("#system-screen").getByText("Counter")).toBeVisible();
  await expect(page.locator("#system-screen").getByText("Settings")).toBeVisible();

  // Tapping the Settings tile boots the Settings app. Its top-level categories
  // are the tab bar, the way a phone puts Wi-Fi, display and sound on separate
  // settings screens.
  await page.locator("#system-screen").getByRole("button", { name: "Open Settings", exact: true }).click();
  await expect(page.locator("#system-screen")).toBeHidden();
  await expect(page.locator("#app-shell")).toBeVisible();
  for (const category of ["Network", "Display", "Sound", "Security", "System", "About"]) {
    await expect(page.getByRole("button", { name: category, exact: true })).toBeVisible();
  }

  // Back returns to the launcher grid.
  await page.locator(".app-back").click();
  await expect(page.locator("#system-screen").getByText("Counter")).toBeVisible();
  await expect(page.locator("#system-screen").getByText("Settings")).toBeVisible();
});

test("Settings Network page scans, lists APs, and a tap fills the SSID field", async ({ page }) => {
  await page.goto("/");
  await page.locator("#system-screen").getByRole("button", { name: "Open Settings", exact: true }).click();
  await expect(page.locator("#app-shell")).toBeVisible();
  // Network is the landing category; it is already the active tab.
  await expect(page.getByText("CONNECT MANUALLY")).toBeVisible();

  // Scan turns the simulator's AP list into named tiles. Every tile exists from
  // the first paint (the device host has no per-node visibility), so before the
  // scan they all read "No network found".
  const networkPage = page.locator("#app-screen .micro-tab-page-active");
  await expect(networkPage.getByText("No network found").first()).toBeVisible();
  await page.getByRole("button", { name: "Scan", exact: true }).click();
  await expect(networkPage.getByText("micro-demo", { exact: true })).toBeVisible();
  await expect(networkPage.getByText("guest", { exact: true })).toBeVisible();
  await expect(networkPage.getByText("micro-os", { exact: true })).toBeVisible();

  // Tapping an AP tile fills the manual SSID field at the bottom of the page.
  await page.getByRole("button", { name: "Use network 2", exact: true }).click();
  await expect(page.locator("#app-screen input").first()).toHaveValue("guest");
});

test("Settings Display slider drives the panel backlight", async ({ page }) => {
  await page.goto("/");
  await page.locator("#system-screen").getByRole("button", { name: "Open Settings", exact: true }).click();
  await page.getByRole("button", { name: "Display", exact: true }).click();

  // The slider starts at the level the host reports (the firmware boots at 3).
  // Scope to the active page: inactive tab pages stay in the DOM, so the Sound
  // page's speech-rate slider is also a `.micro-slider`.
  const displayPage = page.locator("#app-screen .micro-tab-page-active");
  const slider = displayPage.locator(".micro-slider");
  await expect(slider).toHaveValue("3");
  await expect(displayPage.getByText("3 / 4")).toBeVisible();

  await slider.fill("1");
  await expect(slider).toHaveValue("1");
  await expect(displayPage.getByText("1 / 4")).toBeVisible();
});

test("board monitor reports device-level statics and live touch coordinates", async ({ page }) => {
  await page.goto("/");

  await expect(page.locator('[data-monitor="display"]')).toHaveText(/RGB565/);
  await page.locator("[data-device-screen]").hover({ position: { x: 200, y: 120 } });
  await expect(page.locator('[data-monitor="touch"]')).toHaveText(/GT911 · \d+, \d+/);
});

test("simulator display disables browser text selection without affecting the monitor", async ({ page }) => {
  await page.goto("/");

  await expect(page.locator("[data-device-screen]")).toHaveCSS("user-select", "none");
  await expect(page.locator("[data-device-screen]")).toHaveCSS("-webkit-user-select", "none");
  await expect(page.locator(".monitor")).not.toHaveCSS("user-select", "none");
  await expect(page.locator(".simulator-header")).not.toHaveCSS("user-select", "none");
});

test("Device menu powers off and on, restarts, and changes orientation", async ({ page }) => {
  await page.goto("/");

  const hostStatus = page.locator("[data-host-status]");
  const menuButton = page.getByRole("button", { name: "Device" });
  const menu = page.getByRole("dialog", { name: "Device" });
  const deviceCanvas = page.locator("[data-device-canvas]");

  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();
  await expect(hostStatus).toContainText("Running · Home");
  await expect(deviceCanvas).toHaveCSS("width", "800px");
  await expect(deviceCanvas).toHaveCSS("height", "480px");

  // The menu reflects the live power state: running can power off or restart,
  // but cannot power on again.
  await menuButton.click();
  await expect(menu).toBeVisible();
  await expect(page.locator("[data-host-menu-status]")).toContainText("Running · Home");
  await expect(page.getByRole("button", { name: "Power Off", exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Power On", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Force Restart", exact: true })).toBeEnabled();

  await page.getByRole("button", { name: "Power Off", exact: true }).click();
  await expect(menu).toBeHidden();
  await expect(page.locator("[data-device-off]")).toBeVisible();
  await expect(hostStatus).toContainText("Powered off");

  // Power On is only offered while off, and it cold boots Home.
  await menuButton.click();
  await expect(page.getByRole("button", { name: "Power Off", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Power On", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Power On", exact: true }).click();
  await expect(page.locator("[data-device-off]")).toBeHidden();
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();
  await expect(hostStatus).toContainText("Running · Home");

  // Force Restart requires a second, explicit click and then lands on Home.
  await menuButton.click();
  await page.getByRole("button", { name: "Force Restart", exact: true }).click();
  await expect(page.getByRole("button", { name: "Confirm Restart", exact: true })).toBeVisible();
  await expect(page.locator("[data-host-restart-hint]")).toBeVisible();
  await page.getByRole("button", { name: "Confirm Restart", exact: true }).click();
  await expect(menu).toBeHidden();
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();
  await expect(hostStatus).toContainText("Running · Home");

  // Orientation is a real profile change: 800x480 -> 480x800 and the App
  // content re-materializes at 760px (800 - the 40px OS bar).
  await menuButton.click();
  await page.locator("[data-host-orientation]").selectOption("portrait");
  await expect(menu).toBeHidden();
  await expect(deviceCanvas).toHaveAttribute("data-orientation", "portrait");
  await expect(deviceCanvas).toHaveCSS("width", "480px");
  await expect(deviceCanvas).toHaveCSS("height", "800px");
  await expect(page.locator(".app-content")).toHaveCSS("height", "760px");
  await expect(page.locator("[data-monitor='display']")).toHaveText(/480 × 800/);
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();

  // Switching back restores the default logical canvas.
  await menuButton.click();
  await page.locator("[data-host-orientation]").selectOption("landscape");
  await expect(menu).toBeHidden();
  await expect(deviceCanvas).toHaveCSS("width", "800px");
  await expect(deviceCanvas).toHaveCSS("height", "480px");
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();
});

test("Device menu is a responsive sheet on a phone viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await expect(page.locator("[data-host-status]")).toContainText("Running · Home");

  const hostBar = await page.locator(".host-bar").boundingBox();
  const screen = await page.locator("[data-device-screen]").boundingBox();
  expect(hostBar.y + hostBar.height).toBeLessThanOrEqual(screen.y + 1);
  expect(screen.y + screen.height).toBeLessThanOrEqual(844);

  await page.getByRole("button", { name: "Device" }).click();
  const menu = page.getByRole("dialog", { name: "Device" });
  await expect(menu).toBeVisible();
  const box = await menu.boundingBox();
  expect(box.width).toBeLessThanOrEqual(390);
  expect(box.width).toBeGreaterThan(300);
  expect(box.y + box.height).toBeGreaterThanOrEqual(840);
  await expect(page.getByRole("button", { name: "Power Off", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Power On", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Force Restart", exact: true })).toBeVisible();

  // Portrait is a real 480x800 profile on a phone too, and still fits the
  // viewport instead of pushing the bottom controls below the fold.
  await page.getByRole("button", { name: "Close device menu" }).click();
  await page.getByRole("button", { name: "Device" }).click();
  await page.locator("[data-host-orientation]").selectOption("portrait");
  await expect(page.locator("[data-device-canvas]")).toHaveCSS("width", "480px");
  await expect(page.locator("[data-device-canvas]")).toHaveCSS("height", "800px");
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();
  const portrait = await page.locator("[data-device-screen]").boundingBox();
  expect(portrait.y + portrait.height).toBeLessThanOrEqual(844);
});

test("Force Restart closes the running App and keeps Home installed Apps", async ({ page }) => {
  await page.goto("/");
  await page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true }).click();
  await expect(page.getByText("Count: 0")).toBeVisible();

  await page.getByRole("button", { name: "Device" }).click();
  await expect(page.locator("[data-host-menu-status]")).toContainText("Running · Counter");
  await page.getByRole("button", { name: "Force Restart", exact: true }).click();
  await page.getByRole("button", { name: "Confirm Restart", exact: true }).click();

  await expect(page.getByRole("dialog", { name: "Device" })).toBeHidden();
  await expect(page.getByText("Count: 0")).toBeHidden();
  await expect(page.locator("[data-host-status]")).toContainText("Running · Home");
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();

  // Power Off from a running App stops its runtime too; Power On boots Home.
  await page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true }).click();
  await expect(page.getByText("Count: 0")).toBeVisible();
  await page.getByRole("button", { name: "Device" }).click();
  await page.getByRole("button", { name: "Power Off", exact: true }).click();
  await expect(page.locator("[data-device-off]")).toBeVisible();
  await page.getByRole("button", { name: "Device" }).click();
  await page.getByRole("button", { name: "Power On", exact: true }).click();
  await expect(page.locator("#system-screen").getByRole("button", { name: "Open Counter", exact: true })).toBeVisible();
  await expect(page.getByText("Count: 0")).toBeHidden();
});
