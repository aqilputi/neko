import { effect } from '@preact/signals';
import { render } from 'preact';
import { App } from './App.tsx';
import { createClient, getEndpoint, type NkEvent } from './client/index.ts';
import { feedInboxEvent, installInboxKeys } from './inbox/index.ts';
import { createKeyController } from './keymap/controller.ts';
import { installCursorKeeper } from './keymap/cursor.ts';
import { createUi } from './keymap/ui.ts';
import { toggleSceneFlat } from './scene/index.ts';
import { FALLBACK_THEME, createStore, startSync, themeVars } from './state/index.ts';
import { WindowManager, setWindowManager, setWindowWiring } from './windows/index.ts';
import './style.css';

const store = createStore();
installCursorKeeper(store); // j/k win over rebuilds that were in flight

// the theme's tokens as CSS variables on :root (fallback until /v1/theme)
effect(() => {
  const vars = themeVars(store.state.value.theme ?? FALLBACK_THEME);
  for (const [k, v] of Object.entries(vars)) document.documentElement.style.setProperty(k, v);
});

const root = document.getElementById('app')!;
render(<App store={store} />, root);

const endpoint = await getEndpoint();
const client = createClient(endpoint);
const ui = createUi();
const windows = new WindowManager({ client, store, ui });

const onEvent = (ev: NkEvent) => {
  windows.onEvent(ev);
  feedInboxEvent(ev);
};
const sync = startSync(client, store, { onEvent });

const keys = createKeyController({
  store,
  sync,
  client,
  ui,
  windows,
  skip: new Set(['inbox.summon']), // installInboxKeys owns shift+space
  extra: { 'view.flat': toggleSceneFlat },
  pageRows: () => Math.max(Math.floor((window.innerHeight - 220) / 36), 4),
});
setWindowWiring({ manager: windows, keys, ui });
setWindowManager(windows);
installInboxKeys(window, sync);

/** Keys outside terminals and text fields go to the key controller. */
function isTyping(t: EventTarget | null): boolean {
  return t instanceof Element && !!t.closest('.xterm, input, textarea, select, [contenteditable]');
}
window.addEventListener('keydown', (e) => {
  if (e.defaultPrevented || isTyping(e.target)) return;
  if (keys.handleKey(e)) e.preventDefault();
});

render(<App store={store} client={client} sync={sync} keys={keys} ui={ui} windows={windows} />, root);

// handy from the devtools console (and the e2e tests)
Object.assign(globalThis, { nk: { client, store, sync, keys, ui, windows } });
