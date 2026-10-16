import { beforeEach, describe, expect, it } from 'vitest';
import { createClient } from '../client/client.ts';
import { FakeWebSocket, fakeFetch, type Route } from '../test/fakes.ts';
import { FakeView } from './fakeview.ts';
import { TermSession, type TermHandlers } from './term.ts';

const flush = () => new Promise((r) => setTimeout(r, 0));

function rig(routes: Record<string, Route> = {}) {
  const f = fakeFetch({
    'POST /v1/panes': { body: { pane_id: 'p1' } },
    'DELETE /v1/panes/p1': { status: 204 },
    ...routes,
  });
  const client = createClient({ url: 'http://nk', token: 't' }, { fetch: f.fetch, WebSocket: FakeWebSocket });
  const ev: string[] = [];
  const h: TermHandlers = {
    onPane: (p) => ev.push(`pane:${p}`),
    onTitle: (t) => ev.push(`title:${t}`),
    onExit: (e) => ev.push(`exit:${e ?? ''}`),
    onError: (m) => ev.push(`error:${m}`),
  };
  const s = new TermSession('sid', client, h);
  return { f, s, ev };
}

beforeEach(() => FakeWebSocket.reset());

describe('TermSession', () => {
  it('opens the pane with the default size and attaches', async () => {
    const { f, s, ev } = rig();
    await s.start();
    expect(f.calls[0]).toMatchObject({ method: 'POST', body: { session_id: 'sid', cols: 100, rows: 30 } });
    expect(ev).toEqual(['pane:p1']);
    expect(s.paneId).toBe('p1');
    expect(FakeWebSocket.last.url).toBe('ws://nk/v1/panes/p1/ws?token=t');
  });

  it('buffers output until the view mounts, then streams', async () => {
    const { s } = rig();
    await s.start();
    const ws = FakeWebSocket.last;
    ws.open();
    ws.receive(new TextEncoder().encode('✻ Claude Code\r\n'));
    expect(s.text()).toBe('✻ Claude Code\r\n');
    const v = new FakeView();
    s.mount(v);
    expect(v.written).toEqual(['✻ Claude Code\r\n']);
    ws.receive(new TextEncoder().encode('> '));
    expect(s.text()).toBe('✻ Claude Code\r\n> ');
    // the PTY learns the view's size
    expect(ws.controls).toEqual([{ type: 'resize', cols: 90, rows: 25 }]);
  });

  it('input, binary input, resize and titles', async () => {
    const { s, ev } = rig();
    await s.start();
    const ws = FakeWebSocket.last;
    ws.open();
    const v = new FakeView();
    s.mount(v);
    v.type('oi\r');
    v.binary('\x1b\xff');
    v.resize(120, 40);
    v.title('claude · api');
    ws.receive(JSON.stringify({ type: 'title', text: 'do servidor' }));
    ws.receive(JSON.stringify({ type: 'title', text: '' }));
    v.title('');
    expect(ws.texts[0]).toBe('oi\r');
    expect([...(ws.sent[2] as Uint8Array)]).toEqual([0x1b, 0xff]);
    expect(ws.controls.at(-1)).toEqual({ type: 'resize', cols: 120, rows: 40 });
    expect(ev.slice(1)).toEqual(['title:claude · api', 'title:do servidor']);
  });

  it('mounting a view while connecting queues its size', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const { s } = rig({
      'POST /v1/panes': async () => {
        await gate;
        return { body: { pane_id: 'p1' } };
      },
    });
    const v = new FakeView();
    s.mount(v);
    const started = s.start();
    release();
    await started;
    FakeWebSocket.last.open();
    expect(FakeWebSocket.last.controls).toEqual([{ type: 'resize', cols: 90, rows: 25 }]);
  });

  it('remounting drops the old listeners', async () => {
    const { s } = rig();
    await s.start();
    const a = new FakeView();
    s.mount(a);
    expect(a.listeners).toBe(4);
    const b = new FakeView();
    s.mount(b);
    expect(a.listeners).toBe(0);
    s.unmount();
    expect(b.listeners).toBe(0);
    expect(s.mounted).toBeUndefined();
  });

  it('exit reports and a later close does not kill', async () => {
    const { f, s, ev } = rig();
    await s.start();
    const ws = FakeWebSocket.last;
    ws.receive(JSON.stringify({ type: 'exit', err: 'boom' }));
    ws.serverClose(1000);
    expect(ev.at(-1)).toBe('exit:boom');
    s.close(true);
    await flush();
    expect(f.calls.some((c) => c.method === 'DELETE')).toBe(false);
  });

  it('close kills the pane once', async () => {
    const { f, s } = rig();
    await s.start();
    s.close(true);
    s.close(true);
    await flush();
    expect(f.calls.filter((c) => c.method === 'DELETE')).toHaveLength(1);
    expect(FakeWebSocket.last.closeCalls).toHaveLength(1);
  });

  it('close without kill keeps the pane', async () => {
    const { f, s } = rig();
    await s.start();
    s.close(false);
    await flush();
    expect(f.calls.some((c) => c.method === 'DELETE')).toBe(false);
  });

  it('1013 reconnects once, then a drop is an error', async () => {
    const { s, ev } = rig();
    await s.start();
    FakeWebSocket.last.serverClose(1013);
    expect(FakeWebSocket.instances).toHaveLength(2);
    FakeWebSocket.last.serverClose(1013);
    expect(FakeWebSocket.instances).toHaveLength(2);
    expect(ev.at(-1)).toBe('error:a conexão com o painel caiu');
  });

  it('a close after we closed is quiet', async () => {
    const { s, ev } = rig();
    await s.start();
    s.close(false);
    FakeWebSocket.last.serverClose(1006);
    expect(ev).toEqual(['pane:p1']);
  });

  it('open failures are errors', async () => {
    const { s, ev } = rig({ 'POST /v1/panes': { status: 404, body: { error: 'sessão desconhecida', code: 'not_found' } } });
    await s.start();
    expect(ev).toEqual(['error:sessão desconhecida']);
  });

  it('closed while opening: closes a fresh pane, keeps a reused one', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const fresh = rig({
      'POST /v1/panes': async () => {
        await gate;
        return { body: { pane_id: 'p1' } };
      },
    });
    const p = fresh.s.start();
    fresh.s.close(true);
    release();
    await p;
    await flush();
    expect(fresh.f.calls.filter((c) => c.method === 'DELETE')).toHaveLength(1);
    expect(fresh.ev).toEqual([]);

    const reused = rig({ 'POST /v1/panes': { body: { pane_id: 'p1', reused: true } } });
    const p2 = reused.s.start();
    reused.s.close(true);
    await p2;
    await flush();
    expect(reused.f.calls.filter((c) => c.method === 'DELETE')).toHaveLength(0);
  });

  it('an error after close is quiet', async () => {
    const { s, ev } = rig({ 'POST /v1/panes': { status: 500, body: { error: 'x', code: 'failed' } } });
    const p = s.start();
    s.close(false);
    await p;
    expect(ev).toEqual([]);
  });
});
