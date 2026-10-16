// WP-6.3 — the scene against `nk serve --demo`, through the scene's own
// dev page (/src/scene/dev.html: store + sync + <Scene>), read via the
// window.__nkScene debug hook.

import { expect, test, type Page } from '@playwright/test';

const MIG = 'd3e0a002-0000-4000-8000-000000000002'; // always blocked
const REP = 'd3e0a003-0000-4000-8000-000000000003'; // done, returned
const DOT = 'd3e0a004-0000-4000-8000-000000000004'; // interactive idle
const IDX = 'd3e0a006-0000-4000-8000-000000000006'; // busy, 2 subagents running

interface Debug {
  mode: 'webgl' | 'flat';
  cats: number;
  kittens: number;
  poses: Record<string, string>;
  frames: number;
  reducedMotion: boolean;
  screenPos(id: string): { x: number; y: number } | null;
}

function pageURL(extra: Record<string, string> = {}): string {
  const u = new URL('/src/scene/dev.html', process.env.NK_GUI_URL!);
  u.searchParams.set('endpoint', u.origin);
  u.searchParams.set('token', process.env.NK_TOKEN!);
  for (const [k, v] of Object.entries(extra)) u.searchParams.set(k, v);
  return u.toString();
}

const dbg = <T>(page: Page, fn: (d: Debug) => T) =>
  page.evaluate((src) => {
    const d = (globalThis as unknown as { __nkScene?: Debug }).__nkScene;
    return d ? (new Function('d', `return (${src})(d)`) as (d: Debug) => T)(d) : null;
  }, fn.toString());

/** Sessions that should have a cat: the snapshot's live ones (history only if the tree shows it). */
const expectedCats = (page: Page) =>
  page.evaluate(() => {
    const st = (globalThis as unknown as { nk: { store: { state: { value: { snapshot: { sessions: { kind: string; archived?: boolean }[] } } } } } }).nk
      .store.state.value;
    return st.snapshot.sessions.filter((s) => !s.archived && s.kind !== 'history').length;
  });

/** Waits until the frame counter stays put for a whole second (render on demand). */
async function waitIdle(page: Page, timeoutMs = 20_000): Promise<number> {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    const a = (await dbg(page, (d) => d.frames))!;
    await page.waitForTimeout(1000);
    const b = (await dbg(page, (d) => d.frames))!;
    if (a === b) return b;
  }
  throw new Error('the scene never went idle');
}

test('one toon cat per demo session, poses from state, render on demand', async ({ page }, info) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(pageURL());
  await expect(page.locator('canvas.nk-scene-canvas')).toBeVisible();
  await expect.poll(() => dbg(page, (d) => d.mode)).toBe('webgl');
  await expect.poll(() => dbg(page, (d) => d.cats)).toBe(6);
  expect(await expectedCats(page)).toBe(6);
  const poses = (await dbg(page, (d) => d.poses))!;
  expect(poses[MIG]).toBe('alert');
  expect(poses[REP]).toBe('sleeping');
  expect(poses[DOT]).toBe('sit');
  expect(poses[IDX]).toBe('typing');
  expect(await dbg(page, (d) => d.kittens)).toBe(2);

  // the ! bubble over the blocked cat, zZ over the sleeping one, ctx labels
  await expect(page.locator('.nk-ov-bubble', { hasText: '!' }).first()).toBeVisible();
  await expect(page.locator('.nk-ov-bubble', { hasText: 'zZ' }).first()).toBeVisible();
  await expect(page.locator('.nk-ov-ctx')).toHaveText(['oss', 'pessoal', 'visio']);

  // render on demand: some frames were drawn, then none for a full second
  const frames = await waitIdle(page);
  expect(frames).toBeGreaterThan(0);
  const shot = await page.screenshot();
  await info.attach('scene', { body: shot, contentType: 'image/png' });
  // SCENE_SHOT=/path.png keeps a copy for eyeballing the cats
  if (process.env.SCENE_SHOT) await page.screenshot({ path: process.env.SCENE_SHOT });
  expect(errors).toEqual([]);
});

test('hover shows the label, click opens the session', async ({ page }) => {
  await page.goto(pageURL());
  await expect.poll(() => dbg(page, (d) => d.cats)).toBe(6);
  await waitIdle(page);
  const p = await page.evaluate((id) => (globalThis as unknown as { __nkScene: Debug }).__nkScene.screenPos(id), MIG);
  expect(p).not.toBeNull();
  // aim a bit below the head anchor, at the body
  await page.mouse.move(p!.x, p!.y + 30);
  await expect(page.getByTestId('scene-tip')).toContainText('migração do banco');
  if (process.env.SCENE_SHOT) await page.screenshot({ path: process.env.SCENE_SHOT.replace(/\.png$/, '-hover.png') });
  await page.mouse.down();
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => (globalThis as unknown as { nk: { opened: string[] } }).nk.opened)).toEqual([MIG]);
  // away from every cat: no tooltip
  await page.mouse.move(5, 5);
  await expect(page.getByTestId('scene-tip')).toHaveCount(0);
});

test('prefers-reduced-motion: static poses, the scene stays still', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto(pageURL());
  await expect.poll(() => dbg(page, (d) => d.cats)).toBe(6);
  expect(await dbg(page, (d) => d.reducedMotion)).toBe(true);
  const settled = await waitIdle(page);
  // no ambient bout (one would start within 9 s): a quiet stretch has ≤ the
  // single frame a demo state change may cause
  await page.waitForTimeout(10_000);
  expect((await dbg(page, (d) => d.frames))! - settled).toBeLessThanOrEqual(2);
});

test('flat mode: text cats ᓚᘏᗢ, still clickable', async ({ page }) => {
  await page.goto(pageURL({ flat: '1' }));
  const scene = page.getByTestId('scene');
  await expect(scene).toHaveAttribute('data-mode', 'flat');
  await expect(page.locator('.nk-flat-cat')).toHaveCount(6);
  await expect(page.locator(`.nk-flat-cat[data-session="${MIG}"]`)).toHaveAttribute('data-pose', 'alert');
  await expect(page.locator(`.nk-flat-cat[data-session="${MIG}"] .cat`)).toHaveText('ᓚᘏᗢ !');
  await expect(page.locator('canvas')).toHaveCount(0);
  await page.locator(`.nk-flat-cat[data-session="${REP}"]`).click();
  await expect.poll(() => page.evaluate(() => (globalThis as unknown as { nk: { opened: string[] } }).nk.opened)).toEqual([REP]);
  expect(await dbg(page, (d) => d.mode)).toBe('flat');
});

test('no WebGL: falls back to the flat scene with a note', async ({ page }) => {
  await page.addInitScript(() => {
    const orig = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string, ...rest: unknown[]) {
      if (type.startsWith('webgl')) return null;
      return (orig as (...a: unknown[]) => unknown).call(this, type, ...rest);
    } as typeof orig;
  });
  await page.goto(pageURL());
  await expect(page.getByTestId('scene')).toHaveAttribute('data-mode', 'flat');
  await expect(page.locator('.nk-flat-note')).toBeVisible();
  await expect(page.locator('.nk-flat-cat')).toHaveCount(6);
});
