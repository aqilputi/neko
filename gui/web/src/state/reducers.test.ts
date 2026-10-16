import { describe, expect, it } from 'vitest';
import type { NkEvent } from '../client/types.ts';
import { GW, MIG, hello, snapshot, theme, tree } from '../test/fixtures.ts';
import { MAX_TOASTS, initialState, type NkState } from './model.ts';
import {
  applyCommands,
  applyEvent,
  applyEvents,
  applyHello,
  applyKeymap,
  applyPanes,
  applyResult,
  applyResync,
  applySnapshot,
  applyTheme,
  applyTree,
  dismissToast,
  markReady,
  patchViewState,
  pushToast,
  setError,
  setStatus,
} from './reducers.ts';

function loaded(): NkState {
  let s = initialState();
  s = applySnapshot(s, snapshot());
  s = applyTree(s, tree());
  return s;
}

function ev(seq: number, name: string, data?: unknown): NkEvent {
  return { seq, at: `2026-10-16T20:00:${String(seq).padStart(2, '0')}Z`, name, data };
}

function deepFreeze<T>(o: T): T {
  if (o && typeof o === 'object') {
    Object.freeze(o);
    for (const v of Object.values(o)) deepFreeze(v);
  }
  return o;
}

describe('bootstrap reducers', () => {
  it('store what the server answered, normalising nulls', () => {
    let s = initialState();
    s = applyHello(s, hello);
    s = applyTheme(s, theme);
    s = applyKeymap(s, null);
    s = applyCommands(s, null);
    s = applyPanes(s, null);
    expect(s.hello).toBe(hello);
    expect(s.theme).toBe(theme);
    expect([s.keymap, s.commands, s.panes]).toEqual([[], [], []]);
    s = applyKeymap(s, [{ command: 'nav.down', keys: ['j'], mode: 'normal', help: { key: 'j', desc: 'desce' } }]);
    s = applyCommands(s, [
      { name: 'nav.down', title: '', hint: '', keys: ['j'], mode: 'normal', args: null, scope: 'client', confirm: false, source: 'builtin', available: true },
    ]);
    s = applyPanes(s, [{ id: 'p1', cols: 80, rows: 24 }]);
    expect(s.keymap).toHaveLength(1);
    expect(s.commands).toHaveLength(1);
    expect(s.panes).toHaveLength(1);
  });

  it('snapshot replaces pending and clears needsResync', () => {
    const snap = snapshot();
    snap.pending = [{ token: 't', question: 'q?', expires_at: 'x' }];
    const s = applySnapshot({ ...initialState(), needsResync: true }, snap);
    expect(s.pending).toEqual(snap.pending);
    expect(s.needsResync).toBe(false);
    const s2 = applySnapshot(s, { ...snapshot(), pending: null as never });
    expect(s2.pending).toEqual([]);
  });

  it('tree normalises nulls and keeps the state to send back', () => {
    const s = applyTree(initialState(), { rows: null, state: null as never, inbox: null, hidden: undefined as never, archived: undefined as never });
    expect(s.tree).toEqual({ rows: [], state: {}, inbox: [], hidden: 0, archived: 0 });
    const t = applyTree(initialState(), tree());
    expect(t.tree.state.cursor_key).toBe('ctx:visio');
    expect(t.tree.hidden).toBe(1);
  });

  it('markReady sets ready and clears the error once', () => {
    const s = markReady(setError(initialState(), 'boom'));
    expect(s.ready).toBe(true);
    expect(s.error).toBeNull();
    expect(markReady(s)).toBe(s);
  });

  it('patchViewState merges into the tree state', () => {
    const s = patchViewState(loaded(), { tab: 'inbox', collapsed: { 'ctx:visio': true } });
    expect(s.tree.state).toMatchObject({ tab: 'inbox', sort: 'atividade', collapsed: { 'ctx:visio': true } });
  });

  it('status/error/resync return the same state when unchanged', () => {
    const s = initialState();
    expect(setStatus(s, 'connecting')).toBe(s);
    expect(setStatus(s, 'open').status).toBe('open');
    expect(setError(s, null)).toBe(s);
    const r = applyResync(s);
    expect(r.needsResync).toBe(true);
    expect(applyResync(r)).toBe(r);
  });
});

describe('toasts', () => {
  it('keeps the last MAX_TOASTS and dismisses by id', () => {
    let s = initialState();
    for (let i = 1; i <= MAX_TOASTS + 2; i++) s = pushToast(s, { id: i, text: `t${i}`, err: false, at: '' });
    expect(s.toasts.map((t) => t.id)).toEqual([3, 4, 5, 6, 7]);
    const d = dismissToast(s, 4);
    expect(d.toasts.map((t) => t.id)).toEqual([3, 5, 6, 7]);
    expect(dismissToast(d, 99)).toBe(d);
  });

  it('applyResult adds the toast and the pending confirmation', () => {
    const p = { token: 'tk', question: 'remover?', expires_at: 'x' };
    let s = applyResult(initialState(), { toast: 'falhou', err: true, pending: p }, -1, 'now');
    expect(s.toasts).toEqual([{ id: -1, text: 'falhou', err: true, at: 'now' }]);
    expect(s.pending).toEqual([p]);
    s = applyResult(s, { pending: p }, -2, 'now');
    expect(s.pending).toHaveLength(1);
    expect(applyResult(s, {}, -3, 'now')).toBe(s);
  });
});

