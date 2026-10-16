// The GUI's composition (plan §4.1): the WebGL scene behind, the terminal
// windows over it, the DOM chrome (top bar, tree column, status bar,
// overlays, toasts) and the calçada with the inbox cat. "conectando…"
// until the first snapshot and tree arrive.

import { useEffect } from 'preact/hooks';
import type { NkClient } from './client/client.ts';
import { Chrome } from './chrome/index.ts';
import { metricVars } from './chrome/metrics.ts';
import { InboxWalk } from './inbox/index.ts';
import type { KeyController } from './keymap/controller.ts';
import type { Ui } from './keymap/ui.ts';
import { Scene } from './scene/index.ts';
import type { Store, Sync } from './state/index.ts';
import { WindowLayer, openSessionWindow, type WindowManager } from './windows/index.ts';

export interface AppProps {
  store: Store;
  client?: NkClient;
  sync?: Sync;
  keys?: KeyController;
  ui?: Ui;
  windows?: WindowManager;
}

/** Where the inbox cat stops for a session: under its window when one is
 * open, else under its cat in the scene (screenPos is canvas-local: the
 * scene layer's offset is added). */
export function positionOf(windows: WindowManager | undefined, id: string): number | undefined {
  const w = windows?.state.value.wins.find((x) => x.id === id);
  if (w) return w.x + w.w / 2;
  const scene = (globalThis as { __nkScene?: { screenPos(id: string): { x: number } | null } }).__nkScene;
  const p = scene?.screenPos(id);
  if (!p) return undefined;
  const layer = typeof document === 'undefined' ? null : document.querySelector('.nk-scene-layer');
  return p.x + (layer?.getBoundingClientRect().left ?? 0);
}

export function App({ store, client, sync, keys, ui, windows }: AppProps) {
  const s = store.state.value;

  useEffect(() => {
    for (const [k, v] of Object.entries(metricVars())) document.documentElement.style.setProperty(k, v);
  }, []);

  if (!s.ready) {
    return (
      <main class="boot">
        <p data-testid="status">conectando…</p>
        {s.error && <p class="error">{s.error}</p>}
      </main>
    );
  }
  const treeOpen = ui?.treeOpen.value ?? true;
  return (
    <div class={`nk-app${treeOpen ? '' : ' tree-closed'}`}>
      <div class="nk-scene-layer">
        <Scene store={store} onOpenSession={(id) => openSessionWindow(id)} />
      </div>
      <WindowLayer store={store} {...(client ? { client } : {})} {...(sync ? { sync } : {})} />
      <Chrome store={store} {...(client ? { client } : {})} {...(sync ? { sync } : {})} />
      <InboxWalk store={store} sync={sync} onOpenSession={(id) => openSessionWindow(id)} positionOf={(id) => positionOf(windows, id)} />
    </div>
  );
}
