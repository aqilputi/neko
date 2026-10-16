// The typed client of the core's protocol v1 (docs/protocolo.md). It only
// speaks HTTP+JSON, SSE and the pane WebSocket — no state of its own besides
// the endpoint; src/state keeps what the GUI shows.

import type { Endpoint } from './endpoint.ts';
import { RpcError } from './errors.ts';
import { EventStream, type EventSourceCtor, type EventStreamHandlers, type EventStreamOptions } from './events.ts';
import { PaneSocket, type PaneHandlers, type WebSocketCtor } from './pane.ts';
import type {
  Binding,
  CommandInfo,
  Context,
  Hello,
  LuaEval,
  Pane,
  PaneOpenRequest,
  PaneOpened,
  Result,
  Snapshot,
  Theme,
  Tree,
  ViewState,
} from './types.ts';

export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

/** Platform primitives, injectable for tests (defaults: the globals). */
export interface ClientDeps {
  fetch?: FetchLike;
  EventSource?: EventSourceCtor;
  WebSocket?: WebSocketCtor;
}

type Query = Record<string, string | number | undefined>;

/** Query parameters of GET /v1/commands for a selection. */
export function contextQuery(ctx: Context = {}): Query {
  return {
    session_id: ctx.session_id || undefined,
    picked: ctx.picked?.length ? ctx.picked.join(',') : undefined,
    tab: ctx.tab || undefined,
    todo_id: ctx.todo_id || undefined,
    issue_key: ctx.issue_key || undefined,
  };
}

export class NkClient {
  readonly endpoint: Endpoint;
  private readonly fetchFn: FetchLike;
  private readonly ES: EventSourceCtor | undefined;
  private readonly WS: WebSocketCtor | undefined;

  constructor(endpoint: Endpoint, deps: ClientDeps = {}) {
    this.endpoint = endpoint;
    this.fetchFn = deps.fetch ?? ((input, init) => globalThis.fetch(input, init));
    this.ES = deps.EventSource ?? (globalThis.EventSource as EventSourceCtor | undefined);
    this.WS = deps.WebSocket ?? (globalThis.WebSocket as unknown as WebSocketCtor | undefined);
  }

  /** Absolute URL of path (e.g. "/v1/hello") with query; withToken adds ?token=. */
  url(path: string, query: Query = {}, withToken = false): string {
    const u = new URL(this.endpoint.url + path);
    for (const [k, v] of Object.entries(query)) {
      if (v !== undefined && v !== '') u.searchParams.set(k, String(v));
    }
    if (withToken && this.endpoint.token) u.searchParams.set('token', this.endpoint.token);
    return u.toString();
  }

  /** ws(s):// URL of path, with ?token= (WebSockets cannot set headers). */
  wsURL(path: string): string {
    return this.url(path, {}, true).replace(/^http/, 'ws');
  }

  private async request<T>(method: string, path: string, body?: unknown, query?: Query): Promise<T> {
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (this.endpoint.token) headers.Authorization = `Bearer ${this.endpoint.token}`;
    const init: RequestInit = { method, headers };
    if (body !== undefined) {
      headers['Content-Type'] = 'application/json';
      init.body = JSON.stringify(body);
    }
    const res = await this.fetchFn(this.url(path, query), init);
    const text = res.status === 204 ? '' : await res.text();
    if (!res.ok) throw RpcError.from(res.status, text);
    return (text ? JSON.parse(text) : undefined) as T;
  }

  /** GET /v1/hello. */
  hello(): Promise<Hello> {
    return this.request('GET', '/v1/hello');
  }

  /** GET /v1/snapshot. */
  snapshot(): Promise<Snapshot> {
    return this.request('GET', '/v1/snapshot');
  }

  /** GET /v1/commands, with `available` computed for ctx. */
  async commands(ctx?: Context): Promise<CommandInfo[]> {
    return (await this.request<CommandInfo[] | null>('GET', '/v1/commands', undefined, contextQuery(ctx))) ?? [];
  }

  /** GET /v1/keymap. */
  async keymap(): Promise<Binding[]> {
    return (await this.request<Binding[] | null>('GET', '/v1/keymap')) ?? [];
  }

  /** POST /v1/commands/{name}. */
  async run(name: string, args?: Record<string, string>, context: Context = {}): Promise<Result> {
    const body = args && Object.keys(args).length ? { args, context } : { context };
    return (await this.request<Result>('POST', `/v1/commands/${encodeURIComponent(name)}`, body)) ?? {};
  }

  /** POST /v1/confirm/{token}. */
  async confirm(token: string, yes: boolean): Promise<Result> {
    return (await this.request<Result>('POST', `/v1/confirm/${encodeURIComponent(token)}`, { yes })) ?? {};
  }

  /** POST /v1/view/tree: send back the state you got last time. */
  async viewTree(state: ViewState = {}): Promise<Tree> {
    const t = await this.request<Tree>('POST', '/v1/view/tree', { state });
    return { ...t, rows: t.rows ?? [], inbox: t.inbox ?? [] };
  }

  /** GET /v1/theme (the config's theme, or ?name=). */
  theme(name?: string): Promise<Theme> {
    return this.request('GET', '/v1/theme', undefined, { name });
  }

  /** POST /v1/lua/eval. */
  luaEval(code: string): Promise<LuaEval> {
    return this.request('POST', '/v1/lua/eval', { code });
  }

  /** GET /v1/panes: the panes this server opened. */
  async panes(): Promise<Pane[]> {
    return (await this.request<Pane[] | null>('GET', '/v1/panes')) ?? [];
  }

  /** POST /v1/panes. */
  openPane(req: PaneOpenRequest): Promise<PaneOpened> {
    return this.request('POST', '/v1/panes', req);
  }

  /** POST /v1/panes/{id}/resize (last write wins). */
  async resizePane(id: string, cols: number, rows: number): Promise<void> {
    await this.request('POST', `/v1/panes/${encodeURIComponent(id)}/resize`, { cols, rows });
  }

  /** DELETE /v1/panes/{id} (SIGHUP to the program). */
  async closePane(id: string): Promise<void> {
    await this.request('DELETE', `/v1/panes/${encodeURIComponent(id)}`);
  }

  /** GET /v1/events as a resuming stream. */
  events(handlers: EventStreamHandlers, opts: EventStreamOptions = {}): EventStream {
    if (!this.ES) throw new Error('EventSource indisponível');
    const urlFor = (since: number | undefined) => this.url('/v1/events', { since }, true);
    return new EventStream(urlFor, this.ES, handlers, opts);
  }

  /** WS /v1/panes/{id}/ws. */
  attachPane(id: string, handlers: PaneHandlers = {}): PaneSocket {
    if (!this.WS) throw new Error('WebSocket indisponível');
    return new PaneSocket(id, this.wsURL(`/v1/panes/${encodeURIComponent(id)}/ws`), this.WS, handlers);
  }
}

/** Shorthand for new NkClient(endpoint, deps). */
export function createClient(endpoint: Endpoint, deps?: ClientDeps): NkClient {
  return new NkClient(endpoint, deps);
}