describe('applyEvent', () => {
  type Case = {
    name: string;
    from?: () => NkState;
    events: NkEvent[];
    check: (s: NkState) => void;
    resync?: boolean;
  };
  const cases: Case[] = [
    {
      name: 'session.state updates snapshot and tree rows',
      events: [ev(1, 'session.state', { id: GW, prev: 'busy', state: 'blocked' })],
      check: (s) => {
        expect(s.snapshot!.sessions.find((x) => x.session_id === GW)!.state).toBe('blocked');
        expect(s.tree.rows.find((r) => r.key === `sess:${GW}`)!.state).toBe('blocked');
        // other rows untouched
        expect(s.tree.rows.find((r) => r.key === `sess:${MIG}`)!.state).toBe('blocked');
      },
      resync: true,
    },
    {
      name: 'session.state before any snapshot',
      from: initialState,
      events: [ev(1, 'session.state', { id: GW, prev: 'busy', state: 'done' })],
      check: (s) => expect(s.snapshot).toBeNull(),
      resync: true,
    },
    {
      name: 'snapshot.changed asks for a resync',
      events: [ev(3, 'snapshot.changed', { at: 'x', sessions: 7 })],
      check: (s) => expect(s.lastSeq).toBe(3),
      resync: true,
    },
    ...['usage.updated', 'group.changed', 'todos.changed', 'jira.changed', 'dispatched'].map(
      (name): Case => ({ name: `${name} asks for a resync`, events: [ev(1, name, {})], check: () => {}, resync: true }),
    ),
    {
      name: 'toast',
      events: [ev(4, 'toast', { text: 'seção renomeada' }), ev(5, 'toast', { text: 'erro', err: true })],
      check: (s) =>
        expect(s.toasts).toEqual([
          { id: 4, text: 'seção renomeada', err: false, at: '2026-10-16T20:00:04Z' },
          { id: 5, text: 'erro', err: true, at: '2026-10-16T20:00:05Z' },
        ]),
    },
    {
      name: 'notice with and without toast',
      events: [ev(1, 'notice', { meow: 'miau' }), ev(2, 'notice', { meow: 'miau', toast: 'bloqueada', toast_err: true })],
      check: (s) => expect(s.toasts.map((t) => [t.id, t.text, t.err])).toEqual([[2, 'bloqueada', true]]),
    },
    {
      name: 'confirm.requested then resolved',
      events: [
        ev(1, 'confirm.requested', { token: 'a', question: 'q', expires_at: 'x' }),
        ev(2, 'confirm.requested', { token: 'b', question: 'q', expires_at: 'x' }),
        ev(3, 'confirm.requested', { token: 'a', question: 'q', expires_at: 'x' }),
        ev(4, 'confirm.resolved', { token: 'a', outcome: 'yes' }),
      ],
      check: (s) => expect(s.pending.map((p) => p.token)).toEqual(['b']),
    },
    {
      name: 'pane lifecycle',
      events: [
        ev(1, 'pane.opened', { id: 'p1', session_id: GW }),
        ev(2, 'pane.opened', { id: 'p1', session_id: GW }),
        ev(3, 'pane.opened', { id: 'p2' }),
        ev(4, 'pane.resized', { id: 'p1', cols: 120, rows: 40 }),
        ev(5, 'pane.exited', { id: 'p2', err: 'x' }),
      ],
      check: (s) => expect(s.panes).toEqual([{ id: 'p1', session_id: GW, cols: 120, rows: 40 }]),
    },
    {
      name: 'pane.closed',
      events: [ev(1, 'pane.opened', { id: 'p1' }), ev(2, 'pane.closed', { id: 'p1' })],
      check: (s) => expect(s.panes).toEqual([]),
    },
    {
      name: 'mail.cue records the cue on the snapshot',
      events: [ev(7, 'mail.cue', { id: GW, sent: true })],
      check: (s) => expect(s.snapshot!.mail[GW]).toEqual({ at: '2026-10-16T20:00:07Z', sent: true }),
      resync: true,
    },
    {
      name: 'mail.cue without snapshot',
      from: initialState,
      events: [ev(7, 'mail.cue', { id: GW, sent: false })],
      check: (s) => expect(s.snapshot).toBeNull(),
      resync: true,
    },
    {
      name: 'unknown events only advance lastSeq',
      events: [ev(9, 'lua.custom', { x: 1 }), ev(8, 'peer.message', { to: GW, from: 'x', text: 'oi' })],
      check: (s) => {
        expect(s.lastSeq).toBe(9);
        expect(s.toasts).toEqual([]);
      },
    },
  ];

  it.each(cases)('$name', ({ from = loaded, events, check, resync = false }) => {
    const before = deepFreeze(from());
    const s = applyEvents(before, events);
    check(s);
    expect(s.needsResync).toBe(resync);
    expect(s.lastSeq).toBe(Math.max(...events.map((e) => e.seq)));
  });

  it('never mutates its input', () => {
    const before = deepFreeze(loaded());
    expect(() => applyEvent(before, ev(1, 'session.state', { id: GW, prev: 'busy', state: 'done' }))).not.toThrow();
  });
});
