// The window manager: the layout state (layout.ts) in a signal, one
// TermSession per open window, and the commands the keyboard, the tree,
// the scene and the inbox cat use (open, close, zoom, focus, nudge).

import { signal } from '@preact/signals';
import type { NkEvent, PaneEvent } from '../client/types.ts';
import { Ev } from '../client/types.ts';
import type { Ui } from '../keymap/ui.ts';
import { cursorToSession } from '../keymap/nav.ts';
import { patchViewState, pushToast } from '../state/reducers.ts';
import type { Store } from '../state/store.ts';
import * as L from './layout.ts';
import { TermSession, type PaneClient, type TermHandlers, type TermView } from './term.ts';

export interface ManagerDeps {
  client: PaneClient;
  store: Store;
  ui: Ui;
  /** Makes the session of a window (tests pass fakes). */
  makeSession?: (sessionId: string, client: PaneClient, h: TermHandlers) => TermSession;
}

let toastIds = -2_000_000;

export class WindowManager {
  readonly state = signal<L.WinState>(L.emptyWins());
  readonly area = signal<L.Rect>({ x: 0, y: 0, w: 1200, h: 700 });
  private readonly sessions = new Map<string, TermSession>();
  private focusWanted: string | null = null;

  constructor(private readonly d: ManagerDeps) {}

  private set(s: L.WinState): void {
    this.state.value = s;
  }

  private label(id: string): string {
    const s = this.d.store.sessionsById.value.get(id);
    if (s) return s.label;
    const r = this.d.store.state.value.tree.rows.find((x) => x.kind === 'sess' && x.session_id === id);
    return r?.label ?? id.slice(0, 8);
  }

  private toast(text: string, err: boolean): void {
    this.d.store.update(pushToast, { id: toastIds--, text, err, at: new Date().toISOString() });
  }

  get active(): string | null {
    return this.state.value.active;
  }

  /** The session of an open window (for tests and the debug console). */
  session(id: string): TermSession | undefined {
    return this.sessions.get(id);
  }

  /** Opens a session's window (session.open), or raises the open one;
   * focus also puts its terminal in INSERT. The tree cursor follows. */
  open(id: string, focus = true): void {
    this.set(L.openWin(this.state.value, id, this.label(id), this.area.value));
    const tree = this.d.store.state.value.tree;
    const step = cursorToSession(tree, id);
    if (Object.keys(step.patch).length) this.d.store.update(patchViewState, step.patch);
    if (!this.sessions.has(id)) {
      const h: TermHandlers = {
        onPane: (p) => this.set(L.setPane(this.state.value, id, p)),
        onTitle: (t) => this.set(L.setTitle(this.state.value, id, t)),
        onExit: (err) => {
          this.toast(err ? `painel encerrado: ${err}` : `painel de ${this.label(id)} encerrado`, !!err);
          this.close(id, false);
        },
        onError: (msg) => this.set(L.setStatus(this.state.value, id, 'error', msg)),
      };
      const s = this.d.makeSession ? this.d.makeSession(id, this.d.client, h) : new TermSession(id, this.d.client, h);
      this.sessions.set(id, s);
      void s.start();
    }
    if (focus) this.focusTerm(id);
  }

  /** Focuses the window's terminal (INSERT), now or once it mounts. */
  focusTerm(id: string): boolean {
    if (!L.find(this.state.value, id)) return false;
    this.raise(id);
    const view = this.sessions.get(id)?.mounted;
    if (view) {
      this.focusWanted = null;
      view.focus();
      this.d.ui.mode.value = 'insert';
    } else {
      this.focusWanted = id;
    }
    return true;
  }

  /** The window's terminal view is ready (TermWindow mounted it). */
  mount(id: string, view: TermView): void {
    const s = this.sessions.get(id);
    if (!s) return;
    s.mount(view);
    if (this.focusWanted === id) this.focusTerm(id);
  }

  unmount(id: string): void {
    this.sessions.get(id)?.unmount();
  }

  /** Closes a window; kill also closes the pane in the core. */
  close(id: string, kill = true): void {
    const s = this.sessions.get(id);
    const wasInsert = !!s?.mounted && this.d.ui.mode.value === 'insert' && this.active === id;
    s?.close(kill);
    this.sessions.delete(id);
    if (this.focusWanted === id) this.focusWanted = null;
    this.set(L.closeWin(this.state.value, id));
    if (wasInsert) this.d.ui.mode.value = 'normal';
  }

  /** Shows an error in the window (its terminal could not start). */
  fail(id: string, message: string): void {
    this.set(L.setStatus(this.state.value, id, 'error', message));
  }

  raise(id: string): void {
    this.set(L.focusWin(this.state.value, id));
  }

  closeActive(): boolean {
    const a = this.active;
    if (!a) return false;
    this.close(a, true);
    return true;
  }

  zoomActive(): boolean {
    const a = this.active;
    if (!a) return false;
    this.set(L.toggleZoom(this.state.value, a));
    return true;
  }

  focusActive(): boolean {
    const a = this.active;
    return a ? this.focusTerm(a) : false;
  }

  nudgeActive(dx: number, dy: number): boolean {
    const a = this.active;
    if (!a) return false;
    this.set(L.nudgeWin(this.state.value, a, dx, dy, this.area.value));
    return true;
  }

  moveTo(id: string, x: number, y: number): void {
    this.set(L.moveWin(this.state.value, id, x, y, this.area.value));
  }

  resize(id: string, w: number, h: number): void {
    this.set(L.resizeWin(this.state.value, id, w, h, this.area.value));
  }

  toggleZoom(id: string): void {
    this.set(L.toggleZoom(this.state.value, id));
  }

  /** Leaves INSERT: blurs every terminal. */
  blur(): void {
    this.focusWanted = null;
    for (const s of this.sessions.values()) s.mounted?.blur();
  }

  setArea(area: L.Rect): void {
    const a = this.area.value;
    if (a.x === area.x && a.y === area.y && a.w === area.w && a.h === area.h) return;
    this.area.value = area;
    this.set(L.clampAll(this.state.value, area));
  }

  /** Bus events: a pane closed or exited elsewhere closes its window. */
  onEvent(ev: NkEvent): void {
    if (ev.name !== Ev.PaneClosed && ev.name !== Ev.PaneExited) return;
    const p = ev.data as PaneEvent | undefined;
    const w = p?.id ? L.byPane(this.state.value, p.id) : undefined;
    if (w) this.close(w.id, false);
  }

  /** The text of a window's screen ("" when it is not open). */
  text(id: string): string {
    return this.sessions.get(id)?.text() ?? '';
  }

  /** Keeps window titles in step with the sessions' labels. */
  syncLabels(): void {
    let s = this.state.value;
    for (const w of s.wins) s = L.setLabel(s, w.id, this.label(w.id));
    if (s.wins.some((w, i) => w !== this.state.value.wins[i])) this.set(s);
  }
}
