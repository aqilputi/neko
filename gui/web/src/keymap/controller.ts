// The keyboard-first core of the GUI (plan §4.5): a KeyboardEvent becomes
// a key string, the key a command through GET /v1/keymap (keymap.ts
// rules), and the command runs here when it is a client one (nav.*, tab.*,
// view.*, palette/switcher/help, panes) or in the core with this client's
// Context (cursor row, picked, tab). The palette and the switcher run
// their items through the same controller.

import type { NkClient } from '../client/client.ts';
import { RpcError } from '../client/errors.ts';
import type { CommandInfo, Context, Result, TabName, ViewState } from '../client/types.ts';
import type { PaletteAction } from '../chrome/palette.ts';
import { hasRequired } from '../chrome/palette.ts';
import { applyResult, applyTheme, patchViewState, pushToast } from '../state/reducers.ts';
import type { Store } from '../state/store.ts';
import type { Sync } from '../state/sync.ts';
import { isLeaveInsert, keyString, type KeyLike } from './keys.ts';
import * as nav from './nav.ts';
import { IDLE, MODE_INSERT, feedKey, normalMode, resolve } from './resolve.ts';
import type { Ui } from './ui.ts';

/** What the controller needs from the window manager. */
export interface WindowsApi {
  open(sessionId: string, focus: boolean): void;
  closeActive(): boolean;
  zoomActive(): boolean;
  /** Focuses the active window's terminal (INSERT); false when none. */
  focusActive(): boolean;
  nudgeActive(dx: number, dy: number): boolean;
  /** Leaves INSERT: blurs the focused terminal. */
  blur(): void;
}

export interface KeyDeps {
  store: Store;
  sync: Pick<Sync, 'run' | 'confirm' | 'refreshTree'>;
  client: Pick<NkClient, 'run' | 'theme' | 'luaEval'>;
  ui: Ui;
  windows: WindowsApi;
  now?: () => number;
  /** Commands another module already handles by key (inbox.summon when
   * the inbox cat installed its own shift+space). */
  skip?: ReadonlySet<string>;
  /** Rows of a "page" for ctrl+f/ctrl+b (half for ctrl+d/ctrl+u). */
  pageRows?: () => number;
  /** Client commands other modules implement (view.flat → the scene). */
  extra?: Readonly<Record<string, () => void>>;
}

/** Core commands the TUI asks arguments for before running (their args are
 * optional, but running them bare would clear the value). */
const PROMPTED = new Set(['session.rename', 'session.move']);

/** Pixels a window moves per alt+arrow (shift: four times as far). */
export const NUDGE_PX = 24;

let toastIds = -1_000_000; // below sync's local ids, never a bus seq

export class KeyController {
  private readonly d: KeyDeps;
  private readonly now: () => number;

  constructor(deps: KeyDeps) {
    this.d = deps;
    this.now = deps.now ?? (() => Date.now());
  }

  private get state() {
    return this.d.store.state.value;
  }

  /** The keymap mode right now. */
  mode(): string {
    return this.d.ui.mode.value === 'insert' ? MODE_INSERT : normalMode(this.state.tree.state.tab);
  }

  /** The selection the core sees (proto.Context). */
  context(): Context {
    const s = this.state;
    const row = this.d.store.cursorRow.value;
    const ctx: Context = {};
    if (row?.session_id) ctx.session_id = row.session_id;
    const picked = [...this.d.ui.picked.value];
    if (picked.length) ctx.picked = picked;
    if (s.tree.state.tab) ctx.tab = s.tree.state.tab;
    return ctx;
  }

  toast(text: string, err = false): void {
    this.d.store.update(pushToast, { id: toastIds--, text, err, at: new Date(this.now()).toISOString() });
  }

  /**
   * A key pressed outside any terminal or text field. True when it was
   * used (the caller then calls preventDefault).
   */
  handleKey(ev: KeyLike): boolean {
    const ui = this.d.ui;
    const key = keyString(ev);
    if (key === null) return false;
    if (this.state.pending.length) return this.confirmKey(key);
    if (ui.overlay.value) return false; // the overlay's own input handles it
    if (key.startsWith('alt+') && /(up|down|left|right)$/.test(key)) return this.nudge(key);
    if (key === 'esc') {
      const had = ui.seq.value.pending !== null || ui.picked.value.size > 0;
      ui.seq.value = IDLE;
      ui.picked.value = new Set();
      return had;
    }
    const f = feedKey(this.state.keymap, this.mode(), ui.seq.value, key, this.now());
    ui.seq.value = f.state;
    if (f.waiting) return true;
    if (!f.command || this.d.skip?.has(f.command)) return false;
    void this.execute(f.command, f.key);
    return true;
  }

