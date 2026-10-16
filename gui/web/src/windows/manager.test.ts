import { beforeEach, describe, expect, it } from 'vitest';
import { createClient } from '../client/client.ts';
import { createUi } from '../keymap/ui.ts';
import { applySnapshot, applyTree } from '../state/reducers.ts';
import { createStore } from '../state/store.ts';
import { FakeWebSocket, fakeFetch } from '../test/fakes.ts';
import { GW, MIG, snapshot, tree } from '../test/fixtures.ts';
import { FakeView } from './fakeview.ts';
import { find } from './layout.ts';
import { WindowManager } from './manager.ts';
import { TermSession } from './term.ts';

const flush = () => new Promise((r) => setTimeout(r, 0));

function rig() {
  let n = 0;
  const f = fakeFetch({
    'POST /v1/panes': () => ({ body: { pane_id: `p${++n}` } }),
    'DELETE /v1/panes/p1': { status: 204 },
    'DELETE /v1/panes/p2': { status: 204 },
  });
  const client = createClient({ url: 'http://nk' }, { fetch: f.fetch, WebSocket: FakeWebSocket });
  const store = createStore();
  store.update(applySnapshot, snapshot());
  store.update(applyTree, tree());
  const ui = createUi();
  const m = new WindowManager({ client, store, ui });
  m.setArea({ x: 300, y: 56, w: 1200, h: 700 });
  return { f, store, ui, m };
}

beforeEach(() => FakeWebSocket.reset());

