import { describe, expect, it } from 'vitest';
import type { Session, TreeRow } from '../client/types.ts';
import { DEMO_KEYMAP } from '../keymap/demo-fixture.ts';
import { tree } from '../test/fixtures.ts';
import { counts, findLimit, hints, money, rowView, tabLabels, usageToken } from './format.ts';

describe('counts', () => {
  const s = (state: string, attention: number): Session => ({ session_id: 'x', label: '', cwd: '', kind: '', state, attention, cost: 0 });
  it('as the TUI header', () => {
    expect(counts([s('busy', 0), s('busy', 1), s('blocked', 1), s('idle', 3), s('done', 2), s('idle', 0)])).toEqual({
      busy: 2,
      waiting: 2,
      done: 1,
    });
  });
});

describe('usage', () => {
  const u = { limits: [{ name: 'Current session', pct: 60, resets: '5pm' }, { name: 'Current week (all models)', pct: 34 }] };
  it('findLimit', () => {
    expect(findLimit(u, 'session')?.pct).toBe(60);
    expect(findLimit(u, 'ALL MODELS')?.pct).toBe(34);
    expect(findLimit(u, 'opus')).toBeUndefined();
    expect(findLimit(undefined, 'x')).toBeUndefined();
  });
  it.each([
    [10, 'done'],
    [50, 'busy'],
    [79, 'busy'],
    [80, 'blocked'],
  ] as const)('usageToken(%d) = %s', (p, t) => expect(usageToken(p)).toBe(t));
  it('money', () => {
    expect(money(2.1)).toBe('$2.10');
    expect(money(0)).toBe('');
    expect(money(undefined)).toBe('');
  });
});

describe('rowView', () => {
  const rows = tree().rows!;
  it('context', () => {
    expect(rowView(rows[0]!, new Set())).toEqual({ glyph: '▾', text: 'visio', token: 'accent2', cost: '$5.50', picked: false, badge: '2' });
    expect(rowView({ ...rows[0]!, collapsed: true, ctx_n: 0 }, new Set())).toMatchObject({ glyph: '▸', badge: '' });
  });
  it('sessions', () => {
    expect(rowView(rows[1]!, new Set([rows[1]!.session_id!]))).toMatchObject({ glyph: '●', text: 'api-gateway', token: 'busy', picked: true });
    expect(rowView(rows[2]!, new Set())).toMatchObject({ glyph: '◆', token: 'blocked', badge: '3 opções' });
  });
  it.each([
    [{ state: 'done', attention: 2 }, 'done'],
    [{ state: 'done', attention: 0 }, 'done'],
    [{ state: 'idle', attention: 0 }, 'idle'],
    [{ state: 'stopped', attention: 0 }, 'idle'],
    [{ state: 'history', attention: 0 }, 'muted'],
  ])('token of %j → %s', (x, t) => {
    const r: TreeRow = { key: 's', kind: 'sess', ctx: 'c', depth: 1, color: 0, ...x };
    expect(rowView(r, new Set()).token).toBe(t);
  });
  it('session fallbacks and subagent badges', () => {
    const r: TreeRow = { key: 's', kind: 'sess', ctx: 'c', depth: 1, color: 0, attention: 0, short_id: 'abc', subs_running: 2 };
    expect(rowView(r, new Set())).toMatchObject({ text: 'abc', badge: '2 ⑂', picked: false });
    expect(rowView({ ...r, short_id: undefined, session_id: 'sid' }, new Set()).text).toBe('sid');
    expect(rowView({ ...r, short_id: undefined }, new Set()).text).toBe('');
  });
  it('subagents', () => {
    expect(rowView(rows[3]!, new Set())).toMatchObject({ glyph: '◌', text: 'mapear os módulos', token: 'busy' });
    const done: TreeRow = { key: 'x', kind: 'sub', ctx: 'c', depth: 2, color: 0, attention: 0, state: 'done', sub: { id: 'a1', type: 'Explore' } };
    expect(rowView(done, new Set())).toMatchObject({ glyph: '·', text: 'Explore', token: 'muted', badge: 'Explore' });
    expect(rowView({ ...done, sub: { id: 'a1' } }, new Set()).text).toBe('a1');
    expect(rowView({ ...done, sub: undefined }, new Set()).text).toBe('');
  });
});

describe('hints', () => {
  it('normal from the keymap', () => {
    const h = hints(DEMO_KEYMAP, 'normal');
    expect(h[0]).toEqual({ key: 'enter', label: 'abre' });
    expect(h.find((x) => x.label === 'comandos')?.key).toBe(':');
    expect(h.find((x) => x.label === 'ajuda')?.key).toBe('ctrl+/');
  });
  it('insert starts with ctrl+]', () => {
    expect(hints(DEMO_KEYMAP, 'insert')).toEqual([
      { key: 'ctrl+]', label: 'normal' },
      { key: 'ctrl+k', label: 'chats' },
    ]);
  });
  it('unbound commands are left out', () => expect(hints([], 'normal')).toEqual([]));
  it('tab labels', () => expect(tabLabels(3, 1).map((t) => t.label)).toEqual(['1 Sessões', '2 Inbox (3)', '3 Todos', '4 Arquivo (1)']));
});
