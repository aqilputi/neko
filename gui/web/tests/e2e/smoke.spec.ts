import { expect, test } from '@playwright/test';

function pageURL(): string {
  const base = process.env.NK_GUI_URL!;
  const u = new URL(base);
  // same-origin endpoint: Vite proxies /v1 to nk serve --demo
  u.searchParams.set('endpoint', u.origin);
  u.searchParams.set('token', process.env.NK_TOKEN!);
  return u.toString();
}

test('shows the demo sessions from nk serve --demo', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(pageURL());
  const tree = page.getByTestId('tree');
  await expect(tree).toContainText('api-gateway');
  await expect(tree).toContainText('migração do banco');
  await expect(tree).toContainText('acme');
  // the event stream is live
  await expect(page.getByTestId('status')).toHaveText('conectado');
  // theme tokens became CSS variables
  const bg = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--nk-bg').trim());
  expect(bg).toMatch(/^#[0-9a-f]{6}$/i);
  expect(errors).toEqual([]);
});

test('a wrong token never leaves "conectando…"', async ({ page }) => {
  const u = new URL(pageURL());
  u.searchParams.set('token', 'errado');
  await page.goto(u.toString());
  await expect(page.getByTestId('status')).toHaveText('conectando…');
  await expect(page.locator('.error')).toContainText(/token|autoriz|unauthorized/i);
});
