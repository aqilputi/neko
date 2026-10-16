// The client-scope commands over the tree (nav.*, tab.*, view.*), as pure
// functions of the tree view: each answers the ViewState patch to apply
// (and whether the server must rebuild the tree), mirroring the TUI's
// nav.go. A cursor move needs no rebuild; folds, tabs, filter, show_all
// and sort do.

import type { SortMode, TabName, TreeRow, ViewState } from '../client/types.ts';
import type { TreeView } from '../state/model.ts';

export const TABS: readonly TabName[] = ['sessions', 'inbox', 'todos', 'archive'];
export const SORTS: readonly SortMode[] = ['atividade', 'custo', 'nome'];

/** What a client command does to the view. */
export interface NavStep {
  patch: ViewState;
  /** The server must rebuild the tree (POST /v1/view/tree). */
  rebuild: boolean;
}

/** Index of the row under the cursor (cursor_key first, as the store). */
export function cursorIndex(tree: TreeView): number {
  const { rows, state } = tree;
  if (state.cursor_key) {
    const i = rows.findIndex((r) => r.key === state.cursor_key);
    if (i >= 0) return i;
  }
  const c = state.cursor ?? 0;
  return rows.length ? Math.min(Math.max(c, 0), rows.length - 1) : 0;
}

function at(rows: readonly TreeRow[], i: number): ViewState {
  const r = rows[i];
  return r ? { cursor: i, cursor_key: r.key } : { cursor: 0 };
}

/** Moves the cursor by delta rows, clamped. */
export function moveCursor(tree: TreeView, delta: number): NavStep {
  const n = tree.rows.length;
  if (n === 0) return { patch: {}, rebuild: false };
  const i = Math.min(Math.max(cursorIndex(tree) + delta, 0), n - 1);
  return { patch: at(tree.rows, i), rebuild: false };
}

/** Puts the cursor on the row with key (no-op when it is not listed). */
export function cursorTo(tree: TreeView, key: string): NavStep {
  const i = tree.rows.findIndex((r) => r.key === key);
  return i < 0 ? { patch: {}, rebuild: false } : { patch: at(tree.rows, i), rebuild: false };
}

/** Puts the cursor on the session's row. */
export function cursorToSession(tree: TreeView, sessionId: string): NavStep {
  const r = tree.rows.find((x) => x.kind === 'sess' && x.session_id === sessionId);
  return r ? cursorTo(tree, r.key) : { patch: {}, rebuild: false };
}

function hasChildren(rows: readonly TreeRow[], i: number): boolean {
  const r = rows[i];
  const next = rows[i + 1];
  return !!r && !!next && next.depth > r.depth;
}

function foldable(tab: string | undefined): boolean {
  return tab === undefined || tab === 'sessions' || tab === 'archive';
}

/** h: folds the context (or the session's subagents) under the cursor. */
export function collapse(tree: TreeView): NavStep {
  const i = cursorIndex(tree);
  const r = tree.rows[i];
  if (!r || !foldable(tree.state.tab)) return { patch: {}, rebuild: false };
  const folded = { ...(tree.state.collapsed ?? {}) };
  let cursorKey: string;
  switch (r.kind) {
    case 'ctx':
      folded[r.key] = true;
      cursorKey = r.key;
      break;
    case 'sess':
      if (hasChildren(tree.rows, i) && !r.collapsed) {
        folded[r.key] = true;
        cursorKey = r.key;
      } else {
        folded[`ctx:${r.ctx}`] = true;
        cursorKey = `ctx:${r.ctx}`;
      }
      break;
    default:
      folded[`sess:${r.session_id}`] = true;
      cursorKey = `sess:${r.session_id}`;
  }
  return { patch: { collapsed: folded, cursor_key: cursorKey }, rebuild: true };
}

/** l: unfolds the row under the cursor. */
export function expand(tree: TreeView): NavStep {
  const r = tree.rows[cursorIndex(tree)];
  if (!r || r.kind === 'sub' || !foldable(tree.state.tab)) return { patch: {}, rebuild: false };
  const folded = { ...(tree.state.collapsed ?? {}) };
  delete folded[r.key];
  return { patch: { collapsed: folded }, rebuild: true };
}

/** a / A: the next (dir 1) or previous (-1) session waiting on you, wrapping. */
export function jumpAttention(tree: TreeView, dir: 1 | -1): NavStep {
  const n = tree.rows.length;
  if (tree.state.tab === 'inbox') return moveCursor(tree, dir);
  const cur = cursorIndex(tree);
  for (let k = 1; k <= n; k++) {
    const j = (((cur + dir * k) % n) + n) % n;
    const r = tree.rows[j]!;
    if (r.kind === 'sess' && r.attention !== 0) return { patch: at(tree.rows, j), rebuild: false };
  }
  return { patch: {}, rebuild: false };
}

/** 1–4: a tab. */
export function toTab(tab: TabName): NavStep {
  return { patch: { tab }, rebuild: true };
}

/** tab / shift+tab: the next or previous tab. */
export function cycleTab(tree: TreeView, dir: 1 | -1): NavStep {
  const i = Math.max(TABS.indexOf(tree.state.tab ?? 'sessions'), 0);
  return toTab(TABS[(i + dir + TABS.length) % TABS.length]!);
}

/** H: toggles showing stale/archived sessions and finished subagents. */
export function toggleShowAll(tree: TreeView): NavStep {
  return { patch: { show_all: !tree.state.show_all }, rebuild: true };
}

/** S: the next sort mode, re-sorting once. */
export function nextSort(tree: TreeView): NavStep {
  const i = Math.max(SORTS.indexOf(tree.state.sort ?? 'atividade'), 0);
  return { patch: { sort: SORTS[(i + 1) % SORTS.length]!, reset_order: true }, rebuild: true };
}

/** The filter typed after "/". */
export function setFilter(filter: string): NavStep {
  return { patch: { filter }, rebuild: true };
}

/**
 * space: toggles the session under the cursor in the picked set; on a
 * context row, picks all its sessions (or unpicks them when all are).
 */
export function togglePick(tree: TreeView, picked: ReadonlySet<string>): Set<string> {
  const out = new Set(picked);
  const r = tree.rows[cursorIndex(tree)];
  if (!r) return out;
  if (r.kind === 'ctx') {
    const ids = tree.rows.filter((x) => x.kind === 'sess' && x.ctx === r.ctx && x.session_id).map((x) => x.session_id!);
    const all = ids.length > 0 && ids.every((id) => out.has(id));
    for (const id of ids) {
      if (all) out.delete(id);
      else out.add(id);
    }
    return out;
  }
  if (r.session_id) {
    if (out.has(r.session_id)) out.delete(r.session_id);
    else out.add(r.session_id);
  }
  return out;
}

/** The next theme name after current, wrapping. */
export function nextTheme(themes: readonly string[], current: string): string | undefined {
  if (themes.length === 0) return undefined;
  const i = themes.indexOf(current);
  return themes[(i + 1) % themes.length];
}
