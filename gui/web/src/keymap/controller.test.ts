import { beforeEach, describe, expect, it } from 'vitest';
import { RpcError } from '../client/errors.ts';
import type { Context, Result, ViewState } from '../client/types.ts';
import { applyCommands, applyKeymap, applySnapshot, applyTheme, applyTree, patchViewState } from '../state/reducers.ts';
import { createStore, type Store } from '../state/store.ts';
import { GW, MIG, snapshot, theme, tree } from '../test/fixtures.ts';
import { KeyController, NUDGE_PX, type KeyDeps, type WindowsApi } from './controller.ts';
import { DEMO_COMMANDS, DEMO_KEYMAP } from './demo-fixture.ts';
import { createUi, type Ui } from './ui.ts';

class FakeWindows implements WindowsApi {
  calls: unknown[][] = [];
  active = true;
  open(id: string, focus: boolean) {
    this.calls.push(['open', id, focus]);
  }
  closeActive() {
    this.calls.push(['close']);
    return this.active;
  }
  zoomActive() {
    this.calls.push(['zoom']);
    return this.active;
  }
  focusActive() {
    this.calls.push(['focus']);
    return this.active;
  }
  nudgeActive(dx: number, dy: number) {
    this.calls.push(['nudge', dx, dy]);
    return this.active;
  }
  blur() {
    this.calls.push(['blur']);
  }
}

interface Rig {
  store: Store;
  ui: Ui;
  win: FakeWindows;
  kc: KeyController;
  deps: KeyDeps;
  runs: Array<[string, Record<string, string> | undefined]>;
  clientRuns: Array<[string, Record<string, string> | undefined, Context]>;
  confirms: Array<[string, boolean]>;
  refreshes: ViewState[];
  clock: { t: number };
  fail: { next: Error | null };
}

function rig(): Rig {
  const store = createStore();
  store.update(applyKeymap, DEMO_KEYMAP);
  store.update(applyCommands, DEMO_COMMANDS);
  store.update(applySnapshot, snapshot());
  store.update(applyTree, tree());
  store.update(applyTheme, theme);
  const ui = createUi();
  const win = new FakeWindows();
  const r: Partial<Rig> = { store, ui, win, runs: [], clientRuns: [], confirms: [], refreshes: [], clock: { t: 1000 }, fail: { next: null } };
  const rr = r as Rig;
  const sync = {
    async run(name: string, args?: Record<string, string>): Promise<Result> {
      if (rr.fail.next) {
        const e = rr.fail.next;
        rr.fail.next = null;
        throw e;
      }
      rr.runs.push([name, args]);
      return {};
    },
    async confirm(token: string, yes: boolean): Promise<Result> {
      rr.confirms.push([token, yes]);
      return {};
    },
    async refreshTree(patch: ViewState = {}) {
      rr.refreshes.push(patch);
      store.update(patchViewState, patch);
    },
  };
  const client = {
    async run(name: string, args: Record<string, string> | undefined, ctx: Context = {}): Promise<Result> {
      rr.clientRuns.push([name, args, ctx]);
      return { toast: 'feito' };
    },
    async theme(name?: string) {
      if (name === 'boom') throw new Error('sem tema');
      return { ...theme, name: name ?? theme.name };
    },
    async luaEval(code: string) {
      return code === 'bad' ? { error: 'erro lua' } : { result: code === 'nil' ? '' : '42' };
    },
  };
  rr.deps = { store, sync, client, ui, windows: win, now: () => rr.clock.t, pageRows: () => 4 };
  rr.kc = new KeyController(rr.deps);
  return rr;
}

const k = (key: string, mods: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean }> = {}) => ({ key, ...mods });
const flush = () => new Promise((r) => setTimeout(r, 0));

let r: Rig;
beforeEach(() => {
  r = rig();
});

const cursorKey = () => r.store.state.value.tree.state.cursor_key;

