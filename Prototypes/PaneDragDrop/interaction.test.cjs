const { chromium } = require("playwright");
const assert = require("node:assert/strict");

const baseURL = process.env.PROTOTYPE_URL || "http://127.0.0.1:4173";
const results = [];

async function run(name, behavior) {
  try {
    await behavior();
    results.push({ name, ok: true });
    console.log(`✓ ${name}`);
  } catch (error) {
    results.push({ name, ok: false, error });
    console.error(`✗ ${name}\n  ${error.message}`);
  }
}

async function openPage(browser, variant = "edge") {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await page.goto(`${baseURL}/?variant=${variant}`);
  return page;
}

async function pointFor(page, selector, xRatio = 0.5, yRatio = 0.5) {
  const box = await page.locator(selector).boundingBox();
  assert.ok(box, `Expected ${selector} to have a bounding box`);
  return { x: box.x + box.width * xRatio, y: box.y + box.height * yRatio, box };
}

async function beginDrag(page, selector) {
  const start = await pointFor(page, selector, 0.35, 0.5);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + 12, start.y + 8, { steps: 2 });
  return start;
}

(async () => {
  const browser = await chromium.launch({ headless: true });

  await run("tab ghost preserves the pointer grab offset", async () => {
    const page = await openPage(browser);
    const start = await beginDrag(page, '[data-tab-id="claude"]');
    const pointer = { x: start.x + 130, y: start.y + 74 };
    await page.mouse.move(pointer.x, pointer.y, { steps: 3 });
    const ghost = await page.locator(".drag-ghost").boundingBox();
    assert.ok(ghost, "Expected a visible drag ghost");
    assert.ok(Math.abs(ghost.x - (pointer.x - (start.x - start.box.x))) < 3, "Ghost lost the horizontal grab offset");
    assert.ok(Math.abs(ghost.y - (pointer.y - (start.y - start.box.y))) < 3, "Ghost lost the vertical grab offset");
    await page.mouse.up();
    await page.close();
  });

  await run("Escape cancels without mutating the layout", async () => {
    const page = await openPage(browser);
    const before = await page.evaluate(() => window.__panePrototype.getState());
    const target = await pointFor(page, '[data-group-id="B"] .pane-content', 0.96, 0.5);
    await beginDrag(page, '[data-tab-id="claude"]');
    await page.mouse.move(target.x, target.y, { steps: 4 });
    await page.keyboard.press("Escape");
    const after = await page.evaluate(() => window.__panePrototype.getState());
    assert.deepEqual(after, before);
    assert.equal(await page.locator(".drag-ghost").count(), 0);
    await page.close();
  });

  await run("center drop moves a sole tab and collapses its source group", async () => {
    const page = await openPage(browser);
    const target = await pointFor(page, '[data-group-id="C"] .tabbar', 0.8, 0.5);
    await beginDrag(page, '[data-tab-id="appmodel"]');
    await page.mouse.move(target.x, target.y, { steps: 5 });
    await page.mouse.up();
    const state = await page.evaluate(() => window.__panePrototype.getState());
    assert.equal(state.groups.B, undefined);
    assert.deepEqual(state.groups.C.tabs.map((tab) => tab.id), ["readme", "appmodel"]);
    assert.equal(state.lastMutation.kind, "move-and-collapse");
    await page.close();
  });

  await run("edge drop creates a new split and preserves the target group", async () => {
    const page = await openPage(browser, "edge");
    const target = await pointFor(page, '[data-group-id="B"] .pane-content', 0.97, 0.5);
    await beginDrag(page, '[data-tab-id="claude"]');
    await page.mouse.move(target.x, target.y, { steps: 5 });
    assert.equal(await page.locator('[data-drop-zone="right"]').count(), 1);
    await page.mouse.up();
    const state = await page.evaluate(() => window.__panePrototype.getState());
    assert.equal(Object.keys(state.groups).length, 4);
    assert.ok(state.groups.B, "The target group identity must survive the split");
    assert.equal(state.lastMutation.kind, "split");
    assert.match(state.lastMutation.label, /right of group B/);
    await page.close();
  });

  await run("divider resize updates the preferred normalized fraction", async () => {
    const page = await openPage(browser);
    const divider = await pointFor(page, '[data-split-id="split-root"]', 0.5, 0.5);
    const before = await page.evaluate(() => window.__panePrototype.getState().root.fraction);
    await page.mouse.move(divider.x, divider.y);
    await page.mouse.down();
    await page.mouse.move(divider.x + 90, divider.y, { steps: 5 });
    await page.mouse.up();
    const after = await page.evaluate(() => window.__panePrototype.getState().root.fraction);
    assert.ok(after > before + 0.05, `Expected fraction to grow; before=${before}, after=${after}`);
    await page.close();
  });

  await run("all feedback variants expose their distinct right-edge target", async () => {
    for (const variant of ["edge", "compass", "rails"]) {
      const page = await openPage(browser, variant);
      await beginDrag(page, '[data-tab-id="claude"]');
      if (variant === "compass") {
        const groupCenter = await pointFor(page, '[data-group-id="B"] .pane-content');
        await page.mouse.move(groupCenter.x, groupCenter.y, { steps: 4 });
        const target = await pointFor(page, '[data-zone-control="right"]');
        await page.mouse.move(target.x, target.y, { steps: 3 });
      } else if (variant === "rails") {
        const groupCenter = await pointFor(page, '[data-group-id="B"] .pane-content');
        await page.mouse.move(groupCenter.x, groupCenter.y, { steps: 4 });
        const target = await pointFor(page, '[data-zone-control="right"]');
        await page.mouse.move(target.x, target.y, { steps: 3 });
      } else {
        const target = await pointFor(page, '[data-group-id="B"] .pane-content', 0.97, 0.5);
        await page.mouse.move(target.x, target.y, { steps: 4 });
      }
      assert.equal(await page.locator('[data-drop-zone="right"]').count(), 1, `${variant} did not expose a right target`);
      await page.keyboard.press("Escape");
      await page.close();
    }
  });

  await browser.close();
  const failures = results.filter((result) => !result.ok);
  console.log(`\n${results.length - failures.length}/${results.length} interaction checks passed`);
  if (failures.length) process.exit(1);
})();
