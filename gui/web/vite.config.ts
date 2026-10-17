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
    // the three chunk is WebGLRenderer itself (~550 kB minified, ~136 kB
    // gzip — within plan §4.7's ~150 kB for three.js); tree-shaking can't
    // shrink it, so warn only above that
    chunkSizeWarningLimit: 600,
    // vendors in their own chunks: three.js alone is ~500 kB minified, and
    // separate chunks cache across releases of our own code
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            { name: 'three', test: /node_modules[\\/]three[\\/]/ },
            { name: 'xterm', test: /node_modules[\\/]@xterm[\\/]/ },
            { name: 'preact', test: /node_modules[\\/](preact|@preact)[\\/]/ },
          ],
        },
      },
    },
  },
});