describe('navigation', () => {
  it.each([
    [[k('j')], `sess:${GW}`],
    [[k('j'), k('j')], `sess:${MIG}`],
    [[k('ArrowDown'), k('k')], 'ctx:acme'],
    [[k('G', { shiftKey: true })], 'sub:agent-idx-a'],
    [[k('G', { shiftKey: true }), k('g'), k('g')], 'ctx:acme'],
    [[k('d', { ctrlKey: true })], `sess:${MIG}`], // half of 4
    [[k('f', { ctrlKey: true })], 'sub:agent-idx-a'],
  ])('%j → %s', (keys, want) => {
    for (const e of keys) expect(r.kc.handleKey(e)).toBe(true);
    expect(cursorKey()).toBe(want);
  });

  it('g waits, then a late g starts over', () => {
    r.kc.handleKey(k('j'));
    r.kc.handleKey(k('g'));
    r.clock.t += 5000;
    r.kc.handleKey(k('g'));
    expect(cursorKey()).toBe(`sess:${GW}`);
    expect(r.ui.seq.value.pending).toBe('g');
  });

  it('unbound keys are not consumed', () => {
    expect(r.kc.handleKey(k('y'))).toBe(false);
    expect(r.kc.handleKey(k('Shift'))).toBe(false);
  });

  it('folds and tabs rebuild the tree', async () => {
    r.kc.handleKey(k('h'));
    r.kc.handleKey(k('2'));
    r.kc.handleKey(k('Tab'));
    r.kc.handleKey(k('H', { shiftKey: true }));
    r.kc.handleKey(k('S', { shiftKey: true }));
    for (let i = 0; i < 5; i++) await flush();
    expect(r.refreshes).toEqual([
      { collapsed: { 'ctx:acme': true }, cursor_key: 'ctx:acme' },
      { tab: 'inbox' },
      { tab: 'todos' },
      { show_all: true },
      { sort: 'custo', reset_order: true },
    ]);
  });

  it('rebuilds run one after the other', async () => {
    const order: string[] = [];
    let release!: () => void;
    const gate = new Promise<void>((res) => (release = res));
    r.kc = new KeyController({
      ...r.deps,
      sync: {
        ...r.deps.sync,
        async refreshTree(p: ViewState = {}) {
          order.push(`start ${p.filter}`);
          if (p.filter === 'a') await gate;
          order.push(`end ${p.filter}`);
        },
      },
    });
    const a = r.kc.refresh({ filter: 'a' });
    const b = r.kc.refresh({ filter: 'ab' });
    await flush();
    expect(order).toEqual(['start a']);
    release();
    await Promise.all([a, b]);
    expect(order).toEqual(['start a', 'end a', 'start ab', 'end ab']);
  });

  it('l unfolds', async () => {
    r.store.update(patchViewState, { collapsed: { 'ctx:acme': true } });
    r.kc.handleKey(k('l'));
    await flush();
    await flush();
    expect(r.refreshes).toEqual([{ collapsed: {} }]);
  });

  it('a jumps to the next session waiting on you', async () => {
    r.kc.handleKey(k('a'));
    await flush();
    expect(cursorKey()).toBe(`sess:${MIG}`);
    r.kc.handleKey(k('A', { shiftKey: true }));
    await flush();
    expect(cursorKey()).toBe(`sess:${MIG}`);
  });

  it('the todos tab resolves in normal.todos', async () => {
    await r.kc.execute('tab.todos');
    expect(r.kc.mode()).toBe('normal.todos');
    r.kc.handleKey(k('Enter'));
    await flush();
    expect(r.runs).toEqual([['todo.dispatch', {}]]); // optional args only: runs
  });

  it('space picks; esc clears; picks go in the context', async () => {
    r.kc.handleKey(k('j'));
    r.kc.handleKey(k(' '));
    expect([...r.ui.picked.value]).toEqual([GW]);
    expect(r.kc.context()).toEqual({ session_id: GW, picked: [GW], tab: 'sessions' });
    r.kc.handleKey(k('R', { shiftKey: true }));
    await flush();
    expect(r.clientRuns).toEqual([['sessions.refresh', {}, { session_id: GW, picked: [GW], tab: 'sessions' }]]);
    expect(r.store.state.value.toasts.at(-1)?.text).toBe('feito');
    expect(r.kc.handleKey(k('Escape'))).toBe(true);
    expect(r.ui.picked.value.size).toBe(0);
    expect(r.kc.handleKey(k('Escape'))).toBe(false);
  });
});

