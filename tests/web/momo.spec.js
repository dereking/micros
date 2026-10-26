import { expect, test } from "@playwright/test";

test("momo runs the listen → spell → cloze → definition flow", async ({ page }) => {
  await page.goto("/");

  const tile = page
    .locator("#system-screen")
    .getByRole("button", { name: "Open momo", exact: true });
  await expect(tile).toBeVisible();
  await tile.click();

  await expect(page.getByText("MOMO · 默认用户")).toBeVisible();
  await expect(page.getByText("今天的单词")).toBeVisible();
  await expect(
    page.locator('.micro-tab-page-active .micro-surface[data-tone="canvas"]'),
  ).toBeVisible();
  await expect(
    page.locator('.micro-tab-page-active .micro-surface[data-tone="paper"]').first(),
  ).toBeVisible();

  await expect(page.getByRole("button", { name: "回到桌面" })).toHaveCount(0);

  const screenBox = await page.locator("#app-screen").boundingBox();
  const startBox = await page
    .getByRole("button", { name: "开始今日学习" })
    .boundingBox();
  if (!screenBox || !startBox) throw new Error("momo controls should be laid out");
  expect(startBox.y + startBox.height).toBeLessThanOrEqual(
    screenBox.y + screenBox.height + 1,
  );

  await page.getByRole("button", { name: "开始今日学习" }).click();
  await expect(page.getByText("第 1 步 · 听音辨义")).toBeVisible();
  await expect(page.getByRole("button", { name: "开始今日学习" })).toBeHidden();

  // Reveal spelling, then the cloze example, then the definition.
  for (let step = 0; step < 3; step++) {
    await page.getByRole("button", { name: "不认识", exact: true }).click();
  }
  await expect(page.getByText("释义 · 自我评估")).toBeVisible();
  await expect(page.getByText("abandon", { exact: true })).toBeVisible();
  await expect(page.getByText("v. 放弃；抛弃；遗弃")).toBeVisible();

  // Grade the word: it enters the 1-day Ebbinghaus stage and counts as today's
  // first new word. The rating keeps the definition on screen until the blank
  // surface is tapped.
  await page.getByRole("button", { name: "认识", exact: true }).click();
  await expect(page.getByText("新词 1 / 5  ·  连续 1 天")).toBeVisible();
  await expect(page.getByText("释义 · 学习反馈")).toBeVisible();
  await expect(page.getByText("点空白处继续下一个词")).toBeVisible();
  await expect(page.getByText("v. 放弃；抛弃；遗弃")).toBeVisible();
  await expect(page.getByRole("button", { name: "认识", exact: true })).toBeHidden();

  const studyScreen = await page.locator("#app-screen").boundingBox();
  if (!studyScreen) throw new Error("momo screen should be laid out");
  await page.mouse.click(
    studyScreen.x + 8,
    studyScreen.y + studyScreen.height - 8,
  );
  await expect(page.getByText("第 1 步 · 听音辨义")).toBeVisible();

  // The progress tab is bound to the same KV-backed schedule.
  await page.getByRole("button", { name: "进度", exact: true }).click();
  await expect(page.getByText("记忆档案")).toBeVisible();
  await expect(page.getByText(/abandon  ·  第 3 阶  ·  /)).toBeVisible();
  await expect(page.getByText("absorb  ·  新词")).toBeVisible();

  // The sync tab gates on a token before issuing a host request.
  await page.getByRole("button", { name: "同步", exact: true }).click();
  await expect(page.getByText("同步今日计划")).toBeVisible();
  await expect(page.getByText("今日词表：—")).toBeVisible();
  await page.getByRole("button", { name: "同步墨墨", exact: true }).click();
  await expect(
    page.getByText("请先在用户页填写当前用户的 API Key。"),
  ).toBeVisible();

  // With a token the Web host returns its 501 JSON error, which exercises the
  // HTTP callback -> json.getText -> status binding chain. The token is edited
  // together with the user's name on the 用户 tab.
  await page.getByRole("button", { name: "用户", exact: true }).click();
  await page.getByPlaceholder("墨墨开放 API Token").fill("test-token");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByText("已保存当前用户的名称和 API Key。")).toBeVisible();
  await page.getByRole("button", { name: "同步", exact: true }).click();
  // The token goes through the unified `config` API, not the per-key KV store.
  await expect
    .poll(() =>
      page.evaluate(() =>
        JSON.parse(localStorage.getItem("micro:config:momo")).maimemoToken,
      ),
    )
    .toBe("test-token");
  expect(
    await page.evaluate(() =>
      localStorage.getItem("micro:kv:momo:maimemoToken"),
    ),
  ).toBeNull();
  await page.getByRole("button", { name: "同步墨墨", exact: true }).click();
  await expect(
    page.getByText("同步失败：此宿主暂不支持自定义请求头"),
  ).toBeVisible();

  // Reload: the per-App KV namespace must survive the page lifecycle.
  await page.reload();
  await page
    .locator("#system-screen")
    .getByRole("button", { name: "Open momo", exact: true })
    .click();
  await expect(page.getByText("新词 1 / 5  ·  连续 1 天")).toBeVisible();
  await page.getByRole("button", { name: "进度", exact: true }).click();
  await expect(page.getByText(/abandon  ·  第 3 阶  ·  /)).toBeVisible();

  await expect(page.locator("[data-runtime-error]")).toBeHidden();
});

