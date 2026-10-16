import { defineConfig } from 'vite';
import { nkProxy } from './scripts/proxy.ts';

// The core's RPC server sends no CORS headers, so in dev the page talks to
// its own origin and Vite forwards /v1 (HTTP, SSE and the pane WebSockets)
// to the `nk serve` named by NK_ENDPOINT (http://127.0.0.1:PORT).
export default defineConfig({
  oxc: {
    jsx: { runtime: 'automatic', importSource: 'preact' },
  },
  server: {
    proxy: nkProxy(process.env.NK_ENDPOINT),
  },
  build: {
    outDir: 'dist',
    target: 'es2023',
    reportCompressedSize: true,
  },
});