describe('overlays and client commands', () => {
  it.each([
    [k(':'), { kind: 'palette', query: '' }],
    [k('p', { ctrlKey: true }), { kind: 'palette', query: '' }],
    [k('k', { ctrlKey: true }), { kind: 'switcher' }],
    [k('?'), { kind: 'help' }],
    [k('u'), { kind: 'usage' }],
    [k('/'), { kind: 'filter', value: '' }],
  ])('%j opens %j', (e, want) => {
    expect(r.kc.handleKey(e)).toBe(true);
    expect(r.ui.overlay.value).toEqual(want);
    // with an overlay open the controller stays out of the way
    expect(r.kc.handleKey(k('j'))).toBe(false);
  });

  it('T cycles the theme', async () => {
    r.kc.handleKey(k('T', { shiftKey: true }));
    await flush();
    expect(r.store.state.value.theme?.name).toBe('dracula');
  });

  it('q only explains', () => {
    r.kc.handleKey(k('q'));
    expect(r.store.state.value.toasts.at(-1)?.text).toMatch(/feche a janela/);
  });

  it('required or prompted args open the palette typed', async () => {
    r.kc.handleKey(k('j'));
    r.kc.handleKey(k('n'));
    expect(r.ui.overlay.value).toEqual({ kind: 'palette', query: 'session.rename ' });
    r.ui.overlay.value = null;
    r.kc.handleKey(k('r'));
    expect(r.ui.overlay.value).toEqual({ kind: 'palette', query: 'session.reply ' });
    expect(r.runs).toEqual([]);
  });

  it('extra client commands come from other modules', async () => {
    let flat = 0;
    r.kc = new KeyController({ ...r.deps, extra: { 'view.flat': () => flat++ } });
    await r.kc.execute('view.flat');
    expect(flat).toBe(1);
  });

  it('moveTo puts the cursor on a row', () => {
    r.kc.moveTo(2);
    expect(cursorKey()).toBe(`sess:${MIG}`);
    r.kc.moveTo(99);
    expect(cursorKey()).toBe(`sess:${MIG}`);
  });

  it('a client command the GUI does not know says so', async () => {
    r.store.update(applyCommands, [...DEMO_COMMANDS, { ...DEMO_COMMANDS[0]!, name: 'lua.client', scope: 'client' }]);
    await r.kc.execute('lua.client');
    expect(r.store.state.value.toasts.at(-1)).toMatchObject({ err: true });
  });
});

describe('windows', () => {
  it('enter on a session opens and focuses; o previews; O zooms', async () => {
    r.kc.handleKey(k('j'));
    r.kc.handleKey(k('Enter'));
    r.kc.handleKey(k('o'));
    r.kc.handleKey(k('O', { shiftKey: true }));
    await flush();
    expect(r.win.calls).toEqual([
      ['open', GW, true],
      ['open', GW, false],
      ['open', GW, true],
      ['zoom'],
    ]);
  });

  it('enter on a context toggles its fold', async () => {
    r.kc.handleKey(k('Enter'));
    await flush();
    await flush();
    expect(r.refreshes).toEqual([{ collapsed: { 'ctx:acme': true } }]);
    expect(r.win.calls).toEqual([]);
  });

  it('w, z, i, alt+arrows', async () => {
    r.kc.handleKey(k('w'));
    r.kc.handleKey(k('z'));
    r.kc.handleKey(k('i'));
    r.kc.handleKey(k('ArrowLeft', { altKey: true }));
    r.kc.handleKey(k('ArrowDown', { altKey: true, shiftKey: true }));
    await flush();
    expect(r.win.calls).toEqual([['close'], ['zoom'], ['focus'], ['nudge', -NUDGE_PX, 0], ['nudge', 0, NUDGE_PX * 4]]);
  });

  it('i with no window opens the cursor session focused', async () => {
    r.win.active = false;
    r.kc.handleKey(k('j'));
    r.kc.handleKey(k('i'));
    await flush();
    expect(r.win.calls).toEqual([['focus'], ['open', GW, true]]);
  });

  it('INSERT: ctrl+] leaves, ctrl+k opens the switcher, the rest goes to the PTY', () => {
    r.ui.mode.value = 'insert';
    expect(r.kc.mode()).toBe('insert');
    expect(r.kc.handleInsertKey(k('j'))).toBe(false);
    expect(r.kc.handleInsertKey(k('Shift'))).toBe(false);
    expect(r.kc.handleInsertKey(k('k', { ctrlKey: true }))).toBe(true);
    expect(r.ui.overlay.value).toEqual({ kind: 'switcher' });
    expect(r.ui.mode.value).toBe('normal');
    r.ui.mode.value = 'insert';
    expect(r.kc.handleInsertKey(k(']', { ctrlKey: true }))).toBe(true);
    expect(r.ui.mode.value).toBe('normal');
    expect(r.win.calls.filter((c) => c[0] === 'blur')).toHaveLength(2);
  });
});

