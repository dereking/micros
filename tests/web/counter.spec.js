import { expect, test } from "@playwright/test";

test("Counter executes the real MBC in WebAssembly", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Count: 0")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 1")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 2")).toBeVisible();
  const runtimeError = page.locator("[data-runtime-error]");
  await expect(runtimeError).toBeHidden();
  await expect(runtimeError).toHaveText("");
});
