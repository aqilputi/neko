import { describe, expect, it } from 'vitest';
import type { TreeRow, ViewState } from '../client/types.ts';
import type { TreeView } from '../state/model.ts';
import {
  collapse,
  cursorIndex,
  cursorTo,
  cursorToSession,
  cycleTab,
  expand,
  jumpAttention,
  moveCursor,
  nextSort,
  nextTheme,
  setFilter,
  toTab,
  toggleShowAll,
  togglePick,
} from './nav.ts';

function row(key: string, kind: TreeRow['kind'], depth: number, extra: Partial<TreeRow> = {}): TreeRow {
  const ctx = extra.ctx ?? 'acme';
  return { key, kind, ctx, depth, color: 0, attention: 0, ...extra };
}

// acme: gw (2 subs) · mig (blocked) ; oss: idx ; pessoal: dot (idle)
const ROWS: TreeRow[] = [
  row('ctx:acme', 'ctx', 0),
  row('sess:gw', 'sess', 1, { session_id: 'gw' }),
  row('sub:a', 'sub', 2, { session_id: 'gw' }),
  row('sub:b', 'sub', 2, { session_id: 'gw' }),
  row('sess:mig', 'sess', 1, { session_id: 'mig', attention: 1 }),
  row('ctx:oss', 'ctx', 0, { ctx: 'oss' }),
  row('sess:idx', 'sess', 1, { ctx: 'oss', session_id: 'idx' }),
  row('ctx:pessoal', 'ctx', 0, { ctx: 'pessoal' }),
  row('sess:dot', 'sess', 1, { ctx: 'pessoal', session_id: 'dot', attention: 3 }),
];

function tree(state: ViewState = {}, rows = ROWS): TreeView {
  return { rows, state: { tab: 'sessions', ...state }, inbox: [], hidden: 0, archived: 0 };
}

describe('cursorIndex', () => {
  it.each([
    [{ cursor_key: 'sess:mig', cursor: 0 }, 4],
    [{ cursor_key: 'gone', cursor: 2 }, 2],
    [{ cursor: 99 }, 8],
    [{ cursor: -3 }, 0],
    [{}, 0],
  ] as Array<[ViewState, number]>)('%j → %d', (st, want) => expect(cursorIndex(tree(st))).toBe(want));
  it('empty tree', () => expect(cursorIndex(tree({}, []))).toBe(0));
});

describe('moveCursor', () => {
  it.each([
    [0, 1, 1, 'sess:gw'],
    [1, -1, 0, 'ctx:acme'],
    [0, -5, 0, 'ctx:acme'],
    [7, 1 << 30, 8, 'sess:dot'],
    [8, -(1 << 30), 0, 'ctx:acme'],
  ])('from %d by %d → %d', (from, d, want, key) => {
    expect(moveCursor(tree({ cursor: from }), d)).toEqual({ patch: { cursor: want, cursor_key: key }, rebuild: false });
  });
  it('empty tree does nothing', () => expect(moveCursor(tree({}, []), 1)).toEqual({ patch: {}, rebuild: false }));
});

describe('cursorTo', () => {
  it('finds a key', () => expect(cursorTo(tree(), 'sess:idx').patch).toEqual({ cursor: 6, cursor_key: 'sess:idx' }));
  it('unknown key', () => expect(cursorTo(tree(), 'x').patch).toEqual({}));
  it('session id', () => expect(cursorToSession(tree(), 'dot').patch).toEqual({ cursor: 8, cursor_key: 'sess:dot' }));
  it('unknown session', () => expect(cursorToSession(tree(), 'zz').patch).toEqual({}));
});

