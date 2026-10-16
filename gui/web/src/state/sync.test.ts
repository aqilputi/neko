import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NkClient } from '../client/client.ts';
import type { NkEvent } from '../client/types.ts';
import { FakeEventSource, fakeFetch, type Route } from '../test/fakes.ts';
import { GW, MIG, hello, snapshot, theme, tree } from '../test/fixtures.ts';
import { createStore } from './store.ts';
import { startSync, type SyncOptions } from './sync.ts';

function routes(over: Record<string, Route> = {}): Record<string, Route> {
  return {
    'GET /v1/hello': { body: hello },
    'GET /v1/theme': { body: theme },
    'GET /v1/keymap': { body: [] },
    'GET /v1/commands': { body: [] },
    'GET /v1/panes': { body: [] },
    'GET /v1/snapshot': { body: snapshot() },
    'POST /v1/view/tree': { body: tree() },
    ...over,
  };
}

function setup(over: Record<string, Route> = {}, opts: SyncOptions = {}) {
  const f = fakeFetch(routes(over));
  const client = new NkClient({ url: 'http://h', token: 't' }, { fetch: f.fetch, EventSource: FakeEventSource });
  const store = createStore();
  const sync = startSync(client, store, { resyncDelayMs: 50, retryMs: 100, ...opts });
  const count = (r: string) => f.calls.filter((c) => `${c.method} ${c.url.pathname}` === r).length;
  return { f, store, sync, count, s: () => store.state.value };
}

const flush = () => vi.advanceTimersByTimeAsync(0);

beforeEach(() => {
  FakeEventSource.reset();
  vi.useFakeTimers();
});
afterEach(() => vi.useRealTimers());

