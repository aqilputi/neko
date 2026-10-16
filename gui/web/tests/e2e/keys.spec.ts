// Keyboard-first against `nk serve --demo`: the ":" palette finds and runs
// commands (and typed ones), confirmations are a modal dialog answered
// with y/n, the help sheet lists the keymap, tabs switch with 1–4.

import { expect, test, type Page } from '@playwright/test';

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

async function openPalette(page: Page) {
  await page.keyboard.press(':');
  await expect(page.getByTestId('palette').locator('input')).toBeFocused();
}

const cursorRow = (page: Page) => page.locator('[data-testid="tree"] .is-cursor');

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

test('the palette finds and runs a command', async ({ page }) => {
  const errors = await open(page);
  await page.keyboard.press(':');
  const pal = page.getByTestId('palette');
  await expect(pal.locator('input')).toBeFocused();
  await page.keyboard.type('todos os comandos');
  await expect(pal.locator('.nk-pal-item.is-cur')).toContainText('todos os comandos');
  await page.keyboard.press('Enter');
  await expect(pal).toHaveCount(0);
  await expect(page.getByTestId('help')).toBeVisible();
  await expect(page.getByTestId('help')).toContainText('comandos');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('help')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('the palette runs a core command (sessions.refresh) and toasts', async ({ page }) => {
  await open(page);
  await openPalette(page);
  await page.keyboard.type('atualiza e reordena');
  await expect(page.locator('.nk-pal-item.is-cur')).toContainText('atualiza');
  const ran = page.waitForRequest((r) => r.url().includes('/v1/commands/sessions.refresh') && r.method() === 'POST');
  await page.keyboard.press('Enter');
  await ran;
  await expect(page.getByTestId('palette')).toHaveCount(0);
});

test('typed arguments: "group <tag>" groups the cursor session', async ({ page }) => {
  await open(page);
  await cursorTo(page, 'dotfiles');
  await openPalette(page);
  await page.keyboard.type('group gatos');
  await expect(page.locator('.nk-pal-item.is-cur')).toHaveText(/▶ group #gatos/);
  await page.keyboard.press('Enter');
  const chip = page.locator('[data-testid="tree"] .nk-chip', { hasText: '#gatos' });
  await expect(chip).toBeVisible({ timeout: 10_000 });
  // and back (the demo is shared with the other specs)
  await openPalette(page);
  await page.keyboard.type('ungroup gatos');
  await expect(page.locator('.nk-pal-item.is-cur')).toHaveText(/▶ ungroup #gatos/);
  await page.keyboard.press('Enter');
  await expect(chip).toHaveCount(0, { timeout: 10_000 });
});

test('a pending confirmation (session.stop) is a dialog; n cancels', async ({ page }) => {
  const errors = await open(page);
  await cursorTo(page, 'indexador');
  await page.keyboard.press('x');
  const dlg = page.getByTestId('confirm');
  await expect(dlg).toBeVisible();
  await expect(dlg).toContainText('parar "indexador"');
  // modal: j does not move the cursor
  await page.keyboard.press('j');
  await expect(cursorRow(page)).toContainText('indexador');
  const answered = page.waitForRequest((r) => r.url().includes('/v1/confirm/') && r.postDataJSON()?.yes === false);
  await page.keyboard.press('n');
  await answered;
  await expect(dlg).toHaveCount(0);
  // still running
  await expect(cursorRow(page)).toContainText('indexador');
  expect(errors).toEqual([]);
});

test('tabs with 2/1; "/" filters; H shows hidden', async ({ page }) => {
  await open(page);
  await page.keyboard.press('2');
  await expect(page.getByRole('tab', { selected: true })).toContainText('Inbox');
  await expect(page.getByTestId('tree')).toContainText('migração do banco');
  await page.keyboard.press('1');
  await expect(page.getByRole('tab', { selected: true })).toContainText('Sessões');
  await page.keyboard.press('/');
  await expect(page.getByTestId('filter').locator('input')).toBeFocused();
  await page.keyboard.type('dotf');
  await expect(page.getByTestId('tree')).not.toContainText('api-gateway');
  await expect(page.getByTestId('tree')).toContainText('dotfiles');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('tree')).toContainText('api-gateway');
  await expect(page.getByTestId('tree')).not.toContainText('blog antigo');
  await page.keyboard.press('Shift+H');
  await expect(page.getByTestId('tree')).toContainText('blog antigo');
});