describe('WindowManager', () => {
  it('open: window with the label, pane opened, cursor follows', async () => {
    const { m, store, f } = rig();
    m.open(GW, false);
    expect(m.state.value.wins.map((w) => [w.id, w.title, w.status])).toEqual([[GW, 'api-gateway', 'opening']]);
    expect(store.state.value.tree.state.cursor_key).toBe(`sess:${GW}`);
    await flush();
    expect(f.calls[0]!.body).toMatchObject({ session_id: GW });
    expect(find(m.state.value, GW)).toMatchObject({ status: 'open', paneId: 'p1' });
    expect(m.session(GW)).toBeInstanceOf(TermSession);
  });

  it('labels fall back to the tree row, then the id', () => {
    const { m, store } = rig();
    store.update((s) => ({ ...s, snapshot: { ...s.snapshot!, sessions: [] } }));
    m.open(MIG, false);
    m.open('ffffffff-1234', false);
    expect(m.state.value.wins.map((w) => w.title)).toEqual(['migração do banco', 'ffffffff']);
  });

  it('focus waits for the view to mount, then enters INSERT', async () => {
    const { m, ui } = rig();
    m.open(GW, true);
    expect(ui.mode.value).toBe('normal');
    const v = new FakeView();
    m.mount(GW, v);
    expect(v.focused).toBe(true);
    expect(ui.mode.value).toBe('insert');
    m.blur();
    expect(v.focused).toBe(false);
  });

  it('reopening raises without a second pane', async () => {
    const { m, f } = rig();
    m.open(GW, false);
    m.open(MIG, false);
    m.open(GW, false);
    await flush();
    expect(m.active).toBe(GW);
    expect(f.calls.filter((c) => c.method === 'POST')).toHaveLength(2);
  });

  it('w closes the active window and kills its pane; INSERT ends', async () => {
    const { m, f, ui } = rig();
    m.open(GW, true);
    await flush();
    m.mount(GW, new FakeView());
    expect(ui.mode.value).toBe('insert');
    expect(m.closeActive()).toBe(true);
    await flush();
    expect(m.state.value.wins).toEqual([]);
    expect(ui.mode.value).toBe('normal');
    expect(f.calls.some((c) => c.method === 'DELETE' && c.url.pathname === '/v1/panes/p1')).toBe(true);
    expect(m.closeActive()).toBe(false);
  });

  it('zoom, focus, nudge act on the active window; none → false', () => {
    const { m } = rig();
    expect(m.zoomActive()).toBe(false);
    expect(m.focusActive()).toBe(false);
    expect(m.nudgeActive(1, 1)).toBe(false);
    expect(m.focusTerm('nope')).toBe(false);
    m.open(GW, false);
    const before = find(m.state.value, GW)!;
    expect(m.nudgeActive(24, 0)).toBe(true);
    expect(find(m.state.value, GW)!.x).toBe(before.x + 24);
    expect(m.zoomActive()).toBe(true);
    expect(find(m.state.value, GW)!.zoomed).toBe(true);
    m.toggleZoom(GW);
    expect(find(m.state.value, GW)!.zoomed).toBe(false);
    expect(m.focusActive()).toBe(true);
  });

  it('move, resize and area changes stay inside the area', () => {
    const { m } = rig();
    m.open(GW, false);
    m.moveTo(GW, -500, 5000);
    expect(find(m.state.value, GW)).toMatchObject({ x: 300 });
    m.resize(GW, 5000, 100);
    expect(find(m.state.value, GW)).toMatchObject({ w: 1200, h: 180 });
    const s = m.state.value;
    m.setArea({ x: 300, y: 56, w: 1200, h: 700 });
    expect(m.state.value).toBe(s);
    m.setArea({ x: 0, y: 56, w: 600, h: 300 });
    expect(find(m.state.value, GW)).toMatchObject({ w: 600 });
  });

  it('the pane exiting closes the window with a toast, without DELETE', async () => {
    const { m, f, store } = rig();
    m.open(GW, false);
    await flush();
    FakeWebSocket.last.receive(JSON.stringify({ type: 'exit' }));
    expect(m.state.value.wins).toEqual([]);
    expect(store.state.value.toasts.at(-1)?.text).toBe('painel de api-gateway encerrado');
    await flush();
    expect(f.calls.some((c) => c.method === 'DELETE')).toBe(false);
  });

  it('exit with an error is an error toast', async () => {
    const { m, store } = rig();
    m.open(GW, false);
    await flush();
    FakeWebSocket.last.receive(JSON.stringify({ type: 'exit', err: 'sinal' }));
    expect(store.state.value.toasts.at(-1)).toMatchObject({ text: 'painel encerrado: sinal', err: true });
  });

  it('titles and errors show on the window', async () => {
    const { m } = rig();
    m.open(GW, false);
    await flush();
    FakeWebSocket.last.receive(JSON.stringify({ type: 'title', text: 'claude' }));
    expect(find(m.state.value, GW)!.subtitle).toBe('claude');
    FakeWebSocket.last.serverClose(1006);
    expect(find(m.state.value, GW)).toMatchObject({ status: 'error', error: 'a conexão com o painel caiu' });
    m.fail(GW, 'x');
    expect(find(m.state.value, GW)!.error).toBe('x');
  });

  it('pane.closed / pane.exited from the bus close the window', async () => {
    const { m } = rig();
    m.open(GW, false);
    await flush();
    m.onEvent({ seq: 1, at: '', name: 'session.state', data: { id: GW } });
    m.onEvent({ seq: 2, at: '', name: 'pane.closed', data: { id: 'other' } });
    expect(m.state.value.wins).toHaveLength(1);
    m.onEvent({ seq: 3, at: '', name: 'pane.exited', data: { id: 'p1' } });
    expect(m.state.value.wins).toHaveLength(0);
  });

  it('text reads the session screen; unmount keeps the window', async () => {
    const { m } = rig();
    expect(m.text(GW)).toBe('');
    m.open(GW, false);
    await flush();
    FakeWebSocket.last.receive(new TextEncoder().encode('Claude Code'));
    expect(m.text(GW)).toBe('Claude Code');
    const v = new FakeView();
    m.mount(GW, v);
    m.unmount(GW);
    expect(m.state.value.wins).toHaveLength(1);
    m.mount('nope', v); // unknown: ignored
  });

  it('syncLabels follows renames', () => {
    const { m, store } = rig();
    m.open(GW, false);
    const s = m.state.value;
    m.syncLabels();
    expect(m.state.value).toBe(s);
    store.update((st) => ({
      ...st,
      snapshot: { ...st.snapshot!, sessions: st.snapshot!.sessions.map((x) => (x.session_id === GW ? { ...x, label: 'novo' } : x)) },
    }));
    m.syncLabels();
    expect(find(m.state.value, GW)!.title).toBe('novo');
  });

  it('makeSession is used when given', () => {
    const { store, ui } = rig();
    const made: string[] = [];
    const client = { openPane: async () => ({ pane_id: 'x' }), attachPane: () => ({}) as never, closePane: async () => {} };
    const m = new WindowManager({
      client,
      store,
      ui,
      makeSession: (id, c, h) => {
        made.push(id);
        return new TermSession(id, c, { ...h, onPane: () => {} });
      },
    });
    m.open(GW, false);
    expect(made).toEqual([GW]);
  });
});
