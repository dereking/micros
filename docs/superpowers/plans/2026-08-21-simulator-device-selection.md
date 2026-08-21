# Simulator Device Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent browser text selection and native drag behavior inside the simulated ESP32 display without changing its touch, button, or scroll behavior.

**Architecture:** The browser simulator already has a dedicated `.device-screen` boundary. CSS will apply selection and drag suppression exclusively there, leaving the surrounding heading and monitoring panel untouched. A Playwright test will validate the computed selection property at the real device boundary.

**Tech Stack:** CSS, Vite, Playwright, Node.js test runner.

---

### Task 1: Protect simulated display interactions

**Files:**
- Modify: `products/micro-web-player/src/style.css:18`
- Test: `tests/web/counter.spec.js`

- [ ] **Step 1: Write the failing browser test**

Append this test to `tests/web/counter.spec.js`:

```js
test("device display disables browser text selection", async ({ page }) => {
  await page.goto("/");

  const deviceScreen = page.locator("[data-device-screen]");
  await expect(deviceScreen).toHaveCSS("user-select", "none");
  await expect(deviceScreen).toHaveCSS("-webkit-user-select", "none");

  const monitor = page.locator(".monitor");
  await expect(monitor).not.toHaveCSS("user-select", "none");
});
```

- [ ] **Step 2: Verify the test fails before implementation**

Run `npx playwright test tests/web/counter.spec.js`.

Expected: the new test fails because `[data-device-screen]` has computed `user-select: auto`.

- [ ] **Step 3: Add the scoped CSS boundary**

Extend the existing `.device-screen` declaration in `products/micro-web-player/src/style.css` with:

```css
user-select: none;
-webkit-user-select: none;
-webkit-user-drag: none;
```

No JavaScript event listeners are added. The existing `.system-screen` keeps `overflow-y: auto`, so internal scrolling remains available.

- [ ] **Step 4: Verify the browser test is green**

Run `npx playwright test tests/web/counter.spec.js`.

Expected: all four Playwright tests pass, including Counter, touch monitor, device scrolling, and selection suppression.

- [ ] **Step 5: Run Web regression checks**

Run `npm run test:web:unit`, `npm run build:web`, and `git diff --check`.

Expected: Web unit tests pass, Vite production build completes, and `git diff --check` produces no output.

- [ ] **Step 6: Commit the focused change**

Run `git add products/micro-web-player/src/style.css tests/web/counter.spec.js` followed by `git commit -m "fix: disable selection in simulator display"`.

Expected: one commit containing only the CSS boundary and Playwright regression coverage.
