import { expect, test } from "@playwright/test";

test("ESP32 simulator launches the real Counter MBC and returns home", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Counter" })).toBeVisible();
  await page.getByRole("button", { name: "Counter" }).click();
  await expect(page.getByText("Count: 0")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 1")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 2")).toBeVisible();
  await page.getByRole("button", { name: "Back" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  const runtimeError = page.locator("[data-runtime-error]");
  await expect(runtimeError).toBeHidden();
  await expect(runtimeError).toHaveText("");
});

test("Counter exclusively fills the device canvas with fixed logical typography", async ({ page }) => {
  await page.goto("/");
  await page.locator("html").evaluate((element) => { element.style.fontSize = "20px"; });
  await expect(page.locator(".launcher-copy p")).toHaveCSS("font-size", "9px");
  await expect(page.locator(".app-grid")).toHaveCSS("column-gap", "10px");

  await page.getByRole("button", { name: "Counter" }).click();

  const systemScreen = page.locator("#system-screen");
  const appShell = page.locator("#app-shell");
  await expect(systemScreen).toBeHidden();
  await expect(appShell).toBeVisible();

  await page.locator("#app-screen").evaluate((screen) => {
    const nested = document.createElement("div");
    nested.hidden = true;
    nested.style.display = "block";
    nested.textContent = "Nested app markup";
    screen.append(nested);
  });
  await expect(page.getByText("Nested app markup")).toBeVisible();

  await expect(page.locator(".micro-text")).toHaveCSS("font-size", "24px");
  await expect(page.locator(".micro-text")).toHaveCSS("line-height", "32px");
  await expect(page.locator(".micro-button")).toHaveCSS("font-size", "14px");
  await expect(page.locator(".micro-button")).toHaveCSS("line-height", "18px");

  const dimensions = await appShell.evaluate((element) => {
    const app = element.getBoundingClientRect();
    const device = document.querySelector("[data-device-screen]");
    return {
      appWidth: app.width,
      appHeight: app.height,
      deviceWidth: device?.clientWidth,
      deviceHeight: device?.clientHeight,
    };
  });
  expect(dimensions.appWidth).toBeCloseTo(dimensions.deviceWidth, 0);
  expect(dimensions.appHeight).toBeCloseTo(dimensions.deviceHeight, 0);

  await page.getByRole("button", { name: "Back" }).click();
  await expect(page.locator(".launcher-copy h2")).toHaveCSS("font-size", "18px");
  await expect(page.locator(".launcher-copy h2")).toHaveCSS("line-height", "24px");
});

test("simulator monitor reflects reducer-backed settings and touch", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Toggle backlight" }).click();
  await expect(page.locator('[data-monitor="backlight"]')).toHaveText("Backlight: Off");
  await page.locator("[data-device-screen]").hover({ position: { x: 200, y: 120 } });
  await expect(page.locator('[data-monitor="touch"]')).toHaveText(/GT911 · \d+, \d+/);
  await page.getByRole("button", { name: "Safe Mode reboot" }).click();
  await expect(page.getByText("Safe Mode")).toBeVisible();
  await expect(page.locator('[data-monitor="state"]')).toHaveText("SafeMode");
});

test("settings scroll within the fixed device viewport", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 900 });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();

  const screen = page.locator("#system-screen");
  await expect(screen).toHaveCSS("overflow-y", "auto");

  const dimensions = await screen.evaluate((element) => ({
    clientHeight: element.clientHeight,
    scrollHeight: element.scrollHeight,
  }));
  expect(dimensions.scrollHeight).toBeGreaterThan(dimensions.clientHeight);

  await page.getByRole("button", { name: "Safe Mode reboot" }).scrollIntoViewIfNeeded();
  await expect(page.getByRole("button", { name: "Safe Mode reboot" })).toBeVisible();
});

test("simulator display disables browser text selection without affecting the monitor", async ({ page }) => {
  await page.goto("/");

  await expect(page.locator("[data-device-screen]")).toHaveCSS("user-select", "none");
  await expect(page.locator("[data-device-screen]")).toHaveCSS("-webkit-user-select", "none");
  await expect(page.locator(".monitor")).not.toHaveCSS("user-select", "none");
  await expect(page.locator(".simulator-header")).not.toHaveCSS("user-select", "none");
});