describe('collapse / expand', () => {
  it.each([
    ['ctx row folds itself', 'ctx:acme', { 'ctx:acme': true }, 'ctx:acme'],
    ['session with subs folds them', 'sess:gw', { 'sess:gw': true }, 'sess:gw'],
    ['session without subs folds its context', 'sess:mig', { 'ctx:acme': true }, 'ctx:acme'],
    ['sub folds its session', 'sub:b', { 'sess:gw': true }, 'sess:gw'],
  ])('%s', (_n, key, folded, cursorKey) => {
    expect(collapse(tree({ cursor_key: key }))).toEqual({ patch: { collapsed: folded, cursor_key: cursorKey }, rebuild: true });
  });

  it('a folded session folds its context instead', () => {
    const rows = ROWS.map((r) => (r.key === 'sess:gw' ? { ...r, collapsed: true } : r));
    expect(collapse(tree({ cursor_key: 'sess:gw' }, rows)).patch.collapsed).toEqual({ 'ctx:acme': true });
  });

  it('keeps earlier folds', () => {
    const st = collapse(tree({ cursor_key: 'ctx:oss', collapsed: { 'ctx:acme': true } }));
    expect(st.patch.collapsed).toEqual({ 'ctx:acme': true, 'ctx:oss': true });
  });

  it('does nothing on the inbox tab or an empty tree', () => {
    expect(collapse(tree({ tab: 'inbox' }))).toEqual({ patch: {}, rebuild: false });
    expect(collapse(tree({}, []))).toEqual({ patch: {}, rebuild: false });
  });

  it('expand drops the fold', () => {
    const st = expand(tree({ cursor_key: 'ctx:acme', collapsed: { 'ctx:acme': true, 'ctx:oss': true } }));
    expect(st).toEqual({ patch: { collapsed: { 'ctx:oss': true } }, rebuild: true });
  });

  it('expand on a sub or the todos tab does nothing', () => {
    expect(expand(tree({ cursor_key: 'sub:a' })).rebuild).toBe(false);
    expect(expand(tree({ tab: 'todos' })).rebuild).toBe(false);
  });
});

describe('jumpAttention', () => {
  it.each([
    [0, 1, 4],
    [4, 1, 8],
    [8, 1, 4], // wraps
    [4, -1, 8], // wraps backwards
    [6, -1, 4],
  ] as Array<[number, 1 | -1, number]>)('from %d dir %d → %d', (from, dir, want) => {
    expect(jumpAttention(tree({ cursor: from }), dir).patch.cursor).toBe(want);
  });
  it('nobody waiting', () => {
    const rows = ROWS.map((r) => ({ ...r, attention: 0 }));
    expect(jumpAttention(tree({}, rows), 1).patch).toEqual({});
  });
  it('on the inbox tab moves by one', () => {
    expect(jumpAttention(tree({ tab: 'inbox', cursor: 1 }), 1).patch.cursor).toBe(2);
  });
});

describe('tabs, sort, filter, show_all', () => {
  it('toTab', () => expect(toTab('todos')).toEqual({ patch: { tab: 'todos' }, rebuild: true }));
  it.each([
    ['sessions', 1, 'inbox'],
    ['archive', 1, 'sessions'],
    ['sessions', -1, 'archive'],
    ['todos', -1, 'inbox'],
  ] as const)('cycle from %s by %d → %s', (from, dir, want) => {
    expect(cycleTab(tree({ tab: from }), dir).patch.tab).toBe(want);
  });
  it('cycle with no tab starts at sessions', () => {
    expect(cycleTab({ ...tree(), state: {} }, 1).patch.tab).toBe('inbox');
  });
  it('show_all toggles', () => {
    expect(toggleShowAll(tree()).patch).toEqual({ show_all: true });
    expect(toggleShowAll(tree({ show_all: true })).patch).toEqual({ show_all: false });
  });
  it.each([
    [undefined, 'custo'],
    ['custo', 'nome'],
    ['nome', 'atividade'],
  ] as const)('sort %s → %s', (from, want) => {
    expect(nextSort(tree(from ? { sort: from } : {})).patch).toEqual({ sort: want, reset_order: true });
  });
  it('filter', () => expect(setFilter('api')).toEqual({ patch: { filter: 'api' }, rebuild: true }));
});

describe('togglePick', () => {
  it('toggles the session under the cursor', () => {
    const a = togglePick(tree({ cursor_key: 'sess:mig' }), new Set());
    expect([...a]).toEqual(['mig']);
    expect([...togglePick(tree({ cursor_key: 'sess:mig' }), a)]).toEqual([]);
  });
  it('a sub picks its session', () => {
    expect([...togglePick(tree({ cursor_key: 'sub:a' }), new Set())]).toEqual(['gw']);
  });
  it('a context picks all, then unpicks all', () => {
    const a = togglePick(tree({ cursor_key: 'ctx:acme' }), new Set(['mig', 'dot']));
    expect([...a].sort()).toEqual(['dot', 'gw', 'mig']);
    const b = togglePick(tree({ cursor_key: 'ctx:acme' }), a);
    expect([...b]).toEqual(['dot']);
  });
  it('empty tree', () => expect(togglePick(tree({}, []), new Set(['x'])).size).toBe(1));
});

describe('nextTheme', () => {
  it.each([
    [['a', 'b', 'c'], 'a', 'b'],
    [['a', 'b', 'c'], 'c', 'a'],
    [['a', 'b'], 'zz', 'a'],
    [[], 'a', undefined],
  ] as Array<[string[], string, string | undefined]>)('%j after %s → %s', (ts, cur, want) => {
    expect(nextTheme(ts, cur)).toBe(want);
  });
});