  /**
   * A key pressed inside a focused terminal (INSERT). True when the GUI
   * takes it (ctrl+] back to NORMAL, ctrl+k the switcher, a pending
   * confirmation); everything else goes to the PTY.
   */
  handleInsertKey(ev: KeyLike): boolean {
    if (isLeaveInsert(ev)) {
      this.leaveInsert();
      return true;
    }
    const key = keyString(ev);
    if (key === null) return false;
    if (this.state.pending.length) return this.confirmKey(key);
    const cmd = resolve(this.state.keymap, MODE_INSERT, key);
    if (cmd === 'switcher.open') {
      this.leaveInsert();
      this.d.ui.overlay.value = { kind: 'switcher' };
      return true;
    }
    return false;
  }

  leaveInsert(): void {
    this.d.windows.blur();
    this.d.ui.mode.value = 'normal';
  }

  private confirmKey(key: string): boolean {
    const p = this.state.pending[0]!;
    if (key === 'y' || key === 'Y' || key === 'enter') void this.confirm(p.token, true);
    else if (key === 'n' || key === 'N' || key === 'esc') void this.confirm(p.token, false);
    return true; // modal: nothing else runs while it is open
  }

  async confirm(token: string, yes: boolean): Promise<void> {
    try {
      await this.d.sync.confirm(token, yes);
    } catch (e) {
      this.fail(e);
    }
  }

  private nudge(key: string): boolean {
    const step = key.includes('shift+') ? NUDGE_PX * 4 : NUDGE_PX;
    const dir = key.slice(key.lastIndexOf('+') + 1);
    const [dx, dy] = dir === 'up' ? [0, -step] : dir === 'down' ? [0, step] : dir === 'left' ? [-step, 0] : [step, 0];
    return this.d.windows.nudgeActive(dx, dy);
  }

  /** Puts the cursor on row i (a click on the tree). */
  moveTo(i: number): void {
    const r = this.state.tree.rows[i];
    if (r) this.d.store.update(patchViewState, { cursor: i, cursor_key: r.key });
  }

  private moveCursor(delta: number): void {
    this.d.store.update(patchViewState, nav.moveCursor(this.state.tree, delta).patch);
  }

  private rebuilds: Promise<void> = Promise.resolve();

  /**
   * Runs a view step after the rebuilds already asked for: rebuilds go one
   * after the other, so the last one asked is the last one shown (a filter
   * fires one per key), and each step is computed from the tree the one
   * before it left (tab then tab moves two tabs).
   */
  private step(make: () => nav.NavStep): Promise<void> {
    const next = this.rebuilds.then(async () => {
      const s = make();
      if (s.rebuild) await this.d.sync.refreshTree(s.patch);
      else if (Object.keys(s.patch).length) this.d.store.update(patchViewState, s.patch);
    });
    this.rebuilds = next.catch(() => {});
    return next;
  }

  /** Rebuilds the tree with patch (tab, filter, folds…), in order. */
  refresh(patch: ViewState): Promise<void> {
    return this.step(() => ({ patch, rebuild: true }));
  }

  private cursorSession(): string | undefined {
    return this.d.store.cursorRow.value?.session_id;
  }

  private command(name: string): CommandInfo | undefined {
    return this.state.commands.find((c) => c.name === name);
  }

