import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeEventSource } from '../test/fakes.ts';
import { EventStream, type EventStreamHandlers, type StreamStatus } from './events.ts';
import type { Gap, NkEvent } from './types.ts';

function urlFor(since: number | undefined): string {
  return since === undefined ? 'http://h/v1/events' : `http://h/v1/events?since=${since}`;
}

function setup(opts: ConstructorParameters<typeof EventStream>[3] = {}) {
  const events: NkEvent[] = [];
  const resyncs: Array<[string, Gap | undefined]> = [];
  const statuses: StreamStatus[] = [];
  const handlers: EventStreamHandlers = {
    onEvent: (e) => events.push(e),
    onResync: (r, g) => resyncs.push([r, g]),
    onStatus: (s) => statuses.push(s),
  };
  const s = new EventStream(urlFor, FakeEventSource, handlers, opts);
  return { s, events, resyncs, statuses };
}

beforeEach(() => {
  FakeEventSource.reset();
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe('EventStream', () => {
  it('delivers named events in order and tracks the last seq', () => {
    const { s, events, statuses } = setup();
    const es = FakeEventSource.last;
    expect(es.url).toBe('http://h/v1/events');
    expect(s.status).toBe('connecting');
    es.open();
    es.emitEvent(1, 'session.state', { id: 'a', prev: 'idle', state: 'busy' });
    es.emitEvent(2, 'toast', { text: 'oi' });
    expect(events.map((e) => [e.seq, e.name])).toEqual([
      [1, 'session.state'],
      [2, 'toast'],
    ]);
    expect(s.lastSeq).toBe(2);
    expect(statuses).toEqual(['open']);
  });

  it('drops replayed events (seq ≤ last)', () => {
    const { events } = setup({ since: 5 });
    const es = FakeEventSource.last;
    expect(es.url).toBe('http://h/v1/events?since=5');
    es.emitEvent(5, 'toast', {});
    es.emitEvent(6, 'toast', {});
    es.emitEvent(6, 'toast', {});
    expect(events.map((e) => e.seq)).toEqual([6]);
  });

  it('ignores malformed data and takes seq from the SSE id when missing', () => {
    const { events } = setup();
    const es = FakeEventSource.last;
    es.emit('toast', 'not json');
    es.emit('toast', { nope: true });
    es.emit('toast', { name: 'toast', data: { text: 'x' } }, '9');
    es.emit('toast', { name: 'toast' }, 'abc');
    expect(events.map((e) => e.seq)).toEqual([9]);
  });

  it('listens only to the names asked for', () => {
    const { events } = setup({ names: ['custom.thing'] });
    const es = FakeEventSource.last;
    es.emitEvent(1, 'toast', {});
    es.emitEvent(2, 'custom.thing', {});
    expect(events.map((e) => e.name)).toEqual(['custom.thing']);
  });

  it('turns gap into a resync and accepts a restarted numbering', () => {
    const { events, resyncs } = setup();
    const es = FakeEventSource.last;
    es.open();
    es.emitEvent(40, 'toast', {});
    es.emit('gap', { since: 40, oldest: 950 });
    es.emitEvent(3, 'toast', {});
    expect(resyncs).toEqual([['gap', { since: 40, oldest: 950 }]]);
    expect(events.map((e) => e.seq)).toEqual([40, 3]);
  });

  it('a gap without data still resyncs', () => {
    const { resyncs } = setup({ since: 7 });
    FakeEventSource.last.emit('gap', 'x');
    expect(resyncs).toEqual([['gap', { since: 7, oldest: 0 }]]);
  });

  it('lets the browser retry a dropped connection', () => {
    const { statuses } = setup();
    const es = FakeEventSource.last;
    es.open();
    es.drop();
    expect(statuses).toEqual(['open', 'reconnecting']);
    vi.advanceTimersByTime(60_000);
    expect(FakeEventSource.instances).toHaveLength(1);
    es.open();
    expect(statuses).toEqual(['open', 'reconnecting', 'open']);
  });

  it('reopens with ?since after a fatal error, with backoff', () => {
    const { s, resyncs, statuses } = setup({ retryMs: 100, maxRetryMs: 300 });
    const first = FakeEventSource.last;
    first.open();
    first.emitEvent(12, 'toast', {});
    first.fail();
    expect(first.closed).toBe(true);
    expect(statuses.at(-1)).toBe('reconnecting');
    vi.advanceTimersByTime(99);
    expect(FakeEventSource.instances).toHaveLength(1);
    vi.advanceTimersByTime(1);
    const second = FakeEventSource.last;
    expect(second.url).toBe('http://h/v1/events?since=12');
    // the old source is ignored from now on
    first.emitEvent(13, 'toast', {});
    expect(s.lastSeq).toBe(12);
    second.fail();
    vi.advanceTimersByTime(199);
    expect(FakeEventSource.instances).toHaveLength(2);
    vi.advanceTimersByTime(1);
    const third = FakeEventSource.last;
    third.fail();
    vi.advanceTimersByTime(300); // capped
    expect(FakeEventSource.instances).toHaveLength(4);
    FakeEventSource.last.open();
    expect(s.status).toBe('open');
    expect(resyncs).toEqual([]); // resumable: the server replays or sends gap
    // a successful open resets the backoff
    FakeEventSource.last.fail();
    vi.advanceTimersByTime(100);
    expect(FakeEventSource.instances).toHaveLength(5);
  });

  it('resyncs after a hard reconnect with nothing to resume from', () => {
    const { resyncs } = setup({ retryMs: 10 });
    FakeEventSource.last.fail();
    vi.advanceTimersByTime(10);
    expect(FakeEventSource.last.url).toBe('http://h/v1/events');
    FakeEventSource.last.open();
    expect(resyncs).toEqual([['reconnect', undefined]]);
  });

  it('close stops everything, including a pending reconnect', () => {
    const { s, statuses } = setup({ retryMs: 10 });
    const es = FakeEventSource.last;
    es.fail();
    s.close();
    s.close();
    vi.advanceTimersByTime(1000);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(statuses).toEqual(['reconnecting', 'closed']);
    es.onerror?.(new Event('error')); // late callbacks are ignored
  });

  it('close while open closes the source; late errors are ignored', () => {
    const { s } = setup();
    const es = FakeEventSource.last;
    es.open();
    s.close();
    expect(es.closed).toBe(true);
    es.fail();
    es.onopen?.(new Event('open'));
    expect(s.status).toBe('closed');
  });
});
