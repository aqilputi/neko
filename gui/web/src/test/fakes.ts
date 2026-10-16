// Small in-memory fakes of fetch, EventSource and WebSocket for the unit
// tests: no network, everything driven by the test.

import type { FetchLike } from '../client/client.ts';
import type { EventSourceLike } from '../client/events.ts';
import type { WebSocketLike } from '../client/pane.ts';

// ---- fetch ----

export interface FetchCall {
  method: string;
  url: URL;
  headers: Record<string, string>;
  body: unknown;
}

export type Reply = { status?: number; body?: unknown; text?: string };
export type Route = Reply | ((call: FetchCall) => Reply | Promise<Reply>);

/** A fetch answering from "METHOD /path" routes; unknown routes are 404. */
export function fakeFetch(routes: Record<string, Route> = {}) {
  const calls: FetchCall[] = [];
  const fn: FetchLike = async (input, init = {}) => {
    const url = new URL(input);
    const method = init.method ?? 'GET';
    const headers = { ...(init.headers as Record<string, string> | undefined) };
    const body = typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    const call: FetchCall = { method, url, headers, body };
    calls.push(call);
    const route = routes[`${method} ${url.pathname}`];
    const r: Reply = route
      ? typeof route === 'function'
        ? await route(call)
        : route
      : { status: 404, body: { error: 'não encontrado', code: 'not_found' } };
    const status = r.status ?? 200;
    const text = r.text ?? (r.body === undefined ? '' : JSON.stringify(r.body));
    return new Response(status === 204 ? null : text, { status });
  };
  return { fetch: fn, calls, routes };
}

// ---- EventSource ----

type Listener = (ev: MessageEvent) => unknown;

export class FakeEventSource implements EventSourceLike {
  static instances: FakeEventSource[] = [];
  static reset(): void {
    FakeEventSource.instances = [];
  }
  static get last(): FakeEventSource {
    const es = FakeEventSource.instances.at(-1);
    if (!es) throw new Error('no EventSource');
    return es;
  }

  readyState = 0;
  onopen: ((ev: Event) => unknown) | null = null;
  onerror: ((ev: Event) => unknown) | null = null;
  readonly listeners = new Map<string, Listener[]>();
  closed = false;

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }

  get parsedURL(): URL {
    return new URL(this.url);
  }

  addEventListener(type: string, listener: Listener): void {
    const l = this.listeners.get(type) ?? [];
    l.push(listener);
    this.listeners.set(type, l);
  }

  close(): void {
    this.closed = true;
    this.readyState = 2;
  }

  // driving helpers
  open(): void {
    this.readyState = 1;
    this.onopen?.(new Event('open'));
  }

  /** A dropped connection the browser will retry (readyState CONNECTING). */
  drop(): void {
    this.readyState = 0;
    this.onerror?.(new Event('error'));
  }

  /** A fatal error: the browser gives up (readyState CLOSED). */
  fail(): void {
    this.readyState = 2;
    this.onerror?.(new Event('error'));
  }

  /** Delivers a named SSE message; data is JSON-encoded unless a string. */
  emit(name: string, data: unknown, id?: string): void {
    const raw = typeof data === 'string' ? data : JSON.stringify(data);
    const ev = new MessageEvent(name, { data: raw, lastEventId: id ?? '' });
    for (const l of this.listeners.get(name) ?? []) l(ev);
  }

  /** Delivers a bus event the way the server encodes it. */
  emitEvent(seq: number, name: string, data?: unknown): void {
    this.emit(name, { seq, at: '2026-10-16T20:00:00Z', name, data }, String(seq));
  }
}

// ---- WebSocket ----

export class FakeWebSocket implements WebSocketLike {
  static instances: FakeWebSocket[] = [];
  static reset(): void {
    FakeWebSocket.instances = [];
  }
  static get last(): FakeWebSocket {
    const ws = FakeWebSocket.instances.at(-1);
    if (!ws) throw new Error('no WebSocket');
    return ws;
  }

  binaryType = 'blob';
  readyState = 0;
  onopen: ((ev: Event) => unknown) | null = null;
  onmessage: ((ev: MessageEvent) => unknown) | null = null;
  onclose: ((ev: CloseEvent) => unknown) | null = null;
  onerror: ((ev: Event) => unknown) | null = null;
  readonly sent: Array<string | Uint8Array> = [];
  closeCalls: Array<[number | undefined, string | undefined]> = [];

  constructor(readonly url: string) {
    FakeWebSocket.instances.push(this);
  }

  send(data: string | ArrayBufferLike | ArrayBufferView): void {
    if (typeof data === 'string') this.sent.push(data);
    else if (ArrayBuffer.isView(data)) this.sent.push(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    else this.sent.push(new Uint8Array(data));
  }

  close(code?: number, reason?: string): void {
    this.closeCalls.push([code, reason]);
    this.readyState = 2;
  }

  // driving helpers
  open(): void {
    this.readyState = 1;
    this.onopen?.(new Event('open'));
  }

  receive(data: string | ArrayBuffer | Uint8Array): void {
    this.onmessage?.(new MessageEvent('message', { data }));
  }

  serverClose(code = 1000, reason = ''): void {
    this.readyState = 3;
    this.onclose?.({ code, reason } as CloseEvent);
  }

  /** Text frames sent, parsed as JSON. */
  get controls(): unknown[] {
    return this.sent.filter((f): f is string => typeof f === 'string').map((f) => JSON.parse(f));
  }

  /** Binary frames sent, decoded as UTF-8. */
  get texts(): string[] {
    const d = new TextDecoder();
    return this.sent.filter((f): f is Uint8Array => typeof f !== 'string').map((f) => d.decode(f));
  }
}
