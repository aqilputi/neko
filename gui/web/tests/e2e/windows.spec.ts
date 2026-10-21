// Terminal windows against `nk serve --demo`: the tree, opening a window
// for a session (POST /v1/panes + WS + xterm.js), typing in INSERT (the
// demo pane echoes each line), ctrl+] back to NORMAL, w closes, dragging
// stays above the calçada, z zooms; and the main page with the scene.

import { expect, test, type Page } from '@playwright/test';

const GW = 'd3e0a001-0000-4000-8000-000000000001';

function pageURL(): string {
  const u = new URL(process.env.NK_GUI_URL!);
  u.searchParams.set('endpoint', u.origin);
  u.searchParams.set('token', process.env.NK_TOKEN!);
  return u.toString();
}

/** The specs share one `nk serve --demo` with the inbox specs, whose
 * inbox.summon reaches every connected page and opens (and focuses) a
 * window there. These tests drive the keyboard, so they don't listen. */
async function ignoreSummons(page: Page) {
  await page.addInitScript(() => {
    const add = EventSource.prototype.addEventListener;
    EventSource.prototype.addEventListener = function (this: EventSource, ...args: Parameters<typeof add>) {
      if (args[0] !== 'inbox.summoned') add.apply(this, args);
    } as typeof add;
  });
}

async function open(page: Page) {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await ignoreSummons(page);
  await page.goto(pageURL());
  await expect(page.getByTestId('tree')).toContainText('api-gateway');
  await expect(page.getByTestId('status')).toHaveText('conectado');
  return errors;
}

const cursorRow = (page: Page) => page.locator('[data-testid="tree"] .is-cursor');
const screenText = (page: Page, id: string) =>
  page.evaluate((sid) => (globalThis as unknown as { nk: { windows: { text(id: string): string } } }).nk.windows.text(sid), id);

/** Moves the tree cursor to the row whose text contains label (j presses). */
async function cursorTo(page: Page, label: string) {
  await page.keyboard.press('g');
  await page.keyboard.press('g');
  await expect(cursorRow(page)).toHaveAttribute('data-key', /^ctx:/);
  for (let i = 0; i < 30; i++) {
    if (((await cursorRow(page).textContent()) ?? '').includes(label)) return;
    const before = await cursorRow(page).getAttribute('data-key');
    await page.keyboard.press('j');
    // wait for the cursor to move before reading it again (no overshoot)
    await expect(cursorRow(page)).not.toHaveAttribute('data-key', before ?? '');
  }
  throw new Error(`no row ${label}`);
}

test('the tree shows the demo sessions; j/k move the cursor', async ({ page }) => {
  const errors = await open(page);
  const tree = page.getByTestId('tree');
  for (const label of ['acme', 'api-gateway', 'migração do banco', 'indexador', 'dotfiles']) await expect(tree).toContainText(label);
  await expect(page.getByTestId('mode')).toHaveText('NORMAL');
  await expect(page.getByTestId('count-busy')).toContainText('rodando');
  await expect(page.getByTestId('usage-5h')).toContainText('%');
  // group chips
  await expect(tree.locator('.nk-chip').first()).toHaveText('#deploy-rc');

  await expect(cursorRow(page)).toHaveAttribute('data-key', 'ctx:acme');
  await page.keyboard.press('j');
  await expect(cursorRow(page)).toHaveAttribute('data-key', /^sess:/);
  await page.keyboard.press('k');
  await expect(cursorRow(page)).toHaveAttribute('data-key', 'ctx:acme');
  await page.keyboard.press('Shift+G');
  await expect(cursorRow(page)).not.toHaveAttribute('data-key', 'ctx:acme');
  await page.keyboard.press('g');
  await page.keyboard.press('g');
  await expect(cursorRow(page)).toHaveAttribute('data-key', 'ctx:acme');
  expect(errors).toEqual([]);
});

