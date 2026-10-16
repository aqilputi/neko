// WS /v1/panes/{id}/ws: raw PTY bytes both ways in binary frames, JSON
// control frames (resize/title/exit) as text frames.

import type { Control } from './types.ts';

/** WebSocket.readyState values. */
export const WS_CONNECTING = 0;
export const WS_OPEN = 1;
export const WS_CLOSING = 2;
export const WS_CLOSED = 3;

/** The part of WebSocket the pane socket uses (so tests can fake it). */
export interface WebSocketLike {
  binaryType: string;
  readonly readyState: number;
  onopen: ((ev: Event) => unknown) | null;
  onmessage: ((ev: MessageEvent) => unknown) | null;
  onclose: ((ev: CloseEvent) => unknown) | null;
  onerror: ((ev: Event) => unknown) | null;
  send(data: string | ArrayBufferLike | ArrayBufferView): void;
  close(code?: number, reason?: string): void;
}

export type WebSocketCtor = new (url: string) => WebSocketLike;

/** Close code the server uses when the program ended before we could see
 * its exit status: reconnect (protocolo.md, "Painéis"). */
export const WS_TRY_AGAIN = 1013;

export interface PaneHandlers {
  onOpen?(): void;
  /** Raw PTY output. */
  onData?(bytes: Uint8Array): void;
  /** OSC title as relayed by the server. */
  onTitle?(text: string): void;
  /** The program ended (always the last frame before a 1000 close). */
  onExit?(err?: string): void;
  /** Any control frame, including unknown types. */
  onControl?(c: Control): void;
  onClose?(code: number, reason: string): void;
}

const encoder = new TextEncoder();

export class PaneSocket {
  private readonly ws: WebSocketLike;
  private queue: Array<string | Uint8Array> = [];

  constructor(
    readonly id: string,
    url: string,
    Ctor: WebSocketCtor,
    private readonly handlers: PaneHandlers = {},
  ) {
    const ws = new Ctor(url);
    ws.binaryType = 'arraybuffer';
    this.ws = ws;
    ws.onopen = () => {
      const q = this.queue;
      this.queue = [];
      for (const f of q) ws.send(f);
      this.handlers.onOpen?.();
    };
    ws.onmessage = (ev) => this.onMessage(ev.data);
    ws.onclose = (ev) => {
      this.queue = [];
      this.handlers.onClose?.(ev.code, ev.reason);
    };
  }

  get readyState(): number {
    return this.ws.readyState;
  }

  /** Sends keyboard/paste input to the PTY (always a binary frame). */
  write(data: string | Uint8Array): void {
    this.send(typeof data === 'string' ? encoder.encode(data) : data);
  }

  /** Asks the server to resize the PTY (a text control frame). */
  resize(cols: number, rows: number): void {
    const c: Control = { type: 'resize', cols, rows };
    this.send(JSON.stringify(c));
  }

  /** Closes this viewer; the pane itself stays open on the server. */
  close(code = 1000, reason = ''): void {
    this.queue = [];
    if (this.ws.readyState === WS_CLOSING || this.ws.readyState === WS_CLOSED) return;
    this.ws.close(code, reason);
  }

  private send(frame: string | Uint8Array): void {
    switch (this.ws.readyState) {
      case WS_CONNECTING:
        this.queue.push(frame);
        return;
      case WS_OPEN:
        this.ws.send(frame);
        return;
      default:
        // closing/closed: input has nowhere to go
        return;
    }
  }

  private onMessage(data: unknown): void {
    if (typeof data === 'string') {
      let c: Control;
      try {
        c = JSON.parse(data) as Control;
      } catch {
        return;
      }
      this.handlers.onControl?.(c);
      if (c.type === 'title') this.handlers.onTitle?.(c.text ?? '');
      else if (c.type === 'exit') this.handlers.onExit?.(c.err || undefined);
      return;
    }
    if (data instanceof ArrayBuffer) {
      this.handlers.onData?.(new Uint8Array(data));
    } else if (ArrayBuffer.isView(data)) {
      this.handlers.onData?.(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    }
  }
}
