import { describe, expect, it } from 'vitest';
import { createInboxHub, DEDUP_MS, feedInboxEvent, inboxHub, toInboxEvent, type InboxEvent } from './hub.ts';

describe('toInboxEvent', () => {
  it.each([
    [{ name: 'inbox.summoned', data: { session_id: 'a', label: 'A' } }, { kind: 'summoned', sessionId: 'a', label: 'A' }],
    [{ name: 'inbox.summoned', data: { session_id: '' } }, null],
    [{ name: 'inbox.summoned' }, null],
    [{ name: 'peer.message', data: { to: 'b', from: 'gw', text: 'oi' } }, { kind: 'peer', to: 'b', from: 'gw', text: 'oi' }],
    [{ name: 'peer.message', data: { to: 'b' } }, { kind: 'peer', to: 'b', from: '', text: '' }],
    [{ name: 'peer.message', data: { from: 'x' } }, null],
    [{ name: 'peer.message', data: 3 }, null],
    [{ name: 'session.state', data: { id: 'a' } }, null],
  ])('%j', (ev, want) => {
    expect(toInboxEvent(ev)).toEqual(want);
  });
});

describe('hub', () => {
  it('delivers to subscribers until they unsubscribe', () => {
    const hub = createInboxHub(() => 0);
    const got: InboxEvent[] = [];
    const off = hub.subscribe((e) => got.push(e));
    hub.feed({ name: 'peer.message', data: { to: 'b', from: 'a', text: 't' } });
    hub.feed({ name: 'toast', data: {} });
    off();
    hub.feed({ name: 'peer.message', data: { to: 'b', from: 'a', text: 't' } });
    expect(got).toEqual([{ kind: 'peer', to: 'b', from: 'a', text: 't' }]);
  });

  it('a summon seen via Result and via the bus is delivered once', () => {
    let now = 1000;
    const hub = createInboxHub(() => now);
    const got: string[] = [];
    hub.subscribe((e) => e.kind === 'summoned' && got.push(e.sessionId));
    hub.summoned({ session_id: 'a', label: 'A' });
    now += 10;
    hub.feed({ name: 'inbox.summoned', data: { session_id: 'a', label: 'A' } });
    now += DEDUP_MS;
    hub.feed({ name: 'inbox.summoned', data: { session_id: 'a', label: 'A' } });
    hub.feed({ name: 'inbox.summoned', data: { session_id: 'b', label: 'B' } });
    hub.summoned({ session_id: '', label: '' });
    expect(got).toEqual(['a', 'a', 'b']);
  });

  it('feedInboxEvent feeds the app hub', () => {
    const got: InboxEvent[] = [];
    const off = inboxHub.subscribe((e) => got.push(e));
    feedInboxEvent({ seq: 1, at: '', name: 'peer.message', data: { to: 'z', from: 'y', text: 'x' } });
    off();
    expect(got).toHaveLength(1);
  });
});
