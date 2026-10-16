import { render } from 'preact';
import { act } from 'preact/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Session, Snapshot } from '../client/types.ts';
import { createStore, initialState, type Store } from '../state/index.ts';
import { installFakeDom, type FakeElement, type FakeWindow } from './fakedom.ts';
import { createInboxHub, type InboxHub } from './hub.ts';
import { InboxWalk } from './InboxWalk.tsx';
import { HIGHLIGHT_MS, MAX_VISIBLE, SPEED, START_X } from './queue.ts';

const T0 = Date.parse('2026-10-16T12:00:00Z');

function session(id: string, attention: number, extra: Partial<Session> = {}): Session {
  return {
    session_id: id,
    label: `L-${id}`,
    cwd: '/x',
    kind: 'background',
    state: attention === 1 ? 'blocked' : 'idle',
    attention,
    cost: 0,
    last_at: new Date(T0).toISOString(),
    ...extra,
  };
}

function snap(sessions: Session[], mail: Snapshot['mail'] = {}): Snapshot {
  return {
    sessions,
    inbox: [],
    groups: {},
    mail,
    usage: { limits: [] },
    todos: { groups: null },
    jira: { fetched_at: '', issues: null, cost_usd: 0 },
    pending: [],
    at: '',
    today_cost: 0,
  };
}

let dom: ReturnType<typeof installFakeDom>;
let win: FakeWindow;
let root: FakeElement;
let store: Store;
let hub: InboxHub;
let opened: string[];

function mount(sessions: Session[], mail: Snapshot['mail'] = {}) {
  store = createStore({ ...initialState(), snapshot: snap(sessions, mail) });
  act(() => {
    render(<InboxWalk store={store} sync={{}} hub={hub} onOpenSession={(id) => opened.push(id)} />, root as unknown as Element);
  });
}
const walkers = () => root.byTestId('inbox-walker');
const calcada = () => root.byTestId('inbox-walk')[0]!;
const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
const frames = (el: FakeElement) => el.findAll((e) => (e.attributes.class ?? '').includes('nk-frame'));

beforeEach(() => {
  vi.useFakeTimers({ now: T0 });
  dom = installFakeDom();
  win = dom.win;
  root = document.createElement('div') as unknown as FakeElement;
  hub = createInboxHub();
  opened = [];
});

afterEach(() => {
  act(() => render(null, root as unknown as Element));
  dom.restore();
  vi.useRealTimers();
});

describe('InboxWalk', () => {
  it('a blocked session walks in, legs moving, and stops still', () => {
    mount([session('a', 1), session('n', 0)]);
    const [w] = walkers();
    expect(walkers()).toHaveLength(1);
    expect(w!.attributes).toMatchObject({ 'data-session': 'a', 'data-kind': 'blocked', 'data-phase': 'walking' });
    expect(w!.attributes['aria-label']).toBe('L-a — te esperando');
    expect(frames(w!)).toHaveLength(4);
    // one CSS transition to its stop (even layout: centre of 800 px)
    const to = 400 - 27;
    expect(w!.styleProps.transform).toBe(`translateX(${to}px)`);
    expect(w!.styleProps.transition).toBe(`transform ${Math.round((to - START_X) / SPEED)}ms linear 0ms`);
    advance((to - START_X) / SPEED + 20);
    const [arrived] = walkers();
    expect(arrived!.attributes['data-phase']).toBe('arrived');
    expect(frames(arrived!)).toHaveLength(0); // static frame: nothing animates
    expect(vi.getTimerCount()).toBe(0); // and nothing is scheduled
  });

  it('click opens the session and the walker leaves', () => {
    mount([session('a', 1)]);
    act(() => walkers()[0]!.dispatch('click'));
    expect(opened).toEqual(['a']);
    expect(calcada().attributes['data-last-open']).toBe('a');
    expect(walkers()).toHaveLength(0);
  });

  it('leaves when the attention goes back to none', () => {
    mount([session('a', 1)]);
    act(() => store.update((s) => ({ ...s, snapshot: snap([session('a', 0)]) })));
    expect(walkers()).toHaveLength(0);
  });

  it('inbox.summoned: glows, opens, then leaves as seen', () => {
    mount([session('a', 1), session('b', 2)]);
    act(() => hub.feed({ name: 'inbox.summoned', data: { session_id: 'a', label: 'L-a' } }));
    expect(opened).toEqual(['a']);
    expect(calcada().attributes['data-summoned']).toBe('a');
    const glowing = walkers().filter((w) => w.attributes.class!.includes('nk-walker-summoned'));
    expect(glowing.map((w) => w.attributes['data-session'])).toEqual(['a']);
    advance(HIGHLIGHT_MS + 50);
    expect(walkers().map((w) => w.attributes['data-session'])).toEqual(['b']);
  });

  it('a ✉ walks in with the peer message as its text', () => {
    mount([session('b', 0)], { b: { at: new Date(T0).toISOString(), sent: false } });
    act(() => hub.feed({ name: 'peer.message', data: { to: 'b', from: 'gw', text: 'build verde' } }));
    const [w] = walkers();
    expect(w!.attributes['data-kind']).toBe('mail');
    expect(w!.attributes.title).toBe('L-b — mensagem: gw: build verde');
    expect(w!.textContent).toContain('✉');
  });

  it('more than MAX_VISIBLE: the rest is a +N badge', () => {
    const many = Array.from({ length: MAX_VISIBLE + 2 }, (_, i) => session(`s${i}`, 1));
    mount(many);
    expect(walkers()).toHaveLength(MAX_VISIBLE);
    expect(root.byTestId('inbox-more')[0]!.textContent).toBe('+2');
  });

  it('prefers-reduced-motion: a static badge, no walking cats', () => {
    win.reduced = true;
    mount([session('a', 1), session('b', 2)]);
    expect(root.findAll((e) => (e.attributes.class ?? '').includes('nk-walker '))).toHaveLength(0);
    const badge = root.byTestId('inbox-badge')[0]!;
    expect(badge.textContent).toContain('L-a');
    expect(walkers().map((w) => w.attributes['data-phase'])).toEqual(['arrived', 'arrived']);
    act(() => walkers()[1]!.dispatch('click'));
    expect(opened).toEqual(['b']);
    // and back to walking when the preference changes
    act(() => win.setReduced(false));
    expect(root.byTestId('inbox-badge')).toHaveLength(0);
    expect(walkers()).toHaveLength(1);
  });

  it('a resize retargets the walkers', () => {
    mount([session('a', 1)]);
    advance(10_000);
    win.innerWidth = 400;
    act(() => win.fire('resize'));
    advance(150);
    const [w] = walkers();
    expect(w!.styleProps.transform).toBe(`translateX(${200 - 27}px)`);
    expect(w!.attributes['data-phase']).toBe('walking');
  });

  it('no snapshot yet: an empty calçada', () => {
    store = createStore();
    act(() => render(<InboxWalk store={store} hub={hub} onOpenSession={() => {}} />, root as unknown as Element));
    expect(walkers()).toHaveLength(0);
    expect(calcada().attributes['data-last-open']).toBe('');
  });
});
