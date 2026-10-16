// Vite dev-server proxy for the core's protocol (/v1 → NK_ENDPOINT),
// including the pane WebSockets. Shared by vite.config.ts and the e2e /
// demo launchers.
export function nkProxy(target: string | undefined) {
  if (!target) return undefined;
  return {
    '/v1': { target, ws: true, changeOrigin: true },
  };
}
