// Dev/e2e harness for the scene alone (served by Vite at
// /src/scene/dev.html, not part of the build): the same store + sync the
// app uses and <Scene> filling the window. Opening a session only records
// it (globalThis.nk.opened and the URL hash #open=<id>). `?flat=1` shows
// the flat mode.

import { effect } from '@preact/signals';
import { render } from 'preact';
import { createClient, getEndpoint } from '../client/index.ts';
import { FALLBACK_THEME, createStore, startSync, themeVars } from '../state/index.ts';
import '../style.css';
import { Scene } from './index.ts';

const store = createStore();
effect(() => {
  const vars = themeVars(store.state.value.theme ?? FALLBACK_THEME);
  for (const [k, v] of Object.entries(vars)) document.documentElement.style.setProperty(k, v);
});

const opened: string[] = [];
function onOpenSession(id: string) {
  opened.push(id);
  location.hash = `open=${id}`;
}

render(<Scene store={store} onOpenSession={onOpenSession} />, document.getElementById('app')!);

const client = createClient(await getEndpoint());
const sync = startSync(client, store);
Object.assign(globalThis, { nk: { client, store, sync, opened } });