describe('core commands and confirmations', () => {
  it('x runs session.stop with the cursor session', async () => {
    r.kc.handleKey(k('j'));
    r.kc.handleKey(k('x'));
    await flush();
    expect(r.runs).toEqual([['session.stop', {}]]);
  });

  it('dd runs session.remove', async () => {
    r.kc.handleKey(k('j'));
    r.kc.handleKey(k('d'));
    expect(r.runs).toEqual([]);
    r.kc.handleKey(k('d'));
    await flush();
    expect(r.runs).toEqual([['session.remove', {}]]);
  });

  it('a pending confirmation is modal: y/enter yes, n/esc no', async () => {
    const pend = (token: string) =>
      r.store.update((s) => ({ ...s, pending: [{ token, question: 'parar?', expires_at: '' }] }));
    pend('t1');
    expect(r.kc.handleKey(k('j'))).toBe(true); // swallowed
    expect(cursorKey()).toBe('ctx:acme');
    r.kc.handleKey(k('n'));
    r.kc.handleKey(k('Escape'));
    r.kc.handleKey(k('y'));
    r.kc.handleKey(k('Enter'));
    r.ui.mode.value = 'insert';
    expect(r.kc.handleInsertKey(k('Y', { shiftKey: true }))).toBe(true);
    await flush();
    expect(r.confirms).toEqual([
      ['t1', false],
      ['t1', false],
      ['t1', true],
      ['t1', true],
      ['t1', true],
    ]);
  });

  it('errors become error toasts', async () => {
    r.fail.next = new RpcError(409, 'unavailable', 'sem sessão');
    await r.kc.runCore('session.stop', {});
    expect(r.store.state.value.toasts.at(-1)).toMatchObject({ text: 'sem sessão', err: true });
  });

  it('a failed confirm is a toast', async () => {
    r.kc = new KeyController({
      store: r.store,
      ui: r.ui,
      windows: r.win,
      sync: {
        run: async () => ({}),
        refreshTree: async () => {},
        confirm: async () => {
          throw new Error('expirou');
        },
      },
      client: { run: async () => ({}), theme: async () => theme, luaEval: async () => ({}) },
    });
    await r.kc.confirm('t', true);
    expect(r.store.state.value.toasts.at(-1)).toMatchObject({ text: 'expirou', err: true });
  });

  it('skip leaves a command to someone else', () => {
    r.kc = new KeyController({ ...r.deps, skip: new Set(['inbox.summon']) });
    expect(r.kc.handleKey(k(' ', { shiftKey: true }))).toBe(false);
  });

  it('group.add clears the picks', async () => {
    r.ui.picked.value = new Set([GW]);
    await r.kc.runCore('group.add', { tag: 'x' });
    expect(r.ui.picked.value.size).toBe(0);
  });
});

describe('palette actions', () => {
  it.each([
    [{ kind: 'none' } as const, null],
    [{ kind: 'prefill', text: 'group ' } as const, { kind: 'palette', query: 'group ' }],
  ])('%j keeps the palette', async (a, want) => {
    r.ui.overlay.value = { kind: 'palette', query: 'x' };
    await r.kc.runAction(a);
    expect(r.ui.overlay.value).toEqual(want ?? { kind: 'palette', query: 'x' });
  });

  it('run executes and closes', async () => {
    r.ui.overlay.value = { kind: 'palette', query: '' };
    await r.kc.runAction({ kind: 'run', name: 'help.open' });
    expect(r.ui.overlay.value).toEqual({ kind: 'help' });
  });

  it('typed runs a core command with args', async () => {
    await r.kc.runAction({ kind: 'typed', name: 'group.add', args: { tag: 'x' } });
    expect(r.runs).toEqual([['group.add', { tag: 'x' }]]);
  });

  it('theme loads it; a failing theme toasts', async () => {
    await r.kc.runAction({ kind: 'theme', name: 'gruvbox' });
    expect(r.store.state.value.theme?.name).toBe('gruvbox');
    await r.kc.runAction({ kind: 'theme', name: 'boom' });
    expect(r.store.state.value.toasts.at(-1)).toMatchObject({ text: 'sem tema', err: true });
  });

  it.each([
    ['return 1', '42', false],
    ['nil', 'ok', false],
    ['bad', 'erro lua', true],
  ])('lua %s → %s', async (code, text, err) => {
    await r.kc.runAction({ kind: 'lua', code });
    expect(r.store.state.value.toasts.at(-1)).toMatchObject({ text, err });
  });

  it('goto moves the cursor; open also opens the window', async () => {
    await r.kc.runAction({ kind: 'goto', sessionId: MIG, open: false });
    expect(cursorKey()).toBe(`sess:${MIG}`);
    await r.kc.runAction({ kind: 'goto', sessionId: GW, open: true });
    expect(cursorKey()).toBe(`sess:${GW}`);
    expect(r.win.calls).toEqual([['open', GW, true]]);
  });

  it('goto from another tab goes back to sessions first', async () => {
    r.store.update(patchViewState, { tab: 'inbox' });
    r.store.update((s) => ({ ...s, tree: { ...s.tree, rows: [] } }));
    await r.kc.runAction({ kind: 'goto', sessionId: MIG, open: false });
    await flush();
    expect(r.refreshes).toEqual([{ tab: 'sessions' }]);
  });
});
