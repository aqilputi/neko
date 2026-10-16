// `npm run demo`: nk serve --demo + the Vite dev server proxying /v1 to it.
// Prints the URL to open (with ?token=). Ctrl-C stops both.

import { createServer } from 'vite';
import { startDemo } from './nk-demo.ts';

const demo = await startDemo();
process.env.NK_ENDPOINT = demo.endpoint;
const vite = await createServer({ server: { port: Number(process.env.PORT) || 5173 } });
await vite.listen();
const base = vite.resolvedUrls?.local[0] ?? 'http://localhost:5173/';
console.log(`nk serve --demo em ${demo.endpoint}`);
console.log(`abra: ${base}?token=${demo.token}`);

let stopping = false;
async function stop() {
  if (stopping) return;
  stopping = true;
  await vite.close();
  await demo.stop();
  process.exit(0);
}
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