test('enter opens a terminal window; INSERT echoes; ctrl+] back; w closes', async ({ page }) => {
  const errors = await open(page);
  await cursorTo(page, 'api-gateway');
  await page.keyboard.press('Enter');
  const win = page.locator(`[data-testid="window"][data-session="${GW}"]`);
  await expect(win).toBeVisible();
  await expect(win.locator('.nk-win-title')).toHaveText('api-gateway');
  // the fake Claude Code screen of the demo
  await expect.poll(() => screenText(page, GW), { timeout: 10_000 }).toContain('Claude Code');
  await expect.poll(() => screenText(page, GW)).toContain('demo do nekuroddo');
  // focused: INSERT, and keys go to the PTY
  await expect(page.getByTestId('mode')).toHaveText('INSERT');
  await page.keyboard.type('ola gato');
  await page.keyboard.press('Enter');
  await expect.poll(() => screenText(page, GW), { timeout: 10_000 }).toContain('recebi: ola gato');
  // "j" typed in INSERT did not move the tree cursor
  await expect(cursorRow(page)).toContainText('api-gateway');
  // ctrl+] leaves INSERT; now j moves the cursor again
  await page.keyboard.press('Control+BracketRight');
  await expect(page.getByTestId('mode')).toHaveText('NORMAL');
  // z zooms the active window to the whole area
  const before = (await win.boundingBox())!;
  await page.keyboard.press('z');
  await expect.poll(async () => (await win.boundingBox())!.width).toBeGreaterThan(before.width);
  await page.keyboard.press('z');
  await expect.poll(async () => Math.round((await win.boundingBox())!.width)).toBe(Math.round(before.width));
  // w closes it (and the pane in the core)
  const paneId = await page.evaluate(
    (sid) => (globalThis as unknown as { nk: { windows: { state: { value: { wins: Array<{ id: string; paneId?: string }> } } } } }).nk.windows.state.value.wins.find((w) => w.id === sid)?.paneId,
    GW,
  );
  expect(paneId).toBeTruthy();
  await page.keyboard.press('w');
  await expect(win).toHaveCount(0);
  await expect
    .poll(async () => {
      const r = await page.request.get(`${process.env.NK_ENDPOINT}/v1/panes`, {
        headers: { Authorization: `Bearer ${process.env.NK_TOKEN}` },
      });
      return ((await r.json()) as Array<{ id: string }>).some((p) => p.id === paneId);
    })
    .toBe(false);
  expect(errors).toEqual([]);
});

test('ctrl+k inside a terminal opens the switcher', async ({ page }) => {
  await open(page);
  await cursorTo(page, 'api-gateway');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('mode')).toHaveText('INSERT');
  await page.keyboard.press('Control+k');
  await expect(page.getByTestId('switcher')).toBeVisible();
  await expect(page.getByTestId('mode')).toHaveText('NORMAL');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('switcher')).toHaveCount(0);
});

test('drag by the title bar stays above the calçada; alt+arrows move it', async ({ page }) => {
  await open(page);
  await cursorTo(page, 'api-gateway');
  await page.keyboard.press('o'); // preview: opens without focus
  const win = page.locator(`[data-testid="window"][data-session="${GW}"]`);
  await expect(win).toBeVisible();
  await expect(page.getByTestId('mode')).toHaveText('NORMAL');
  const bar = win.locator('.nk-win-bar');
  const b = (await bar.boundingBox())!;
  await page.mouse.move(b.x + 60, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + 120, b.y + 2000, { steps: 5 });
  await expect(win).toHaveClass(/is-dragging/);
  await page.mouse.up();
  await expect(win).not.toHaveClass(/is-dragging/);
  const vh = page.viewportSize()!.height;
  // above the status bar (40px) and the calçada (72px), once the lift settled
  await expect.poll(async () => {
    const bb = (await win.boundingBox())!;
    return bb.y + bb.height;
  }).toBeLessThanOrEqual(vh - 72 - 40 + 1);
  const box = (await win.boundingBox())!;
  expect(box.x).toBeGreaterThan(b.x + 30);

  const x0 = box.x;
  await page.keyboard.press('Alt+ArrowLeft');
  await expect.poll(async () => Math.round((await win.boundingBox())!.x)).toBe(Math.round(x0 - 24));
});

test('clicking a cat in the scene opens its window', async ({ page }) => {
  const errors = await open(page);
  await expect(page.locator('.nk-scene-layer canvas, .nk-scene-layer [data-testid]').first()).toBeVisible();
  type Dbg = { __nkScene?: { cats: number; mode: string; screenPos(id: string): { x: number; y: number } | null } };
  await expect.poll(() => page.evaluate(() => (globalThis as Dbg).__nkScene?.cats ?? 0), { timeout: 10_000 }).toBeGreaterThan(0);
  const mode = await page.evaluate(() => (globalThis as Dbg).__nkScene!.mode);
  if (mode === 'webgl') {
    await expect(page.locator('.nk-scene-layer canvas')).toBeVisible();
    // screenPos is canvas-local; wait for the cat to settle where it is
    let pos: { x: number; y: number } | null = null;
    await expect
      .poll(async () => {
        const a = await page.evaluate((id) => (globalThis as Dbg).__nkScene!.screenPos(id), GW);
        await page.waitForTimeout(300);
        const b = await page.evaluate((id) => (globalThis as Dbg).__nkScene!.screenPos(id), GW);
        pos = b;
        return !!a && !!b && Math.abs(a.x - b.x) < 1 && Math.abs(a.y - b.y) < 1;
      }, { timeout: 15_000 })
      .toBe(true);
    const rect = (await page.locator('.nk-scene-layer canvas').boundingBox())!;
    await page.mouse.click(rect.x + pos!.x, rect.y + pos!.y + 30);
  } else {
    await page.locator(`.nk-scene-layer [data-session="${GW}"]`).first().click();
  }
  await expect(page.locator(`[data-testid="window"][data-session="${GW}"]`)).toBeVisible();
  expect(errors).toEqual([]);
});
