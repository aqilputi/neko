// The inbox cat against `nk serve --demo`: the demo always has someone
// waiting ("migração do banco" blocked, "relatório Q3" returned; the
// api-gateway blocks every other 15 s) and sends a ✉ every 20 s.
//
// The cat is exercised on its own harness (/src/inbox/dev.html: same
// store/sync as the app, opening a session = #open=<id>), and in the app
// once App mounts InboxWalk.

import { expect, test, type Page } from '@playwright/test';

const MIG = 'd3e0a002-0000-4000-8000-000000000002';
const GW = 'd3e0a001-0000-4000-8000-000000000001';

function pageURL(path = '/'): string {
  const u = new URL(path, process.env.NK_GUI_URL!);
  u.searchParams.set('endpoint', u.origin);
  u.searchParams.set('token', process.env.NK_TOKEN!);
  return u.toString();
}

async function openHarness(page: Page) {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(pageURL('/src/inbox/dev.html'));
  return errors;
}

const walker = (page: Page, session: string, kind?: string) =>
  page.locator(`[data-testid="inbox-walker"][data-session="${session}"]${kind ? `[data-kind="${kind}"]` : ''}`);

test('a blocked session sends a cat that walks the calçada and stops', async ({ page }) => {
  const errors = await openHarness(page);
  const cat = walker(page, MIG, 'blocked');
  await expect(cat).toBeVisible({ timeout: 5000 });
  await expect(cat.locator('.nk-bubble')).toHaveText('!');
  await expect(cat).toHaveAttribute('aria-label', /migração do banco/);
  // it walks in from the left and stops (render on demand: nothing animates
  // once it arrived)
  await expect(cat).toHaveAttribute('data-phase', 'arrived', { timeout: 15_000 });
  const box = await cat.boundingBox();
  const vh = page.viewportSize()!.height;
  expect(box!.x).toBeGreaterThan(0);
  expect(box!.y).toBeGreaterThanOrEqual(vh - 72);
  const running = await cat.evaluate((el) => el.getAnimations({ subtree: true }).filter((a) => a.playState === 'running').length);
  expect(running).toBe(0);
  // label on hover
  await cat.hover();
  await expect(cat.locator('.nk-walker-label')).toBeVisible();
  expect(errors).toEqual([]);
});

test('clicking a cat opens its session and it leaves', async ({ page }) => {
  await openHarness(page);
  const cat = walker(page, MIG, 'blocked');
  await expect(cat).toBeVisible({ timeout: 5000 });
  await cat.click();
  await expect(page).toHaveURL(new RegExp(`#open=${MIG}$`));
  await expect(page.getByTestId('inbox-walk')).toHaveAttribute('data-last-open', MIG);
  await expect(walker(page, MIG)).toHaveCount(0);
});

test('shift+space summons the most urgent session (inbox.summoned)', async ({ page }) => {
  await openHarness(page);
  await expect(walker(page, MIG, 'blocked')).toBeVisible({ timeout: 5000 });
  await page.keyboard.press('Shift+Space');
  // the gateway when it is blocked (more recent), else the migration
  await expect(page).toHaveURL(new RegExp(`#open=(${MIG}|${GW})$`));
  const picked = new URL(page.url()).hash.slice('#open='.length);
  await expect(page.getByTestId('inbox-walk')).toHaveAttribute('data-summoned', picked);
  await expect(walker(page, picked).first()).toHaveClass(/nk-walker-summoned/);
  // after the glow it leaves as seen
  await expect(walker(page, picked)).toHaveCount(0, { timeout: 5000 });
});

test('inbox.summoned from another client (nk run inbox.summon) opens it here', async ({ page, request }) => {
  await openHarness(page);
  await expect(walker(page, MIG, 'blocked')).toBeVisible({ timeout: 5000 });
  const res = await request.post(`${process.env.NK_ENDPOINT}/v1/commands/inbox.summon`, {
    headers: { Authorization: `Bearer ${process.env.NK_TOKEN}` },
    data: { context: {} },
  });
  expect(res.ok()).toBe(true);
  await expect(page).toHaveURL(new RegExp(`#open=(${MIG}|${GW})$`));
});

test('a peer ✉ brings an envelope cat', async ({ page }) => {
  test.setTimeout(45_000);
  await openHarness(page);
  const env = page.locator('[data-testid="inbox-walker"][data-kind="mail"]').first();
  await expect(env).toBeVisible({ timeout: 25_000 });
  await expect(env.locator('.nk-bubble')).toHaveText('✉');
});

test('prefers-reduced-motion: a static badge with the same items', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await openHarness(page);
  const badge = page.getByTestId('inbox-badge');
  await expect(badge).toBeVisible({ timeout: 5000 });
  await expect(badge).toContainText('migração do banco');
  expect(await page.locator('.nk-walker').count()).toBe(0);
  await walker(page, MIG, 'blocked').click();
  await expect(page).toHaveURL(new RegExp(`#open=${MIG}$`));
});

test('in the app: the cat is on the calçada (once App mounts InboxWalk)', async ({ page }) => {
  await page.goto(pageURL());
  await expect(page.getByTestId('status')).toHaveText('conectado', { timeout: 10_000 });
  test.skip((await page.getByTestId('inbox-walk').count()) === 0, 'App does not mount InboxWalk yet');
  await expect(walker(page, MIG, 'blocked')).toBeVisible({ timeout: 5000 });
});
