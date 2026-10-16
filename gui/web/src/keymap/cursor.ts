// The tree cursor is the client's: a rebuild (POST /v1/view/tree) answers
// with the state it was sent, so a rebuild that was in flight while the
// user pressed j would put the cursor back where it was. When the rows
// come back from the server, the cursor the user last had wins, as long
// as its row is still listed (a filtered-out row lets the server's choice
// stand).

import { effect } from '@preact/signals';
import type { TreeRow, ViewState } from '../client/types.ts';
import type { TreeView } from '../state/model.ts';
import { patchViewState } from '../state/reducers.ts';
import type { Store } from '../state/store.ts';

export interface CursorMemo {
  rows: readonly TreeRow[];
  key: string | undefined;
}

/** The patch that restores the user's cursor after a rebuild, or null. */
export function keepCursor(prev: CursorMemo, next: TreeView): ViewState | null {
  if (next.rows === prev.rows || !prev.key || next.state.cursor_key === prev.key) return null;
  const i = next.rows.findIndex((r) => r.key === prev.key);
  return i < 0 ? null : { cursor: i, cursor_key: prev.key };
}

/** Keeps the user's cursor across rebuilds; returns the disposer. */
export function installCursorKeeper(store: Store): () => void {
  let memo: CursorMemo = { rows: store.state.value.tree.rows, key: store.state.value.tree.state.cursor_key };
  return effect(() => {
    const t = store.state.value.tree;
    const fix = keepCursor(memo, t);
    memo = { rows: t.rows, key: fix?.cursor_key ?? t.state.cursor_key };
    if (fix) store.update(patchViewState, fix);
  });
}
