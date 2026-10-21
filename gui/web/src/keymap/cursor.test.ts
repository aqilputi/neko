import { describe, expect, it } from 'vitest';
import type { Tree } from '../client/types.ts';
import { applyTree, patchViewState } from '../state/reducers.ts';
import { createStore } from '../state/store.ts';
import { GW, MIG, tree } from '../test/fixtures.ts';
import { installCursorKeeper, keepCursor } from './cursor.ts';

const view = (t: Tree = tree()) => ({ rows: t.rows ?? [], state: t.state, inbox: [], hidden: 0, archived: 0 });

describe('keepCursor', () => {
  const t = view();
  it.each([
    ['same rows: a local move', { rows: t.rows, key: 'x' }, t, null],
    ['no previous cursor', { rows: [], key: undefined }, t, null],
    ['server agrees', { rows: [], key: 'ctx:acme' }, t, null],
    ['stale rebuild: restore', { rows: [], key: `sess:${MIG}` }, t, { cursor: 2, cursor_key: `sess:${MIG}` }],
    ['row gone: server wins', { rows: [], key: 'sess:gone' }, t, null],
  ])('%s', (_n, memo, next, want) => expect(keepCursor(memo, next)).toEqual(want));
});

describe('installCursorKeeper', () => {
  it('a rebuild answering an older cursor does not move it back', () => {
    const store = createStore();
    store.update(applyTree, tree());
    const off = installCursorKeeper(store);
    store.update(patchViewState, { cursor: 1, cursor_key: `sess:${GW}` }); // j
    store.update(applyTree, tree()); // in flight since before j: cursor ctx:acme
    expect(store.state.value.tree.state.cursor_key).toBe(`sess:${GW}`);
    expect(store.state.value.tree.state.cursor).toBe(1);
    // a rebuild where the row is gone lets the server decide
    const t = tree();
    t.rows = t.rows!.filter((r) => r.key !== `sess:${GW}`);
    store.update(applyTree, t);
    expect(store.state.value.tree.state.cursor_key).toBe('ctx:acme');
    off();
    store.update(patchViewState, { cursor_key: `sess:${MIG}` });
    store.update(applyTree, tree());
    expect(store.state.value.tree.state.cursor_key).toBe('ctx:acme');
  });
});