test("momo isolates each user's study data, goal and token", async ({ page }) => {
  await page.goto("/");
  await page
    .locator("#system-screen")
    .getByRole("button", { name: "Open momo", exact: true })
    .click();
  await expect(page.getByText("MOMO · 默认用户")).toBeVisible();

  // Create the second profile from the 用户 tab.
  await page.getByRole("button", { name: "用户", exact: true }).click();
  await expect(page.getByText("PROFILE · 学习账号")).toBeVisible();
  await expect(page.getByText("名称和 Token 只保存到当前用户。")).toBeVisible();
  await page.getByRole("button", { name: "新建用户", exact: true }).click();
  await expect(page.getByPlaceholder("用户名")).toHaveValue("用户 2");
  await expect(
    page.getByRole("button", { name: "用户 2  ·  当前", exact: true }),
  ).toHaveClass(/micro-button-selected/);
  await expect(page.getByText("已创建 用户 2，请填写名称和 API Key。")).toBeVisible();

  const config = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("micro:config:momo")),
  );
  expect(config.userCount).toBe(2);
  expect(config.activeUser).toBe(2);
  expect(config["u2:maimemoToken"]).toBeUndefined();

  // User 2 gets its own daily goal, then studies a word and saves a token.
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("button", { name: "8 个", exact: true }).click();
  await expect(page.getByText("目标：8 个新词 / 天（可超额）")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() =>
        JSON.parse(localStorage.getItem("micro:config:momo"))["u2:goal"],
      ),
    )
    .toBe(8);

  await page.getByRole("button", { name: "学习", exact: true }).click();
  await expect(page.getByText("MOMO · 用户 2")).toBeVisible();
  await expect(page.getByText("新词 0 / 8  ·  连续 0 天")).toBeVisible();
  await page.getByRole("button", { name: "开始今日学习" }).click();
  await page.getByRole("button", { name: "认识", exact: true }).click();
  await expect(page.getByText("新词 1 / 8  ·  连续 1 天")).toBeVisible();

  // Name and API key are managed together on the 用户 tab.
  await page.getByRole("button", { name: "用户", exact: true }).click();
  await page.getByPlaceholder("用户名").fill("小墨");
  await page.getByPlaceholder("墨墨开放 API Token").fill("user-two-token");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByText("已保存当前用户的名称和 API Key。")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "小墨  ·  当前", exact: true }),
  ).toHaveClass(/micro-button-selected/);
  await expect(
    page.getByRole("button", { name: "小墨  ·  当前", exact: true }),
  ).toHaveCSS("background-color", "rgb(198, 122, 79)");
  await expect(page.getByRole("button", { name: "默认用户", exact: true })).toHaveCSS(
    "background-color",
    "rgb(31, 107, 79)",
  );
  await expect
    .poll(() =>
      page.evaluate(() => {
        const config = JSON.parse(localStorage.getItem("micro:config:momo"));
        return [config["u2:name"], config["u2:maimemoToken"]];
      }),
    )
    .toEqual(["小墨", "user-two-token"]);
  await page.getByRole("button", { name: "学习", exact: true }).click();
  await expect(page.getByText("MOMO · 小墨")).toBeVisible();
  await page.getByRole("button", { name: "同步", exact: true }).click();
  await expect(page.getByText(/已配置 · 可在用户页修改/)).toBeVisible();
  expect(
    JSON.parse(
      await page.evaluate(() => localStorage.getItem("micro:kv:momo:u2:newToday")),
    ),
  ).toBe(1);
  expect(
    await page.evaluate(() => localStorage.getItem("micro:kv:momo:newToday")),
  ).toBeNull();

  // The default user keeps an empty, independent profile.
  await page.getByRole("button", { name: "用户", exact: true }).click();
  await page.getByRole("button", { name: "默认用户", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "默认用户  ·  当前", exact: true }),
  ).toHaveClass(/micro-button-selected/);
  await expect(page.getByPlaceholder("用户名")).toHaveValue("默认用户");
  await page.getByRole("button", { name: "学习", exact: true }).click();
  await expect(page.getByText("MOMO · 默认用户")).toBeVisible();
  await expect(page.getByText("新词 0 / 5  ·  连续 0 天")).toBeVisible();

  await page.getByRole("button", { name: "同步", exact: true }).click();
  await expect(page.getByText(/未配置 · 请到用户页填写/)).toBeVisible();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByText("目标：5 个新词 / 天（可超额）")).toBeVisible();

  // Make user 2 active again, then reload: the active user, its name, word and
  // token all survive the page lifecycle.
  await page.getByRole("button", { name: "用户", exact: true }).click();
  await page.getByRole("button", { name: "小墨", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "小墨  ·  当前", exact: true }),
  ).toHaveClass(/micro-button-selected/);
  await expect
    .poll(() =>
      page.evaluate(() =>
        JSON.parse(localStorage.getItem("micro:config:momo")).activeUser,
      ),
    )
    .toBe(2);
  await page.reload();
  await page
    .locator("#system-screen")
    .getByRole("button", { name: "Open momo", exact: true })
    .click();
  await expect(page.getByText("MOMO · 小墨")).toBeVisible();
  await expect(page.getByText("新词 1 / 8  ·  连续 1 天")).toBeVisible();
  await page.getByRole("button", { name: "同步", exact: true }).click();
  await expect(page.getByText(/已配置 · 可在用户页修改/)).toBeVisible();

  await page.getByRole("button", { name: "用户", exact: true }).click();
  await expect(page.getByPlaceholder("用户名")).toHaveValue("小墨");
  await expect(page.getByPlaceholder("墨墨开放 API Token")).toHaveValue(
    "user-two-token",
  );
  await page.getByRole("button", { name: "默认用户", exact: true }).click();
  await page.getByRole("button", { name: "学习", exact: true }).click();
  await expect(page.getByText("新词 0 / 5  ·  连续 0 天")).toBeVisible();
  expect(
    await page.evaluate(() => localStorage.getItem("micro:kv:momo:w0")),
  ).toBeNull();

  await expect(page.locator("[data-runtime-error]")).toBeHidden();
});

