import { expect, test } from "@playwright/test";

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

  // Shell launcher typography (icon tile is 32px, name label is 12px).
  const iconTile = page.locator("#system-screen").getByRole("button", { name: "C", exact: true });
  await expect(iconTile).toHaveCSS("font-size", "32px");
  await expect(page.getByText("Counter")).toHaveCSS("font-size", "12px");

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

test("shell exposes Home / Settings / WiFi tabs and the monitor reflects touch", async ({ page }) => {
  await page.goto("/");

  // The shell MBC's tabview is its page navigation.
  await expect(page.locator("#system-screen")).toBeVisible();
  await expect(page.getByRole("button", { name: "Home" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await expect(page.getByRole("button", { name: "WiFi" })).toBeVisible();

  // The board monitor reports device-level statics + live touch coordinates.
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
