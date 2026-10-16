// GET /v1/events: the core's event bus over SSE, with resume.
//
// The browser's EventSource reconnects by itself after a dropped
// connection and sends Last-Event-ID, so the server replays what we missed
// (or sends `gap` if its ring no longer covers it). When the EventSource
// gives up (readyState CLOSED: an HTTP error, the server closing), we open
// a new one ourselves with `?since=<last seq>` after a backoff. Either way
// a `gap` — or a hard reconnect with no known seq — is surfaced as
// onResync: re-read the snapshot, something was lost.

import { EV_GAP, EVENT_NAMES, type Gap, type NkEvent } from './types.ts';

/** EventSource.readyState values. */
export const ES_CONNECTING = 0;
export const ES_OPEN = 1;
export const ES_CLOSED = 2;

/** The part of EventSource the stream uses (so tests can fake it). */
export interface EventSourceLike {
  readonly readyState: number;
  onopen: ((ev: Event) => unknown) | null;
  onerror: ((ev: Event) => unknown) | null;
  addEventListener(type: string, listener: (ev: MessageEvent) => unknown): void;
  close(): void;
}

export type EventSourceCtor = new (url: string) => EventSourceLike;

export type StreamStatus = 'connecting' | 'open' | 'reconnecting' | 'closed';

/** Why the client should re-read the snapshot. */
export type ResyncReason = 'gap' | 'reconnect';

export interface EventStreamHandlers {
  /** Every bus event, in order, at most once per seq. */
  onEvent?(ev: NkEvent): void;
  /** Something may have been lost: re-read snapshot (and tree). */
  onResync?(reason: ResyncReason, gap?: Gap): void;
  onStatus?(status: StreamStatus): void;
}

export interface EventStreamOptions {
  /** Resume after this seq (0 = everything the server keeps). */
  since?: number;
  /** Event names to listen to (EventSource has no wildcard). */
  names?: readonly string[];
  /** First backoff for a hard reconnect, doubled up to maxRetryMs. */
  retryMs?: number;
  maxRetryMs?: number;
}

export class EventStream {
  private es: EventSourceLike | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private retry: number;
  private closed = false;
  private hard = false; // the current EventSource replaces one we closed
  private _status: StreamStatus = 'connecting';
  /** Seq of the last event delivered (undefined before the first). */
  lastSeq: number | undefined;

  constructor(
    private readonly urlFor: (since: number | undefined) => string,
    private readonly Ctor: EventSourceCtor,
    private readonly handlers: EventStreamHandlers,
    private readonly opts: EventStreamOptions = {},
  ) {
    this.lastSeq = opts.since;
    this.retry = opts.retryMs ?? 1000;
    this.open(opts.since);
  }

  get status(): StreamStatus {
    return this._status;
  }

  /** Stops the stream for good. */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.es?.close();
    this.es = null;
    this.setStatus('closed');
  }

  private setStatus(s: StreamStatus): void {
    if (s === this._status) return;
    this._status = s;
    this.handlers.onStatus?.(s);
  }

  private open(since: number | undefined): void {
    const es = new this.Ctor(this.urlFor(since));
    this.es = es;
    es.onopen = () => {
      if (es !== this.es) return;
      this.retry = this.opts.retryMs ?? 1000;
      this.setStatus('open');
      if (this.hard && this.lastSeq === undefined) {
        // nothing to resume from: whatever happened meanwhile is lost
        this.handlers.onResync?.('reconnect');
      }
      this.hard = false;
    };
    es.onerror = () => {
      if (es !== this.es || this.closed) return;
      if (es.readyState === ES_CLOSED) {
        es.close();
        this.es = null;
        this.setStatus('reconnecting');
        this.schedule();
      } else {
        // the browser retries by itself, with Last-Event-ID
        this.setStatus('reconnecting');
      }
    };
    for (const name of this.opts.names ?? EVENT_NAMES) {
      es.addEventListener(name, (m) => this.onMessage(es, m));
    }
    es.addEventListener(EV_GAP, (m) => {
      if (es !== this.es) return;
      const gap = parse<Gap>(m.data) ?? { since: this.lastSeq ?? 0, oldest: 0 };
      // the numbering may have restarted (another server process): accept
      // whatever seq comes next
      this.lastSeq = undefined;
      this.handlers.onResync?.('gap', gap);
    });
  }

  private schedule(): void {
    const wait = this.retry;
    this.retry = Math.min(this.retry * 2, this.opts.maxRetryMs ?? 30000);
    this.timer = setTimeout(() => {
      this.timer = null;
      if (this.closed) return;
      this.hard = true;
      this.open(this.lastSeq);
    }, wait);
  }

  private onMessage(es: EventSourceLike, m: MessageEvent): void {
    if (es !== this.es) return;
    const ev = parse<NkEvent>(m.data);
    if (!ev || typeof ev.name !== 'string') return;
    if (typeof ev.seq !== 'number') {
      const id = Number(m.lastEventId);
      ev.seq = Number.isFinite(id) ? id : 0;
    }
    if (this.lastSeq !== undefined && ev.seq <= this.lastSeq) return; // replayed
    this.lastSeq = ev.seq;
    this.handlers.onEvent?.(ev);
  }
}

function parse<T>(data: unknown): T | null {
  if (typeof data !== 'string') return null;
  try {
    return JSON.parse(data) as T;
  } catch {
    return null;
  }
}
