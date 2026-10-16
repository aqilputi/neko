import { beforeEach, describe, expect, it } from 'vitest';
import { FakeEventSource, FakeWebSocket, fakeFetch } from '../test/fakes.ts';
import { NkClient, contextQuery, createClient } from './client.ts';
import { RpcError, isCode } from './errors.ts';

const EP = { url: 'http://127.0.0.1:4321', token: 'tok' };

function client(routes: Parameters<typeof fakeFetch>[0] = {}) {
  const f = fakeFetch(routes);
  const c = createClient(EP, { fetch: f.fetch, EventSource: FakeEventSource, WebSocket: FakeWebSocket });
  return { c, calls: f.calls };
}

beforeEach(() => {
  FakeEventSource.reset();
  FakeWebSocket.reset();
});

describe('NkClient requests', () => {
  it('sends the bearer token and parses hello', async () => {
    const hello = { app: 'nekuroddo', version: 'dev', protocol: 1, capabilities: ['panes', 'demo'], pid: 7 };
    const { c, calls } = client({ 'GET /v1/hello': { body: hello } });
    expect(await c.hello()).toEqual(hello);
    expect(calls[0]?.headers.Authorization).toBe('Bearer tok');
    expect(calls[0]?.url.toString()).toBe('http://127.0.0.1:4321/v1/hello');
  });

  it('omits Authorization without a token', async () => {
    const f = fakeFetch({ 'GET /v1/snapshot': { body: { sessions: [] } } });
    const c = new NkClient({ url: 'http://x' }, { fetch: f.fetch });
    await c.snapshot();
    expect(f.calls[0]?.headers.Authorization).toBeUndefined();
  });

  type Case = {
    name: string;
    call: (c: NkClient) => Promise<unknown>;
    route: string;
    reply?: unknown;
    status?: number;
    want?: unknown;
    body?: unknown;
    query?: Record<string, string>;
  };
  const cases: Case[] = [
    { name: 'snapshot', call: (c) => c.snapshot(), route: 'GET /v1/snapshot', reply: { sessions: [] }, want: { sessions: [] } },
    {
      name: 'commands with context',
      call: (c) => c.commands({ session_id: 'abc', picked: ['a', 'b'], tab: 'inbox', todo_id: 3, issue_key: 'K-1' }),
      route: 'GET /v1/commands',
      reply: [{ name: 'nav.down', available: true }],
      want: [{ name: 'nav.down', available: true }],
      query: { session_id: 'abc', picked: 'a,b', tab: 'inbox', todo_id: '3', issue_key: 'K-1' },
    },
    { name: 'commands null → []', call: (c) => c.commands(), route: 'GET /v1/commands', reply: null, want: [] },
    { name: 'keymap', call: (c) => c.keymap(), route: 'GET /v1/keymap', reply: [{ command: 'x' }], want: [{ command: 'x' }] },
    { name: 'keymap null', call: (c) => c.keymap(), route: 'GET /v1/keymap', reply: null, want: [] },
    {
      name: 'run with args',
      call: (c) => c.run('session.rename', { name: 'novo' }, { session_id: 's1' }),
      route: 'POST /v1/commands/session.rename',
      reply: { toast: 'ok' },
      want: { toast: 'ok' },
      body: { args: { name: 'novo' }, context: { session_id: 's1' } },
    },
    {
      name: 'run without args',
      call: (c) => c.run('sessions.refresh'),
      route: 'POST /v1/commands/sessions.refresh',
      reply: {},
      want: {},
      body: { context: {} },
    },
    {
      name: 'run with an empty answer',
      call: (c) => c.run('x', {}),
      route: 'POST /v1/commands/x',
      status: 200,
      want: {},
      body: { context: {} },
    },
    {
      name: 'confirm yes',
      call: (c) => c.confirm('t/1', true),
      route: 'POST /v1/confirm/t%2F1',
      reply: { data: { refresh: true } },
      want: { data: { refresh: true } },
      body: { yes: true },
    },
    { name: 'confirm empty', call: (c) => c.confirm('t', false), route: 'POST /v1/confirm/t', want: {}, body: { yes: false } },
    {
      name: 'viewTree default state',
      call: (c) => c.viewTree(),
      route: 'POST /v1/view/tree',
      reply: { rows: null, state: { tab: 'sessions' }, inbox: null, hidden: 0, archived: 0 },
      want: { rows: [], state: { tab: 'sessions' }, inbox: [], hidden: 0, archived: 0 },
      body: { state: {} },
    },
    {
      name: 'viewTree sends state back',
      call: (c) => c.viewTree({ tab: 'inbox', cursor: 2 }),
      route: 'POST /v1/view/tree',
      reply: { rows: [{ key: 'ctx:a' }], state: {}, inbox: ['x'], hidden: 1, archived: 0 },
      want: { rows: [{ key: 'ctx:a' }], state: {}, inbox: ['x'], hidden: 1, archived: 0 },
      body: { state: { tab: 'inbox', cursor: 2 } },
    },
    { name: 'theme', call: (c) => c.theme(), route: 'GET /v1/theme', reply: { name: 'a' }, want: { name: 'a' } },
    {
      name: 'theme by name',
      call: (c) => c.theme('dracula'),
      route: 'GET /v1/theme',
      reply: { name: 'dracula' },
      want: { name: 'dracula' },
      query: { name: 'dracula' },
    },
    {
      name: 'luaEval',
      call: (c) => c.luaEval('return 6*7'),
      route: 'POST /v1/lua/eval',
      reply: { result: '42' },
      want: { result: '42' },
      body: { code: 'return 6*7' },
    },
    { name: 'panes', call: (c) => c.panes(), route: 'GET /v1/panes', reply: [{ id: 'p1', cols: 80, rows: 24 }], want: [{ id: 'p1', cols: 80, rows: 24 }] },
    { name: 'panes null', call: (c) => c.panes(), route: 'GET /v1/panes', reply: null, want: [] },
    {
      name: 'openPane',
      call: (c) => c.openPane({ session_id: 's1', cols: 100, rows: 30 }),
      route: 'POST /v1/panes',
      reply: { pane_id: 'p9' },
      want: { pane_id: 'p9' },
      body: { session_id: 's1', cols: 100, rows: 30 },
    },
    {
      name: 'resizePane',
      call: (c) => c.resizePane('p1', 120, 40),
      route: 'POST /v1/panes/p1/resize',
      status: 204,
      want: undefined,
      body: { cols: 120, rows: 40 },
    },
    { name: 'closePane', call: (c) => c.closePane('p1'), route: 'DELETE /v1/panes/p1', status: 204, want: undefined },
  ];

  it.each(cases)('$name', async (tc) => {
    const method = tc.route.split(' ')[0];
    // the fake matches the encoded path (URL.pathname)
    const { c, calls } = client({ [tc.route]: { status: tc.status ?? 200, body: tc.reply } });
    expect(await tc.call(c)).toEqual(tc.want);
    const call = calls[0]!;
    expect(call.method).toBe(method);
    if (tc.body !== undefined) {
      expect(call.body).toEqual(tc.body);
      expect(call.headers['Content-Type']).toBe('application/json');
    }
    if (tc.query) {
      expect(Object.fromEntries(call.url.searchParams)).toEqual(tc.query);
    }
  });

  it('turns proto.Error answers into RpcError', async () => {
    const { c } = client({
      'POST /v1/confirm/old': { status: 410, body: { error: 'confirmação expirou', code: 'expired' } },
    });
    const err = await c.confirm('old', true).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RpcError);
    expect(isCode(err, 'expired')).toBe(true);
    expect((err as RpcError).status).toBe(410);
    expect((err as RpcError).message).toBe('confirmação expirou');
  });

  it('keeps non-JSON error bodies as the message', async () => {
    const { c } = client({ 'GET /v1/hello': { status: 502, text: 'bad gateway\n' } });
    const err = (await c.hello().catch((e: unknown) => e)) as RpcError;
    expect(err.code).toBe('');
    expect(err.message).toBe('bad gateway');
  });

  it('falls back to the status when the error body is empty', async () => {
    const { c } = client({ 'GET /v1/hello': { status: 500, text: '' } });
    const err = (await c.hello().catch((e: unknown) => e)) as RpcError;
    expect(err.message).toBe('HTTP 500');
    expect(isCode(new Error('x'), 'failed')).toBe(false);
  });

  it('JSON that is not proto.Error is kept as text', () => {
    const e = RpcError.from(400, '{"nope":1}');
    expect(e.message).toBe('{"nope":1}');
  });
});

