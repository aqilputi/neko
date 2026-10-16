import { effect } from '@preact/signals';
import { render } from 'preact';
import { App } from './App.tsx';
import { createClient, getEndpoint } from './client/index.ts';
import { FALLBACK_THEME, createStore, startSync, themeVars } from './state/index.ts';
import './style.css';

const store = createStore();

// the theme's tokens as CSS variables on :root (fallback until /v1/theme)
effect(() => {
  const vars = themeVars(store.state.value.theme ?? FALLBACK_THEME);
  for (const [k, v] of Object.entries(vars)) document.documentElement.style.setProperty(k, v);
});

render(<App store={store} />, document.getElementById('app')!);

const endpoint = await getEndpoint();
const client = createClient(endpoint);
const sync = startSync(client, store);

// handy from the devtools console while the other WPs land
Object.assign(globalThis, { nk: { client, store, sync } });
