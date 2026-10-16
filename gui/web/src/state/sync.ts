// Keeps a Store in step with the core: bootstrap (hello, theme, keymap,
// commands, panes), the event stream, and debounced resyncs (snapshot +
// tree) whenever a reducer says the state is behind (needsResync) or the
// stream reports a gap. The only impure part of src/state.

import type { NkClient } from '../client/client.ts';
import type { EventStream, StreamStatus } from '../client/events.ts';
import type { NkEvent, Result, ViewState } from '../client/types.ts';
import type { ConnStatus } from './model.ts';
import {
  applyCommands,
  applyEvent,
  applyHello,
  applyKeymap,
  applyPanes,
  applyResult,
  applyResync,
  applySnapshot,
  applyTheme,
  applyTree,
  markReady,
  patchViewState,
  setError,
  setStatus,
} from './reducers.ts';
import type { Store } from './store.ts';

/** The client calls sync needs (an NkClient, or a fake). */
export type SyncClient = Pick<
  NkClient,
  'hello' | 'theme' | 'keymap' | 'commands' | 'panes' | 'snapshot' | 'viewTree' | 'events' | 'run' | 'confirm'
>;

export interface SyncOptions {
  /** Coalesces bursts of events into one resync (ms). */
  resyncDelayMs?: number;
  /** Wait before retrying a failed bootstrap (ms). */
  retryMs?: number;
  /** Every bus event, after the store applied it (inbox, scene cues…). */
  onEvent?(ev: NkEvent): void;
}

export interface Sync {
  /** Re-reads snapshot and tree now. */
  resync(): Promise<void>;
  /** Patches the view state (tab, filter, folds, cursor…) and rebuilds the tree. */
  refreshTree(patch?: ViewState): Promise<void>;
  /** Runs a command for the current selection and applies its Result. */
  run(name: string, args?: Record<string, string>): Promise<Result>;
  /** Answers a pending confirmation. */
  confirm(token: string, yes: boolean): Promise<Result>;
  stop(): void;
}

const streamStatus: Record<StreamStatus, ConnStatus> = {
  connecting: 'connecting',
  open: 'open',
  reconnecting: 'reconnecting',
  closed: 'closed',
};

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function startSync(client: SyncClient, store: Store, opts: SyncOptions = {}): Sync {
  const resyncDelay = opts.resyncDelayMs ?? 150;
  const retryMs = opts.retryMs ?? 2000;
  let stream: EventStream | null = null;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let inflight: Promise<void> | null = null;
  let again = false;
  let localIds = -1; // toast ids for command results (bus events use seq ≥ 1)

  const st = () => store.state.value;

  async function doResync(): Promise<void> {
    try {
      const [snap, tree] = await Promise.all([client.snapshot(), client.viewTree(st().tree.state)]);
      if (stopped) return;
      store.update(applySnapshot, snap);
      store.update(applyTree, tree);
      store.update(markReady);
    } catch (e) {
      if (!stopped) store.update(setError, message(e));
    }
  }

  function resync(): Promise<void> {
    if (inflight) {
      again = true;
      return inflight;
    }
    inflight = (async () => {
      do {
        again = false;
        await doResync();
      } while (again && !stopped);
      inflight = null;
    })();
    return inflight;
  }

  function scheduleResync(): void {
    if (timer !== null || stopped) return;
    timer = setTimeout(() => {
      timer = null;
      void resync();
    }, resyncDelay);
  }

  function onEvent(ev: NkEvent): void {
    store.update(applyEvent, ev);
    opts.onEvent?.(ev);
    if (st().needsResync) scheduleResync();
  }

  let opened = false;
  function openStream(): void {
    stream = client.events({
      onEvent,
      onResync: () => {
        store.update(applyResync);
        scheduleResync();
      },
      onStatus: (s) => {
        store.update(setStatus, streamStatus[s]);
        if (s === 'open' && !opened) {
          // events published before the stream opened are not replayed:
          // read the world once the stream is live
          opened = true;
          void resync();
        }
      },
    });
  }

  async function bootstrap(): Promise<void> {
    store.update(setStatus, 'connecting');
    try {
      const [hello, theme, keymap, commands, panes] = await Promise.all([
        client.hello(),
        client.theme(),
        client.keymap(),
        client.commands(),
        client.panes(),
      ]);
      if (stopped) return;
      store.update(applyHello, hello);
      store.update(applyTheme, theme);
      store.update(applyKeymap, keymap);
      store.update(applyCommands, commands);
      store.update(applyPanes, panes);
      store.update(setError, null);
      openStream();
      await resync();
    } catch (e) {
      if (stopped) return;
      store.update(setError, message(e));
      store.update(setStatus, 'error');
      timer = setTimeout(() => {
        timer = null;
        void bootstrap();
      }, retryMs);
    }
  }

  void bootstrap();

  function context() {
    const s = st();
    const row = s.tree.rows.find((r) => r.key === s.tree.state.cursor_key) ?? s.tree.rows[s.tree.state.cursor ?? 0];
    const ctx: { session_id?: string; tab?: string } = {};
    if (row?.session_id) ctx.session_id = row.session_id;
    if (s.tree.state.tab) ctx.tab = s.tree.state.tab;
    return ctx;
  }

  return {
    resync,
    async refreshTree(patch = {}) {
      store.update(patchViewState, patch);
      try {
        const tree = await client.viewTree(st().tree.state);
        if (!stopped) store.update(applyTree, tree);
      } catch (e) {
        if (!stopped) store.update(setError, message(e));
      }
    },
    async run(name, args) {
      const r = await client.run(name, args, context());
      store.update(applyResult, r, localIds--, new Date().toISOString());
      return r;
    },
    async confirm(token, yes) {
      const r = await client.confirm(token, yes);
      store.update(applyResult, r, localIds--, new Date().toISOString());
      return r;
    },
    stop() {
      stopped = true;
      if (timer !== null) clearTimeout(timer);
      timer = null;
      stream?.close();
      store.update(setStatus, 'closed');
    },
  };
}
