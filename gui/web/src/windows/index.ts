// WP-6.2 — terminal windows: draggable DOM windows over the scene, each a
// POST /v1/panes + WS /v1/panes/{id}/ws feeding xterm.js (WebGL renderer,
// DOM fallback). openSessionWindow is what the scene's cats and the inbox
// cat call.

import type { WindowManager } from './manager.ts';

export { WindowLayer, setWindowWiring, getWindowWiring, type WindowLayerProps, type WindowWiring } from './WindowLayer.tsx';
export { WindowManager, type ManagerDeps } from './manager.ts';
export { TermSession, type TermView, type PaneClient } from './term.ts';
export * from './layout.ts';

let current: WindowManager | null = null;
const queued: Array<[string, boolean]> = [];

/** Makes m the manager openSessionWindow talks to (main does it once). */
export function setWindowManager(m: WindowManager | null): void {
  current = m;
  if (m) for (const [id, focus] of queued.splice(0)) m.open(id, focus);
}

/** Opens (or raises) the terminal window of a session, focused (INSERT)
 * unless focus is false — session.open semantics. Calls made before the
 * app is wired are replayed once it is. */
export function openSessionWindow(sessionId: string, focus = true): void {
  if (current) current.open(sessionId, focus);
  else queued.push([sessionId, focus]);
}
