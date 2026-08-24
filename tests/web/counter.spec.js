import { expect, test } from "@playwright/test";

test("edge swipe from the right edge returns an app to the shell", async ({ page }) => {
  await page.goto("/");

  // Boot the Counter app from the launcher.
  const iconTile = page.locator("#system-screen").getByRole("button", { name: "C", exact: true });
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
  await expect(page.getByText("Counter")).toBeVisible();
});

test("shell launcher boots the real Counter MBC and returns home", async ({ page }) => {
  await page.goto("/");

  // The OS shell (itself an MBC) renders the launcher grid: an icon tile for
  // the installed Counter app plus its name label.
  await expect(page.locator("#system-screen")).toBeVisible();
  await expect(page.getByText("Counter")).toBeVisible();
  const iconTile = page.locator("#system-screen").getByRole("button", { name: "C", exact: true });
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
  await expect(page.getByText("Counter")).toBeVisible();
  const runtimeError = page.locator("[data-runtime-error]");
  await expect(runtimeError).toBeHidden();
});

test("Counter app and shell use fixed logical typography on the device canvas", async ({ page }) => {
  await page.goto("/");
  await page.locator("html").evaluate((element) => { element.style.fontSize = "20px"; });

  // Shell launcher typography (icon tile is 32px, name label is 14px).
  const iconTile = page.locator("#system-screen").getByRole("button", { name: "C", exact: true });
  await expect(iconTile).toHaveCSS("font-size", "32px");
  await expect(page.getByText("Counter")).toHaveCSS("font-size", "14px");

  // Boot the Counter app.
  await iconTile.click();
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
  await expect(page.locator("#system-screen").getByRole("button", { name: "C", exact: true })).toBeVisible();
  await expect(page.locator("#system-screen").getByRole("button", { name: "S", exact: true })).toBeVisible();
  await expect(page.getByText("Counter")).toBeVisible();
  await expect(page.getByText("Settings")).toBeVisible();

  // Tapping the Settings tile boots the Settings app, whose pages are tabs
  // (Wi-Fi is one of them).
  await page.locator("#system-screen").getByRole("button", { name: "S", exact: true }).click();
  await expect(page.locator("#system-screen")).toBeHidden();
  await expect(page.locator("#app-shell")).toBeVisible();
  await expect(page.getByRole("button", { name: "Device" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Wi-Fi" })).toBeVisible();

  // Back returns to the launcher grid.
  await page.locator(".app-back").click();
  await expect(page.getByText("Counter")).toBeVisible();
  await expect(page.getByText("Settings")).toBeVisible();
});

test("Settings Wi-Fi tab scans, lists APs, and a tap fills the SSID field", async ({ page }) => {
  await page.goto("/");
  await page.locator("#system-screen").getByRole("button", { name: "S", exact: true }).click();
  await expect(page.locator("#app-shell")).toBeVisible();
  await page.getByRole("button", { name: "Wi-Fi" }).click();

  // Scan exposes the simulator's AP list as buttons.
  await page.getByRole("button", { name: "Scan", exact: true }).click();
  await expect(page.getByRole("button", { name: "micro-demo", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "guest", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "micro-os", exact: true })).toBeVisible();

  // Tapping an AP fills the SSID input; the Device tab shows backlight as a
  // property only (the dim/bright buttons are gone).
  await page.getByRole("button", { name: "guest", exact: true }).click();
  await expect(page.locator("#app-screen input").first()).toHaveValue("guest");
  await page.getByRole("button", { name: "Device" }).click();
  await expect(page.getByText("backlight:")).toBeVisible();
  await expect(page.getByRole("button", { name: "dim" })).toBeHidden();
  await expect(page.getByRole("button", { name: "bright" })).toBeHidden();
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
