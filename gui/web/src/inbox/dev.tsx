// Dev/e2e harness for the inbox cat alone (served by Vite at
// /src/inbox/dev.html, not part of the build): the same store + sync the
// app uses, wired the way App/main should wire the cat — InboxWalk,
// installInboxKeys(window, sync) and onEvent → feedInboxEvent, listening
// to inbox.summoned as well. Opening a session only records it in the URL
// hash (#open=<id>) and the calçada's data-last-open.

import { effect } from '@preact/signals';
import { render } from 'preact';
import { createClient, EVENT_NAMES, getEndpoint, type EventStreamHandlers, type EventStreamOptions } from '../client/index.ts';
import { FALLBACK_THEME, createStore, startSync, themeVars } from '../state/index.ts';
import '../style.css';
import { EV_INBOX_SUMMONED, feedInboxEvent, InboxWalk, installInboxKeys } from './index.ts';

const store = createStore();
effect(() => {
  const vars = themeVars(store.state.value.theme ?? FALLBACK_THEME);
  for (const [k, v] of Object.entries(vars)) document.documentElement.style.setProperty(k, v);
});

const client = createClient(await getEndpoint());
const names = [...EVENT_NAMES, EV_INBOX_SUMMONED];
const withSummoned = Object.assign(Object.create(client) as typeof client, {
  events: (h: EventStreamHandlers, o: EventStreamOptions = {}) => client.events(h, { names, ...o }),
});
const sync = startSync(withSummoned, store, { onEvent: feedInboxEvent });
installInboxKeys(window, sync);

const opened: string[] = [];
function onOpenSession(id: string) {
  opened.push(id);
  location.hash = `open=${id}`;
}

render(<InboxWalk store={store} sync={sync} onOpenSession={onOpenSession} />, document.getElementById('app')!);
Object.assign(globalThis, { nk: { client, store, sync, opened } });
