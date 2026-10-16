// One terminal window's link to the core: POST /v1/panes (the core picks
// resume/attach as the TUI's openPane), then WS /v1/panes/{id}/ws feeding a
// terminal view. The view (xterm.js, xterm.ts) is behind TermView so this
// runs in tests with fakes; output that arrives before the view mounts is
// buffered.

import type { NkClient } from '../client/client.ts';
import type { PaneSocket } from '../client/pane.ts';
import { WS_TRY_AGAIN } from '../client/pane.ts';

export interface Disposable {
  dispose(): void;
}

/** The part of an xterm.js Terminal (plus fit/text) a window uses. */
export interface TermView {
  readonly cols: number;
  readonly rows: number;
  write(data: string | Uint8Array): void;
  onData(cb: (data: string) => void): Disposable;
  onBinary(cb: (data: string) => void): Disposable;
  onResize(cb: (size: { cols: number; rows: number }) => void): Disposable;
  onTitleChange(cb: (title: string) => void): Disposable;
  focus(): void;
  blur(): void;
  /** Everything on the screen and scrollback, as text. */
  text(): string;
}

export type PaneClient = Pick<NkClient, 'openPane' | 'attachPane' | 'closePane'>;

export interface TermHandlers {
  onPane(paneId: string): void;
  onTitle(title: string): void;
  onExit(err?: string): void;
  onError(message: string): void;
}

/** Default PTY size asked before the view measured itself. */
export const DEFAULT_COLS = 100;
export const DEFAULT_ROWS = 30;

const decoder = new TextDecoder();

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export class TermSession {
  paneId: string | undefined;
  private socket: PaneSocket | undefined;
  private view: TermView | undefined;
  private subs: Disposable[] = [];
  private buffer: Uint8Array[] = [];
  private closed = false;
  private exited = false;
  private retried = false;

  constructor(
    readonly sessionId: string,
    private readonly client: PaneClient,
    private readonly h: TermHandlers,
  ) {}

  get mounted(): TermView | undefined {
    return this.view;
  }

  /** Opens (or reuses) the core's pane and attaches to it. */
  async start(cols = DEFAULT_COLS, rows = DEFAULT_ROWS): Promise<void> {
    try {
      const r = await this.client.openPane({ session_id: this.sessionId, cols, rows });
      if (this.closed) {
        if (!r.reused) void this.client.closePane(r.pane_id).catch(() => {});
        return;
      }
      this.paneId = r.pane_id;
      this.h.onPane(r.pane_id);
      this.connect();
    } catch (e) {
      if (!this.closed) this.h.onError(message(e));
    }
  }

  private connect(): void {
    const id = this.paneId!;
    this.socket = this.client.attachPane(id, {
      onData: (b) => {
        if (this.view) this.view.write(b);
        else this.buffer.push(b.slice());
      },
      onTitle: (t) => {
        if (t) this.h.onTitle(t);
      },
      onExit: (err) => {
        this.exited = true;
        this.h.onExit(err);
      },
      onClose: (code) => {
        if (this.closed || this.exited) return;
        if (code === WS_TRY_AGAIN && !this.retried) {
          // the program ended before we saw its status: reconnect once
          this.retried = true;
          this.connect();
          return;
        }
        this.h.onError('a conexão com o painel caiu');
      },
    });
    if (this.view) this.socket.resize(this.view.cols, this.view.rows);
  }

  /** Attaches the terminal view: replays what arrived, wires input/size. */
  mount(view: TermView): void {
    this.unmount();
    this.view = view;
    for (const b of this.buffer) view.write(b);
    this.buffer = [];
    this.subs = [
      view.onData((d) => this.socket?.write(d)),
      view.onBinary((d) => this.socket?.write(Uint8Array.from(d, (c) => c.charCodeAt(0) & 0xff))),
      view.onResize(({ cols, rows }) => this.socket?.resize(cols, rows)),
      view.onTitleChange((t) => {
        if (t) this.h.onTitle(t);
      }),
    ];
    this.socket?.resize(view.cols, view.rows);
  }

  unmount(): void {
    for (const s of this.subs) s.dispose();
    this.subs = [];
    this.view = undefined;
  }

  /** Stops viewing; kill also closes the pane in the core (SIGHUP, as the
   * TUI's w) — never when the pane already went away. */
  close(kill: boolean): void {
    if (this.closed) return;
    this.closed = true;
    this.unmount();
    this.socket?.close();
    if (kill && this.paneId && !this.exited) void this.client.closePane(this.paneId).catch(() => {});
  }

  /** The screen as text (the buffered bytes before the view mounted). */
  text(): string {
    if (this.view) return this.view.text();
    return this.buffer.map((b) => decoder.decode(b, { stream: true })).join('');
  }
}
