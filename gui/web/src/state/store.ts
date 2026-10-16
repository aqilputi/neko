// The store: one signal holding NkState, updated only through the pure
// reducers, plus a few computed selectors the other modules share.

import { computed, signal, type ReadonlySignal } from '@preact/signals';
import type { Session, TreeRow } from '../client/types.ts';
import { initialState, type NkState } from './model.ts';

export interface Store {
  /** The whole state; read .value in components/effects to subscribe. */
  readonly state: ReadonlySignal<NkState>;
  /** Replaces the state with fn(state, ...args) — fn must be a reducer. */
  update<A extends unknown[]>(fn: (s: NkState, ...args: A) => NkState, ...args: A): void;
  /** Sessions of the last snapshot by session_id. */
  readonly sessionsById: ReadonlySignal<ReadonlyMap<string, Session>>;
  /** Tree rows (shorthand for state.tree.rows). */
  readonly rows: ReadonlySignal<readonly TreeRow[]>;
  /** The row under the cursor, if any. */
  readonly cursorRow: ReadonlySignal<TreeRow | undefined>;
}

export function createStore(init: NkState = initialState()): Store {
  const state = signal(init);
  const sessionsById = computed(() => {
    const m = new Map<string, Session>();
    for (const s of state.value.snapshot?.sessions ?? []) m.set(s.session_id, s);
    return m;
  });
  const rows = computed(() => state.value.tree.rows);
  const cursorRow = computed(() => {
    const t = state.value.tree;
    if (t.state.cursor_key) {
      const r = t.rows.find((x) => x.key === t.state.cursor_key);
      if (r) return r;
    }
    return t.rows[t.state.cursor ?? 0];
  });
  return {
    state,
    update(fn, ...args) {
      state.value = fn(state.value, ...args);
    },
    sessionsById,
    rows,
    cursorRow,
  };
}