describe('startSync', () => {
  it('bootstraps, opens the stream and loads snapshot + tree', async () => {
    const { s, count } = setup();
    expect(s().status).toBe('connecting');
    await flush();
    expect(s().hello).toEqual(hello);
    expect(s().theme?.name).toBe('catppuccin-mocha');
    expect(s().ready).toBe(true);
    expect(s().tree.rows).toHaveLength(4);
    expect(s().snapshot?.sessions).toHaveLength(2);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(count('GET /v1/snapshot')).toBe(1);
    FakeEventSource.last.open();
    expect(s().status).toBe('open');
    await flush();
    // the stream opening re-reads once (events before it are not replayed)
    expect(count('GET /v1/snapshot')).toBe(2);
    FakeEventSource.last.drop();
    FakeEventSource.last.open();
    await flush();
    expect(count('GET /v1/snapshot')).toBe(2);
  });

  it('sends the tree state back on every build', async () => {
    const { f } = setup();
    await flush();
    FakeEventSource.last.open();
    await flush();
    const bodies = f.calls.filter((c) => c.url.pathname === '/v1/view/tree').map((c) => c.body);
    expect(bodies[0]).toEqual({ state: {} });
    expect(bodies[1]).toEqual({ state: tree().state });
  });

  it('applies events and coalesces resyncs', async () => {
    const seen: NkEvent[] = [];
    const { s, count } = setup({}, { onEvent: (e) => seen.push(e) });
    await flush();
    const es = FakeEventSource.last;
    es.emitEvent(1, 'session.state', { id: GW, prev: 'busy', state: 'blocked' });
    es.emitEvent(2, 'snapshot.changed', { at: 'x', sessions: 7 });
    es.emitEvent(3, 'toast', { text: 'oi' });
    expect(seen.map((e) => e.seq)).toEqual([1, 2, 3]);
    expect(s().needsResync).toBe(true);
    expect(s().toasts.map((t) => t.text)).toEqual(['oi']);
    expect(count('GET /v1/snapshot')).toBe(1);
    await vi.advanceTimersByTimeAsync(50);
    expect(count('GET /v1/snapshot')).toBe(2);
    expect(s().needsResync).toBe(false);
    expect(s().lastSeq).toBe(3);
  });

  it('a gap triggers a resync', async () => {
    const { count } = setup();
    await flush();
    FakeEventSource.last.emit('gap', { since: 3, oldest: 900 });
    await vi.advanceTimersByTimeAsync(50);
    expect(count('GET /v1/snapshot')).toBe(2);
  });

  it('overlapping resyncs run once more, not twice', async () => {
    let release!: () => void;
    let calls = 0;
    const slow: Route = () => {
      calls++;
      return calls === 2 ? new Promise((ok) => (release = () => ok({ body: snapshot() }))) : { body: snapshot() };
    };
    const { sync, count } = setup({ 'GET /v1/snapshot': slow });
    await flush();
    const a = sync.resync();
    const b = sync.resync();
    const c = sync.resync();
    expect(a).toBe(b);
    expect(b).toBe(c);
    await flush();
    release();
    await a;
    await flush();
    expect(count('GET /v1/snapshot')).toBe(3);
  });

  it('retries a failed bootstrap', async () => {
    let fail = true;
    const { s } = setup({
      'GET /v1/hello': () => (fail ? { status: 401, body: { error: 'token inválido', code: 'unauthorized' } } : { body: hello }),
    });
    await flush();
    expect(s().status).toBe('error');
    expect(s().error).toBe('token inválido');
    expect(FakeEventSource.instances).toHaveLength(0);
    fail = false;
    await vi.advanceTimersByTimeAsync(100);
    expect(s().error).toBeNull();
    expect(s().ready).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it('keeps the error of a failed resync', async () => {
    const { s } = setup({ 'GET /v1/snapshot': { status: 500, text: 'boom' } });
    await flush();
    expect(s().error).toBe('boom');
    expect(s().ready).toBe(false);
  });

  it('refreshTree patches the view state and rebuilds', async () => {
    const { s, sync, f } = setup();
    await flush();
    await sync.refreshTree({ tab: 'inbox' });
    const last = f.calls.filter((c) => c.url.pathname === '/v1/view/tree').at(-1)!;
    expect(last.body).toEqual({ state: { ...tree().state, tab: 'inbox' } });
    expect(s().tree.rows).toHaveLength(4);
    await sync.refreshTree();
  });

  it('refreshTree reports errors', async () => {
    let fail = false;
    const { s, sync } = setup({ 'POST /v1/view/tree': () => (fail ? { status: 400, body: { error: 'tab', code: 'bad_request' } } : { body: tree() }) });
    await flush();
    fail = true;
    await sync.refreshTree({ tab: 'nope' as never });
    expect(s().error).toBe('tab');
  });

  it('run sends the cursor selection and applies the result', async () => {
    const { s, sync, f } = setup({
      'POST /v1/commands/session.remove': { body: { pending: { token: 'tk', question: 'remover?', expires_at: 'x' } } },
      'POST /v1/confirm/tk': { body: { toast: 'removida' } },
      // like the server, keep the cursor the client sent
      'POST /v1/view/tree': (c) => ({
        body: { ...tree(), state: { ...tree().state, ...(c.body as { state: object }).state } },
      }),
    });
    await flush();
    await sync.refreshTree({ cursor_key: `sess:${MIG}` });
    const r = await sync.run('session.remove');
    expect(r.pending?.token).toBe('tk');
    expect(f.calls.at(-1)!.body).toEqual({ context: { session_id: MIG, tab: 'sessions' } });
    expect(s().pending.map((p) => p.token)).toEqual(['tk']);
    await sync.confirm('tk', true);
    expect(s().toasts.map((t) => t.text)).toEqual(['removida']);
  });

  it('run without a selection sends an empty context', async () => {
    const { sync, f } = setup({ 'POST /v1/commands/sessions.refresh': { body: {} }, 'POST /v1/view/tree': { body: { rows: [], state: {}, inbox: [], hidden: 0, archived: 0 } } });
    await flush();
    await sync.run('sessions.refresh', { x: '1' });
    expect(f.calls.at(-1)!.body).toEqual({ args: { x: '1' }, context: {} });
  });

  it('stop closes the stream and cancels timers', async () => {
    const { s, sync, count } = setup();
    await flush();
    FakeEventSource.last.emitEvent(1, 'snapshot.changed', {});
    sync.stop();
    expect(FakeEventSource.last.closed).toBe(true);
    expect(s().status).toBe('closed');
    await vi.advanceTimersByTimeAsync(1000);
    expect(count('GET /v1/snapshot')).toBe(1);
  });

  it('stop during a failed bootstrap stops retrying', async () => {
    const { sync, f } = setup({ 'GET /v1/hello': { status: 500, text: 'x' } });
    await flush();
    sync.stop();
    const n = f.calls.length;
    await vi.advanceTimersByTimeAsync(1000);
    expect(f.calls.length).toBe(n);
  });

  it('stop while bootstrapping ignores the answers', async () => {
    const { s, sync } = setup();
    sync.stop();
    await flush();
    expect(s().hello).toBeNull();
    expect(FakeEventSource.instances).toHaveLength(0);
  });
});