test("mouse wheel and trackpad scrolling move the tab page scrollbar", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .locator("#system-screen")
    .getByRole("button", { name: "Open momo", exact: true })
    .click();

  // The 同步 page is taller than the tab body, so it is a real scroll
  // container; the 学习 page fits and must not be used as the fixture.
  await page.getByRole("button", { name: "同步", exact: true }).click();
  const tabPage = page.locator(".micro-tab-page-active");
  await expect(tabPage).toBeVisible();
  const metrics = await tabPage.evaluate((element) => ({
    clientHeight: element.clientHeight,
    scrollHeight: element.scrollHeight,
    scrollTop: element.scrollTop,
  }));
  expect(metrics.scrollHeight).toBeGreaterThan(metrics.clientHeight);
  expect(metrics.scrollTop).toBe(0);

  // A real scrollbar, not hidden browser chrome: the stylesheet gives the
  // scroll container a slim thumb and reserves its gutter during layout.
  await expect(tabPage).toHaveCSS("overflow-y", "auto");
  await expect(tabPage).toHaveCSS("scrollbar-width", "thin");
  const scrollbar = await tabPage.evaluate((element) => ({
    width: getComputedStyle(element, "::-webkit-scrollbar").width,
    thumb: getComputedStyle(element, "::-webkit-scrollbar-thumb").backgroundColor,
  }));
  expect(scrollbar.width).toBe("6px");
  expect(scrollbar.thumb).toBe("rgb(184, 191, 180)");

  const box = await tabPage.boundingBox();
  if (!box) throw new Error("active tab page should be laid out");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, 120);
  await expect
    .poll(() => tabPage.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(0);

  // Wheel-up returns the page to the top, so the gesture is symmetric.
  await page.mouse.wheel(0, -240);
  await expect.poll(() => tabPage.evaluate((element) => element.scrollTop)).toBe(0);

  await expect(page.locator("[data-runtime-error]")).toBeHidden();
});
