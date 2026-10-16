// Starts `nk serve --demo` on tcp://127.0.0.1:0 (isolated XDG dirs, token
// read from the temp XDG_RUNTIME_DIR) and the Vite dev server proxying /v1
// to it — the core sends no CORS headers, so the page must be same-origin.
// Tests read NK_GUI_URL, NK_ENDPOINT and NK_TOKEN.

import { createServer } from 'vite';
import { startDemo } from '../../scripts/nk-demo.ts';

export default async function globalSetup() {
  const demo = await startDemo();
  process.env.NK_ENDPOINT = demo.endpoint;
  process.env.NK_TOKEN = demo.token;
  const vite = await createServer({
    configFile: new URL('../../vite.config.ts', import.meta.url).pathname,
    server: { port: 0, strictPort: false, host: '127.0.0.1' },
    logLevel: 'warn',
  });
  await vite.listen();
  const url = vite.resolvedUrls?.local[0];
  if (!url) throw new Error('vite não informou a URL');
  process.env.NK_GUI_URL = url;
  return async () => {
    await vite.close();
    await demo.stop();
  };
}
