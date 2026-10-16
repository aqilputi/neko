// Screenshots of the real GUI the way a user opens it: nkg --browser --demo.
// usage: node scripts/shot.mjs <outdir>   (needs `npx vite build` first)
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { chromium } from '@playwright/test';

const out = process.argv[2] ?? '.';
const tmp = mkdtempSync(join(tmpdir(), 'nkshot-'));
const env = { ...process.env, HOME: tmp, XDG_STATE_HOME: join(tmp, 's'), XDG_CONFIG_HOME: join(tmp, 'c'),
  XDG_CACHE_HOME: join(tmp, 'k'), XDG_RUNTIME_DIR: tmp, NEKO_CLAUDE: '/bin/false' };
const go = process.env.GO ?? 'go';
const nkg = spawn(go, ['run', './cmd/nekuroddo-gui', '--browser', '--demo', '--dev', '--no-open'],
  { cwd: '../..', env, stdio: ['ignore', 'pipe', 'pipe'] });

const link = await new Promise((resolve, reject) => {
  let buf = '';
  const onData = (d) => {
    buf += d;
    const m = buf.match(/https?:\/\/127\.0\.0\.1:\d+\/\S+/);
    if (m) resolve(m[0]);
  };
  nkg.stdout.on('data', onData);
  nkg.stderr.on('data', onData);
  nkg.on('exit', (c) => reject(new Error(`nkg exited ${c}: ${buf}`)));
  setTimeout(() => reject(new Error(`no link: ${buf}`)), 60_000);
});

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.goto(link);
await page.waitForFunction(() => globalThis.__nkScene?.cats > 0, null, { timeout: 30_000 });
await page.waitForTimeout(6000); // let the inbox cat walk in
await page.screenshot({ path: join(out, 'gui-1-scene.png') });
await page.keyboard.press('j'); // from the context row to its first session
await page.keyboard.press('Enter'); // open it
await page.waitForTimeout(2500);
await page.screenshot({ path: join(out, 'gui-2-window.png') });
console.log(JSON.stringify({ link: link.replace(/\?.*/, '?…'), errors, cats: await page.evaluate(() => globalThis.__nkScene?.cats) }));
await browser.close();
nkg.kill('SIGTERM');
