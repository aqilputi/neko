// The ":" palette and the ctrl+k switcher as pure functions: the items
// (the core's commands for the current selection, "ir: sessão", themes,
// the group commands typed with their argument), a fuzzy filter close to
// the TUI's (sahilm/fuzzy: subsequence, bonus for runs and word starts),
// and the typed commands ("group <tag>", "notify #tag msg", "<name> <arg>…",
// "lua <code>") — the same grammar as internal/tui/dialog.go.

import type { CommandInfo, Session } from '../client/types.ts';
import { isSequence } from '../keymap/resolve.ts';

export type PaletteAction =
  | { kind: 'run'; name: string; key?: string }
  | { kind: 'prefill'; text: string }
  | { kind: 'goto'; sessionId: string; open: boolean }
  | { kind: 'theme'; name: string }
  | { kind: 'typed'; name: string; args: Record<string, string> }
  | { kind: 'lua'; code: string }
  | { kind: 'none' };

export interface PaletteItem {
  id: string;
  title: string;
  hint: string;
  action: PaletteAction;
}

// ---- fuzzy ----

/** Lower case without accents ("Migração" → "migracao"). */
export function fold(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

const WORD_SEP = /[\s\-_./:·#()]/;

/** Fuzzy score of q in text (higher is better), or null when q is not a
 * subsequence of text. */
export function fuzzyScore(q: string, text: string): number | null {
  const query = [...fold(q).replace(/\s+/g, '')];
  if (query.length === 0) return 0;
  const t = [...fold(text)];
  let score = 0;
  let ti = 0;
  let prev = -2;
  let first = -1;
  for (const qc of query) {
    while (ti < t.length && t[ti] !== qc) ti++;
    if (ti >= t.length) return null;
    if (first < 0) first = ti;
    score += 1;
    if (ti === prev + 1) score += 5;
    if (ti === 0 || WORD_SEP.test(t[ti - 1]!)) score += 8;
    prev = ti;
    ti++;
  }
  return score - first * 0.1 - t.length * 0.01;
}

/** Items matching q, best first (stable for ties); all of them for "". */
export function fuzzyFilter<T extends { title: string; hint: string }>(items: readonly T[], q: string): T[] {
  if (!q.trim()) return [...items];
  const scored: Array<[T, number, number]> = [];
  items.forEach((it, i) => {
    const s = fuzzyScore(q, `${it.title} ${it.hint}`);
    if (s !== null) scored.push([it, s, i]);
  });
  scored.sort((a, b) => b[1] - a[1] || a[2] - b[2]);
  return scored.map((x) => x[0]);
}

// ---- items ----

export interface PaletteInput {
  /** GET /v1/commands for the current selection. */
  commands: readonly CommandInfo[];
  sessions: readonly Session[];
  showAll: boolean;
  themes: readonly string[];
  activeTheme: string;
  /** Sessions picked with space (0: the cursor's). */
  pickedN: number;
  /** Group tag → member count. */
  groups: Readonly<Record<string, number>>;
}

/** Commands the palette leaves out: itself, the group commands (offered
 * typed), sequences (their keys are quicker) and what is unavailable. */
export function listedCommand(c: CommandInfo): boolean {
  if (c.name === 'palette.open' || c.name.startsWith('group.')) return false;
  if (c.available === false) return false;
  return !(c.keys ?? []).some(isSequence);
}

export function hasRequired(c: Pick<CommandInfo, 'args'>): boolean {
  return (c.args ?? []).some((a) => a.required);
}

/** "<required> [optional]…". */
export function argsUsage(c: Pick<CommandInfo, 'args'>): string {
  return (c.args ?? []).map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`)).join(' ');
}

export function paletteItems(inp: PaletteInput): PaletteItem[] {
  const items: PaletteItem[] = [];
  for (const c of inp.commands) {
    if (!listedCommand(c)) continue;
    const key = c.keys?.[0];
    const action: PaletteAction =
      c.scope === 'core' && hasRequired(c) ? { kind: 'prefill', text: `${c.name} ` } : { kind: 'run', name: c.name, ...(key ? { key } : {}) };
    items.push({ id: `cmd:${c.name}`, title: c.title || c.name, hint: key ?? c.hint ?? '', action });
  }
  const what = inp.pickedN > 0 ? `as ${inp.pickedN} selecionadas` : 'a sessão do cursor';
  items.push(
    { id: 'typed:group', title: `group <tag> — agrupa ${what}`, hint: 'space seleciona', action: { kind: 'prefill', text: 'group ' } },
    { id: 'typed:ungroup', title: `ungroup [tag] — tira ${what} do grupo`, hint: 'grupos', action: { kind: 'prefill', text: 'ungroup ' } },
    { id: 'typed:notify', title: 'notify #tag <mensagem> — avisa o grupo', hint: 'grupos', action: { kind: 'prefill', text: 'notify #' } },
  );
  for (const [tag, n] of Object.entries(inp.groups).sort(([a], [b]) => a.localeCompare(b))) {
    items.push({
      id: `dissolve:${tag}`,
      title: `desfazer grupo #${tag} (${n} sessões)`,
      hint: 'grupos',
      action: { kind: 'typed', name: 'group.dissolve', args: { tag } },
    });
  }
  for (const s of inp.sessions) {
    if (s.stale && !inp.showAll) continue;
    items.push({
      id: `goto:${s.session_id}`,
      title: `ir: ${s.label}`,
      hint: [s.context, s.cwd].filter(Boolean).join(' · '),
      action: { kind: 'goto', sessionId: s.session_id, open: false },
    });
  }
  for (const t of inp.themes) {
    items.push({ id: `theme:${t}`, title: `tema: ${t}`, hint: t === inp.activeTheme ? 'atual' : '', action: { kind: 'theme', name: t } });
  }
  return items;
}

