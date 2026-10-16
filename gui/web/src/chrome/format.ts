// What the chrome shows, as pure functions of the state: the header's
// counts and usage bar (the TUI's header()), a tree row's parts, the
// status bar's key hints. No DOM.

import type { Binding, Limit, Session, TreeRow, Usage } from '../client/types.ts';
import { keysOf } from '../keymap/resolve.ts';
import { stateGlyph } from './palette.ts';

export interface Counts {
  busy: number;
  waiting: number;
  done: number;
}

/** The TUI's counts(): running, waiting on you (blocked or idle), returned. */
export function counts(sessions: readonly Session[]): Counts {
  const c: Counts = { busy: 0, waiting: 0, done: 0 };
  for (const s of sessions) {
    if (s.state === 'busy') c.busy++;
    else if (s.attention === 1 || s.attention === 3) c.waiting++;
    else if (s.attention === 2) c.done++;
  }
  return c;
}

/** usage.Find: the first limit whose name contains needle (case-insensitive). */
export function findLimit(usage: Usage | undefined, needle: string): Limit | undefined {
  const n = needle.toLowerCase();
  return usage?.limits?.find((l) => l.name.toLowerCase().includes(n));
}

/** The token that colours a usage percentage (the TUI's usageColor). */
export function usageToken(pct: number): 'done' | 'busy' | 'blocked' {
  if (pct >= 80) return 'blocked';
  if (pct >= 50) return 'busy';
  return 'done';
}

export function money(n: number | undefined): string {
  return n ? `$${n.toFixed(2)}` : '';
}

export interface RowView {
  glyph: string;
  text: string;
  /** Theme token for the glyph. */
  token: 'busy' | 'blocked' | 'done' | 'idle' | 'muted' | 'accent2';
  cost: string;
  picked: boolean;
  /** Small trailing badge: options waiting, subagents running, ctx size. */
  badge: string;
}

function rowToken(r: TreeRow): RowView['token'] {
  if (r.attention === 1 || r.state === 'blocked') return 'blocked';
  if (r.attention === 2) return 'done';
  switch (r.state) {
    case 'busy':
      return 'busy';
    case 'done':
      return 'done';
    case 'idle':
    case 'stopped':
      return 'idle';
  }
  return 'muted';
}

/** One tree row for the left column. */
export function rowView(r: TreeRow, picked: ReadonlySet<string>): RowView {
  if (r.kind === 'ctx') {
    return {
      glyph: r.collapsed ? '▸' : '▾',
      text: r.ctx,
      token: 'accent2',
      cost: money(r.ctx_cost),
      picked: false,
      badge: r.ctx_n ? String(r.ctx_n) : '',
    };
  }
  if (r.kind === 'sub') {
    return {
      glyph: r.state === 'busy' ? '◌' : '·',
      text: r.label ?? r.sub?.description ?? r.sub?.type ?? r.sub?.id ?? '',
      token: r.state === 'busy' ? 'busy' : 'muted',
      cost: money(r.cost),
      picked: false,
      badge: r.sub?.type ?? '',
    };
  }
  const badges: string[] = [];
  if (r.options) badges.push(`${r.options} opções`);
  if (r.subs_running) badges.push(`${r.subs_running} ⑂`);
  return {
    glyph: stateGlyph(r.state, r.attention),
    text: r.label ?? r.short_id ?? r.session_id ?? '',
    token: rowToken(r),
    cost: money(r.cost),
    picked: !!r.session_id && picked.has(r.session_id),
    badge: badges.join(' · '),
  };
}

/** The hints the status bar shows per mode (command → short label). */
const HINTS: Record<string, Array<[string, string]>> = {
  normal: [
    ['session.open', 'abre'],
    ['session.insert', 'insert'],
    ['nav.next_attention', 'próx. esperando'],
    ['palette.open', 'comandos'],
    ['switcher.open', 'chats'],
    ['inbox.summon', 'inbox'],
    ['pane.zoom', 'zoom'],
    ['pane.close', 'fecha'],
    ['help.open', 'ajuda'],
  ],
  insert: [['switcher.open', 'chats']],
};

export interface Hint {
  key: string;
  label: string;
}

/** Key hints for the mode, keys taken from the live keymap (so Lua
 * overrides show); commands with no binding are left out. */
export function hints(keymap: readonly Binding[], mode: 'normal' | 'insert'): Hint[] {
  const out: Hint[] = [];
  if (mode === 'insert') out.push({ key: 'ctrl+]', label: 'normal' });
  for (const [cmd, label] of HINTS[mode]!) {
    const key = keysOf(keymap, cmd)[0];
    if (key) out.push({ key, label });
  }
  return out;
}

/** Tab labels with their counts, as the TUI's tab bar. */
export function tabLabels(inbox: number, archived: number): Array<{ tab: 'sessions' | 'inbox' | 'todos' | 'archive'; label: string }> {
  return [
    { tab: 'sessions', label: '1 Sessões' },
    { tab: 'inbox', label: `2 Inbox (${inbox})` },
    { tab: 'todos', label: '3 Todos' },
    { tab: 'archive', label: `4 Arquivo (${archived})` },
  ];
}