describe('contextQuery', () => {
  it('drops empty fields', () => {
    expect(contextQuery({ picked: [], session_id: '' })).toEqual({
      session_id: undefined,
      picked: undefined,
      tab: undefined,
      todo_id: undefined,
      issue_key: undefined,
    });
    expect(contextQuery()).toMatchObject({ session_id: undefined });
  });
});

describe('URLs', () => {
  it('puts the token in SSE and WS URLs only', () => {
    const { c } = client();
    expect(c.url('/v1/hello')).toBe('http://127.0.0.1:4321/v1/hello');
    expect(c.url('/v1/events', { since: 5 }, true)).toBe('http://127.0.0.1:4321/v1/events?since=5&token=tok');
    expect(c.wsURL('/v1/panes/p1/ws')).toBe('ws://127.0.0.1:4321/v1/panes/p1/ws?token=tok');
  });

  it('maps https to wss', () => {
    const c = new NkClient({ url: 'https://h' });
    expect(c.wsURL('/v1/panes/a/ws')).toBe('wss://h/v1/panes/a/ws');
  });

  it('opens the event stream with ?token and ?since', () => {
    const { c } = client();
    const s = c.events({}, { since: 3 });
    expect(FakeEventSource.last.parsedURL.searchParams.get('since')).toBe('3');
    expect(FakeEventSource.last.parsedURL.searchParams.get('token')).toBe('tok');
    expect(FakeEventSource.last.parsedURL.pathname).toBe('/v1/events');
    s.close();
  });

  it('attaches a pane socket', () => {
    const { c } = client();
    const p = c.attachPane('p 1');
    expect(FakeWebSocket.last.url).toBe('ws://127.0.0.1:4321/v1/panes/p%201/ws?token=tok');
    expect(p.id).toBe('p 1');
  });

  it('fails clearly without EventSource/WebSocket', () => {
    const c = new NkClient(EP, { fetch: fakeFetch().fetch });
    // force the globals away for this client
    (c as unknown as { ES: undefined }).ES = undefined;
    (c as unknown as { WS: undefined }).WS = undefined;
    expect(() => c.events({})).toThrow(/EventSource/);
    expect(() => c.attachPane('x')).toThrow(/WebSocket/);
  });

  it('uses the global fetch by default', async () => {
    const orig = globalThis.fetch;
    const f = fakeFetch({ 'GET /v1/hello': { body: { app: 'nekuroddo' } } });
    globalThis.fetch = f.fetch as typeof fetch;
    try {
      const c = new NkClient(EP);
      expect((await c.hello()).app).toBe('nekuroddo');
    } finally {
      globalThis.fetch = orig;
    }
  });
});