/** core.CleanTag: lower case, no "#", spaces → "-". */
export function cleanTag(s: string): string {
  return s.trim().replace(/^#+/, '').trim().toLowerCase().replace(/\s+/g, '-');
}

export interface TypedInput {
  commands: readonly CommandInfo[];
  pickedN: number;
  /** Groups of the session under the cursor (notify's default tag). */
  cursorTags: readonly string[];
  lua: boolean;
}

/** What was typed, as a command with arguments; null falls back to fuzzy. */
export function typedItem(q: string, inp: TypedInput): PaletteItem | null {
  const f = q.trim().split(/\s+/).filter(Boolean);
  const head = f[0];
  if (!head) return null;
  const what = inp.pickedN > 0 ? `${inp.pickedN} sessões` : 'a sessão do cursor';
  const rest = q.trim().slice(head.length).trim();
  switch (head) {
    case 'lua':
      if (!inp.lua) return null;
      return rest
        ? { id: 'typed', title: `▶ lua ${rest}`, hint: 'avalia no core', action: { kind: 'lua', code: rest } }
        : { id: 'typed', title: 'lua <código>', hint: 'avalia no core', action: { kind: 'none' } };
    case 'group':
    case 'g': {
      if (!rest) return { id: 'typed', title: 'group <tag>', hint: `agrupa ${what}`, action: { kind: 'none' } };
      const tag = cleanTag(rest);
      return { id: 'typed', title: `▶ group #${tag}`, hint: what, action: { kind: 'typed', name: 'group.add', args: { tag } } };
    }
    case 'ungroup':
    case 'ug': {
      const tag = f[1] ? cleanTag(f[1]) : '';
      const title = tag ? `▶ ungroup #${tag} (${what})` : `▶ ungroup ${what}`;
      return { id: 'typed', title, hint: 'tira do grupo', action: { kind: 'typed', name: 'group.remove', args: tag ? { tag } : {} } };
    }
    case 'notify':
    case 'n': {
      let tag = '';
      let text = rest;
      if (f[1]?.startsWith('#')) {
        tag = cleanTag(f[1]);
        text = rest.slice(f[1].length).trim();
      }
      if (!tag) tag = inp.cursorTags[0] ?? '';
      if (!tag || !text) {
        return { id: 'typed', title: 'notify #tag <mensagem>', hint: tag ? `#${tag}` : 'sem grupo', action: { kind: 'none' } };
      }
      return { id: 'typed', title: `▶ notify #${tag} ${text}`, hint: 'avisa o grupo', action: { kind: 'typed', name: 'group.notify', args: { tag, text } } };
    }
  }
  return argCommand(q, inp.commands);
}

/** "<name> <arg>…" for a core command with arguments: the words fill its
 * arguments in order, the last one taking the rest. */
export function argCommand(q: string, commands: readonly CommandInfo[]): PaletteItem | null {
  const trimmed = q.trim();
  const name = trimmed.split(/\s+/)[0];
  const c = commands.find((x) => x.name === name);
  if (!c || !c.args?.length || c.scope !== 'core') return null;
  let rest = trimmed.slice(c.name.length).trim();
  const args: Record<string, string> = {};
  const shown: string[] = [];
  for (let i = 0; i < c.args.length; i++) {
    const a = c.args[i]!;
    let v = rest;
    if (i < c.args.length - 1) {
      const sp = rest.indexOf(' ');
      v = sp < 0 ? rest : rest.slice(0, sp);
      rest = sp < 0 ? '' : rest.slice(sp + 1).trim();
    }
    if (!v) {
      if (a.required) return { id: 'typed', title: `${c.name} ${argsUsage(c)}`, hint: c.title, action: { kind: 'none' } };
      continue;
    }
    args[a.name] = v;
    shown.push(v);
  }
  return { id: 'typed', title: `▶ ${c.name} ${shown.join(' ')}`.trimEnd(), hint: c.title, action: { kind: 'typed', name: c.name, args } };
}

/** The palette's list for what was typed: the typed command on top, then
 * the fuzzy matches. */
export function filterPalette(items: readonly PaletteItem[], q: string, typed: PaletteItem | null): PaletteItem[] {
  const out = fuzzyFilter(items, q);
  return typed ? [typed, ...out] : out;
}

// ---- switcher (ctrl+k) ----

/** Chats running or waiting on you, waiting ones first, then by activity. */
export function switcherItems(sessions: readonly Session[]): PaletteItem[] {
  const live = sessions.filter(
    (s) => !s.ended && !s.archived && (s.state === 'busy' || s.state === 'blocked' || s.attention !== 0),
  );
  const ordered = live
    .map((s, i) => ({ s, i }))
    .sort((a, b) => {
      const wa = a.s.attention !== 0;
      const wb = b.s.attention !== 0;
      if (wa !== wb) return wa ? -1 : 1;
      const ta = a.s.last_at ? Date.parse(a.s.last_at) : 0;
      const tb = b.s.last_at ? Date.parse(b.s.last_at) : 0;
      return tb - ta || a.i - b.i;
    })
    .map((x) => x.s);
  return ordered.map((s) => ({
    id: `sw:${s.session_id}`,
    title: `${stateGlyph(s.state, s.attention)} ${s.label}`,
    hint: [stateLabel(s.state, s.attention), s.context].filter(Boolean).join(' · '),
    action: { kind: 'goto', sessionId: s.session_id, open: true },
  }));
}

/** The TUI's state icons. */
export function stateGlyph(state: string | undefined, attention = 0): string {
  if (attention === 1 || state === 'blocked') return '◆';
  if (attention === 2) return '✓';
  switch (state) {
    case 'busy':
      return '●';
    case 'done':
      return '✓';
    case 'idle':
      return '○';
    default:
      return '·';
  }
}

export function stateLabel(state: string | undefined, attention = 0): string {
  switch (attention) {
    case 1:
      return 'te esperando';
    case 2:
      return 'voltou';
    case 3:
      return 'ociosa';
  }
  switch (state) {
    case 'busy':
      return 'rodando';
    case 'blocked':
      return 'te esperando';
    case 'done':
      return 'pronta';
    case 'idle':
      return 'ociosa';
    default:
      return state ?? '';
  }
}