  /** Runs a command by name, as if its key was pressed (key: the one
   * pressed, for commands that care; the palette passes the first). */
  async execute(name: string, _key = ''): Promise<void> {
    const ui = this.d.ui;
    const tree = this.state.tree;
    const page = Math.max(this.d.pageRows?.() ?? 20, 2);
    switch (name) {
      case 'nav.down':
        return this.moveCursor(1);
      case 'nav.up':
        return this.moveCursor(-1);
      case 'nav.top':
        return this.moveCursor(-(1 << 30));
      case 'nav.bottom':
        return this.moveCursor(1 << 30);
      case 'nav.half_down':
        return this.moveCursor(page >> 1);
      case 'nav.half_up':
        return this.moveCursor(-(page >> 1));
      case 'nav.page_down':
        return this.moveCursor(page);
      case 'nav.page_up':
        return this.moveCursor(-page);
      case 'nav.collapse':
        return this.step(() => nav.collapse(this.state.tree));
      case 'nav.expand':
        return this.step(() => nav.expand(this.state.tree));
      case 'nav.pick':
        ui.picked.value = nav.togglePick(tree, ui.picked.value);
        return;
      case 'nav.next_attention':
        return this.step(() => nav.jumpAttention(this.state.tree, 1));
      case 'nav.prev_attention':
        return this.step(() => nav.jumpAttention(this.state.tree, -1));
      case 'tab.sessions':
      case 'tab.inbox':
      case 'tab.todos':
      case 'tab.archive':
        return this.step(() => nav.toTab(name.slice(4) as TabName));
      case 'tab.next':
        return this.step(() => nav.cycleTab(this.state.tree, 1));
      case 'tab.prev':
        return this.step(() => nav.cycleTab(this.state.tree, -1));
      case 'view.filter':
        ui.overlay.value = { kind: 'filter', value: tree.state.filter ?? '' };
        return;
      case 'view.show_all':
        return this.step(() => nav.toggleShowAll(this.state.tree));
      case 'view.sort':
        return this.step(() => nav.nextSort(this.state.tree));
      case 'view.theme': {
        const th = this.state.theme;
        const next = th && nav.nextTheme(th.themes, th.name);
        if (next) await this.setTheme(next);
        return;
      }
      case 'palette.open':
        ui.overlay.value = { kind: 'palette', query: '' };
        return;
      case 'switcher.open':
        ui.overlay.value = { kind: 'switcher' };
        return;
      case 'help.open':
        ui.overlay.value = { kind: 'help' };
        return;
      case 'usage.show':
        ui.overlay.value = { kind: 'usage' };
        return;
      case 'app.quit':
        this.toast('feche a janela para sair — as sessões seguem rodando em background');
        return;
      case 'session.insert': {
        if (this.d.windows.focusActive()) return;
        const sid = this.cursorSession();
        if (sid) this.d.windows.open(sid, true);
        return;
      }
      case 'session.open':
      case 'session.preview':
      case 'session.fullscreen': {
        const row = this.d.store.cursorRow.value;
        if (row?.kind === 'ctx') {
          return this.step(() => {
            const c = this.state.tree.state.collapsed ?? {};
            return { patch: { collapsed: { ...c, [row.key]: !c[row.key] } }, rebuild: true };
          });
        }
        if (!row?.session_id) return;
        this.d.windows.open(row.session_id, name !== 'session.preview');
        if (name === 'session.fullscreen') this.d.windows.zoomActive();
        return;
      }
      case 'pane.close':
        this.d.windows.closeActive();
        return;
      case 'pane.zoom':
        this.d.windows.zoomActive();
        return;
    }
    const extra = this.d.extra?.[name];
    if (extra) {
      extra();
      return;
    }
    const info = this.command(name);
    if (info?.scope === 'client') {
      this.toast(`${name}: ainda não existe no GUI`, true);
      return;
    }
    if (info && (hasRequired(info) || PROMPTED.has(name))) {
      ui.overlay.value = { kind: 'palette', query: `${name} ` };
      return;
    }
    await this.runCore(name, {});
  }

  /** Runs a core command with this client's selection and shows its result. */
  async runCore(name: string, args: Record<string, string>): Promise<Result | undefined> {
    const ui = this.d.ui;
    try {
      let r: Result;
      const ctx = this.context();
      if (ctx.picked?.length) {
        r = await this.d.client.run(name, args, ctx);
        this.d.store.update(applyResult, r, toastIds--, new Date(this.now()).toISOString());
      } else {
        r = await this.d.sync.run(name, args);
      }
      if (name === 'group.add' || name === 'group.remove') ui.picked.value = new Set();
      return r;
    } catch (e) {
      this.fail(e);
      return undefined;
    }
  }

  private fail(e: unknown): void {
    const msg = e instanceof RpcError || e instanceof Error ? e.message : String(e);
    this.toast(msg, true);
  }

  async setTheme(name: string): Promise<void> {
    try {
      const th = await this.d.client.theme(name);
      this.d.store.update(applyTheme, th);
    } catch (e) {
      this.fail(e);
    }
  }

  /** Runs a palette/switcher item. */
  async runAction(a: PaletteAction): Promise<void> {
    const ui = this.d.ui;
    switch (a.kind) {
      case 'none':
        return;
      case 'prefill':
        ui.overlay.value = { kind: 'palette', query: a.text };
        return;
    }
    ui.overlay.value = null;
    switch (a.kind) {
      case 'run':
        return this.execute(a.name, a.key);
      case 'typed':
        await this.runCore(a.name, a.args);
        return;
      case 'theme':
        return this.setTheme(a.name);
      case 'lua': {
        try {
          const r = await this.d.client.luaEval(a.code);
          if (r.error) this.toast(r.error, true);
          else this.toast(r.result || 'ok');
        } catch (e) {
          this.fail(e);
        }
        return;
      }
      case 'goto': {
        const tree = this.state.tree;
        const onTree = tree.rows.some((r) => r.session_id === a.sessionId && r.kind === 'sess');
        if (!onTree && tree.state.tab !== 'sessions') await this.d.sync.refreshTree({ tab: 'sessions' });
        this.d.store.update(patchViewState, nav.cursorToSession(this.state.tree, a.sessionId).patch);
        if (a.open) this.d.windows.open(a.sessionId, true);
        return;
      }
    }
  }
}

export function createKeyController(deps: KeyDeps): KeyController {
  return new KeyController(deps);
}
