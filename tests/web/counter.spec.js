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
