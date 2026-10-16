import { describe, expect, it } from 'vitest';
import type { MailCue } from '../client/types.ts';
import {
  arriveAt,
  CAT_W,
  compareUrgency,
  dismiss,
  dismissSession,
  emptyQueue,
  evenLayout,
  HIGHLIGHT_MS,
  MAIL_FRESH_MS,
  MAX_VISIBLE,
  nextWake,
  notePeer,
  ordered,
  overflow,
  phaseOf,
  place,
  sourceOf,
  SPEED,
  STAGGER,
  START_X,
  step,
  summon,
  visible,
  xAt,
  type InboxQueue,
  type InboxSource,
  type Layout,
  type SessionLike,
  type Walker,
} from './queue.ts';

const T0 = Date.parse('2026-10-16T12:00:00Z');
const iso = (ms: number) => new Date(ms).toISOString();

function sess(id: string, attention: number, lastAt = T0, extra: Partial<SessionLike> = {}): SessionLike {
  return { session_id: id, label: `L-${id}`, attention, last_at: iso(lastAt), ...extra };
}
function src(sessions: SessionLike[], mail: Record<string, MailCue> = {}): InboxSource {
  return { sessions, mail };
}
/** Every walker stops at 300 px, unless told otherwise. */
const at300: Layout = (vis) => Object.fromEntries(vis.map((w) => [w.key, 300]));
const keys = (q: InboxQueue) => ordered(q).map((w) => w.key);
const get = (q: InboxQueue, key: string) => q.walkers.find((w) => w.key === key)!;

describe('lifecycle with a fake clock', () => {
  it('enter → waiting/walking → arrived → dismissed when attention goes back to none', () => {
    let q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    const w = get(q, 'att:a');
    expect(w).toMatchObject({ kind: 'blocked', label: 'L-a', from: START_X, to: 300, startAt: T0 });
    expect(phaseOf(w, T0)).toBe('walking');
    const travel = (300 - START_X) / SPEED;
    expect(arriveAt(w)).toBe(T0 + travel);
    expect(xAt(w, T0 + travel / 2)).toBeCloseTo((START_X + 300) / 2);
    expect(phaseOf(w, T0 + travel - 1)).toBe('walking');
    expect(phaseOf(w, T0 + travel)).toBe('arrived');
    expect(xAt(w, T0 + travel + 5000)).toBe(300);
    // the next wake is the arrival; after it nothing is pending
    expect(nextWake(q, T0)).toBe(T0 + travel);
    expect(nextWake(q, T0 + travel)).toBeNull();
    // attention back to none: the walker leaves
    q = step(q, src([sess('a', 0)]), T0 + travel + 10, at300);
    expect(q.walkers).toEqual([]);
  });

  it('enters single file, STAGGER apart, most urgent first', () => {
    const q = step(
      emptyQueue(),
      src([sess('ret', 2, T0 + 50), sess('old', 1, T0 - 1000), sess('new', 1, T0)]),
      T0,
      at300,
    );
    const starts = ordered(q).map((w) => [w.key, w.startAt]);
    expect(starts).toEqual([
      ['att:new', T0],
      ['att:old', T0 + STAGGER],
      ['att:ret', T0 + 2 * STAGGER],
    ]);
    expect(phaseOf(get(q, 'att:old'), T0)).toBe('waiting');
    expect(xAt(get(q, 'att:old'), T0 + 100)).toBe(START_X);
    expect(nextWake(q, T0)).toBe(T0 + STAGGER);
  });

  it('a later item keeps the file going after the last start', () => {
    let q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    q = step(q, src([sess('a', 1), sess('b', 1, T0 - 5)]), T0 + 100, at300);
    expect(get(q, 'att:b').startAt).toBe(T0 + STAGGER);
    q = step(q, src([sess('a', 1), sess('b', 1, T0 - 5), sess('c', 2)]), T0 + 10_000, at300);
    expect(get(q, 'att:c').startAt).toBe(T0 + 10_000);
  });

  it('idle and none never send a cat; archived neither', () => {
    const q = step(emptyQueue(), src([sess('i', 3), sess('n', 0), sess('x', 1, T0, { archived: true })]), T0, at300);
    expect(q.walkers).toEqual([]);
  });

  it('keeps a walker when its kind changes (blocked → returned) without re-walking', () => {
    let q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    q = step(q, src([sess('a', 2, T0 + 9)]), T0 + 20_000, at300);
    expect(get(q, 'att:a')).toMatchObject({ kind: 'returned', startAt: T0, to: 300 });
  });
});

describe('dismissal', () => {
  it('click dismisses until the signature changes', () => {
    let q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    q = dismissSession(q, 'a');
    expect(q.walkers).toEqual([]);
    q = step(q, src([sess('a', 1)]), T0 + 1000, at300);
    expect(q.walkers).toEqual([]); // same block: stays dismissed
    q = step(q, src([sess('a', 1, T0 + 30_000)]), T0 + 30_000, at300);
    expect(keys(q)).toEqual(['att:a']); // blocked again later: comes back
  });

  it('the dismissal is forgotten once the item resolves', () => {
    let q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    q = dismiss(q, 'att:a');
    expect(q.dismissed).toEqual({ 'att:a': `1|${iso(T0)}` });
    q = step(q, src([sess('a', 0)]), T0 + 1, at300);
    expect(q.dismissed).toEqual({});
    q = step(q, src([sess('a', 1)]), T0 + 2, at300);
    expect(keys(q)).toEqual(['att:a']);
  });

  it('dismiss of an unknown key is a no-op', () => {
    const q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    expect(dismiss(q, 'att:zz')).toBe(q);
  });

  it('dismissSession takes the ✉ walker too', () => {
    let q = step(emptyQueue(), src([sess('a', 1)], { a: { at: iso(T0), sent: false } }), T0, at300);
    expect(keys(q).sort()).toEqual(['att:a', 'mail:a']);
    q = dismissSession(q, 'a');
    expect(q.walkers).toEqual([]);
  });
});

describe('✉ walkers', () => {
  it('a fresh received ✉ walks in; a sent one does not; a stale one leaves', () => {
    const mail = { b: { at: iso(T0), sent: false }, a: { at: iso(T0), sent: true } };
    let q = step(emptyQueue(), src([sess('a', 0), sess('b', 0)], mail), T0, at300);
    expect(keys(q)).toEqual(['mail:b']);
    expect(get(q, 'mail:b')).toMatchObject({ kind: 'mail', label: 'L-b', expiresAt: T0 + MAIL_FRESH_MS });
    expect(nextWake(q, T0 + 60_000)).toBe(T0 + MAIL_FRESH_MS);
    q = step(q, src([sess('a', 0), sess('b', 0)], mail), T0 + MAIL_FRESH_MS, at300);
    expect(q.walkers).toEqual([]);
  });

  it('an unknown recipient gets its short id as label', () => {
    const q = step(emptyQueue(), src([], { 'd3e0a001-xyz': { at: iso(T0), sent: false } }), T0, at300);
    expect(get(q, 'mail:d3e0a001-xyz').label).toBe('d3e0a001');
  });

  it('peer.message text sticks to the recipient walker, before or after it exists', () => {
    let q = notePeer(emptyQueue(), 'b', 'api-gateway', 'build verde');
    q = step(q, src([sess('b', 0)], { b: { at: iso(T0), sent: false } }), T0, at300);
    expect(get(q, 'mail:b').text).toBe('api-gateway: build verde');
    q = notePeer(q, 'b', '', 'oi');
    expect(get(q, 'mail:b').text).toBe('oi');
  });

  it('a new ✉ for the same session updates it in place', () => {
    let q = step(emptyQueue(), src([], { b: { at: iso(T0), sent: false } }), T0, at300);
    q = step(q, src([], { b: { at: iso(T0 + 20_000), sent: false } }), T0 + 20_000, at300);
    expect(q.walkers).toHaveLength(1);
    expect(get(q, 'mail:b')).toMatchObject({ sig: iso(T0 + 20_000), startAt: T0 });
  });
});

describe('urgency order (core.SortInbox: blocked > returned > idle, then most recent)', () => {
  const w = (key: string, kind: Walker['kind'], activityAt: number): Walker => ({
    key,
    sessionId: key,
    label: key,
    kind,
    activityAt,
    text: '',
    sig: '',
    expiresAt: null,
    from: 0,
    to: 0,
    startAt: null,
  });
  it.each([
    ['blocked before returned', w('a', 'returned', 9), w('b', 'blocked', 1), ['b', 'a']],
    ['returned before ✉', w('a', 'mail', 9), w('b', 'returned', 1), ['b', 'a']],
    ['✉ before idle', w('a', 'idle', 9), w('b', 'mail', 1), ['b', 'a']],
    ['tie: most recent first', w('a', 'blocked', 1), w('b', 'blocked', 9), ['b', 'a']],
    ['full tie: by key', w('b', 'blocked', 1), w('a', 'blocked', 1), ['a', 'b']],
  ])('%s', (_name, x, y, want) => {
    expect([x, y].sort(compareUrgency).map((v) => v.key)).toEqual(want);
  });
});

describe('max queue and the +N badge', () => {
  const many = (n: number) => Array.from({ length: n }, (_, i) => sess(`s${i}`, 1, T0 - i * 1000));
  it('only MAX_VISIBLE walk; the rest waits in +N', () => {
    const q = step(emptyQueue(), src(many(MAX_VISIBLE + 3)), T0, at300);
    expect(visible(q)).toHaveLength(MAX_VISIBLE);
    expect(overflow(q)).toBe(3);
    const hidden = ordered(q).slice(MAX_VISIBLE);
    expect(hidden.every((x) => x.startAt === null && phaseOf(x, T0) === 'queued')).toBe(true);
  });

  it('when a visible one leaves, the next one walks in', () => {
    let q = step(emptyQueue(), src(many(MAX_VISIBLE + 1)), T0, at300);
    const next = `att:s${MAX_VISIBLE}`;
    expect(get(q, next).startAt).toBeNull();
    q = dismissSession(q, 's0');
    q = step(q, src(many(MAX_VISIBLE + 1)), T0 + 10_000, at300);
    expect(get(q, next).startAt).toBe(T0 + 10_000);
    expect(overflow(q)).toBe(0);
  });

  it('a more urgent arrival pushes the least urgent back into +N', () => {
    let q = step(emptyQueue(), src(many(MAX_VISIBLE)), T0, at300);
    const last = `att:s${MAX_VISIBLE - 1}`;
    expect(get(q, last).startAt).not.toBeNull();
    q = step(q, src([...many(MAX_VISIBLE), sess('urgent', 1, T0 + 1)]), T0 + 5000, at300);
    expect(get(q, last).startAt).toBeNull();
    expect(get(q, 'att:urgent').startAt).toBe(T0 + 5000);
  });
});

describe('placing', () => {
  it('retargets a walking cat from where it is', () => {
    let q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    const mid = T0 + 1000;
    const x = xAt(get(q, 'att:a'), mid);
    q = place(q, mid, () => ({ 'att:a': 100 }));
    expect(get(q, 'att:a')).toMatchObject({ from: x, to: 100, startAt: mid });
  });

  it('retargets a waiting cat keeping its slot in the file', () => {
    let q = step(emptyQueue(), src([sess('a', 1), sess('b', 1, T0 - 1)]), T0, at300);
    q = place(q, T0 + 10, (vis) => Object.fromEntries(vis.map((w) => [w.key, 500])));
    expect(get(q, 'att:b')).toMatchObject({ from: START_X, to: 500, startAt: T0 + STAGGER });
  });

  it('same targets: same queue object (no re-render)', () => {
    const q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    expect(place(q, T0 + 5, at300)).toBe(q);
  });

  it('a missing target keeps the old one', () => {
    const q = step(emptyQueue(), src([sess('a', 1)]), T0, at300);
    expect(place(q, T0 + 5, () => ({}))).toBe(q);
  });
});

describe('summon', () => {
  it('glows for HIGHLIGHT_MS, then the session leaves as seen', () => {
    let q = step(emptyQueue(), src([sess('a', 1), sess('b', 1, T0 - 9)]), T0, at300);
    q = summon(q, 'a', T0 + 100);
    expect(q.highlight).toEqual({ sessionId: 'a', until: T0 + 100 + HIGHLIGHT_MS });
    expect(nextWake(q, T0 + 100)).toBeLessThanOrEqual(T0 + 100 + HIGHLIGHT_MS);
    q = step(q, src([sess('a', 1), sess('b', 1, T0 - 9)]), T0 + 100 + HIGHLIGHT_MS - 1, at300);
    expect(q.highlight).not.toBeNull();
    q = step(q, src([sess('a', 1), sess('b', 1, T0 - 9)]), T0 + 100 + HIGHLIGHT_MS, at300);
    expect(q.highlight).toBeNull();
    expect(keys(q)).toEqual(['att:b']);
  });

  it('dismissing the summoned session clears the glow', () => {
    let q = summon(step(emptyQueue(), src([sess('a', 1)]), T0, at300), 'a', T0);
    q = dismissSession(q, 'a');
    expect(q.highlight).toBeNull();
  });
});

describe('evenLayout', () => {
  const ws = (q: InboxQueue) => visible(q);
  it('spreads distinct sessions evenly and clamps to the width', () => {
    const q = step(emptyQueue(), src([sess('a', 1, T0 + 2), sess('b', 1, T0 + 1), sess('c', 1)]), T0, at300);
    const t = evenLayout(800)(ws(q));
    expect(t).toEqual({
      'att:a': Math.round(200 - CAT_W / 2),
      'att:b': Math.round(400 - CAT_W / 2),
      'att:c': Math.round(600 - CAT_W / 2),
    });
  });

  it('a second walker of a session stands beside the first', () => {
    const q = step(emptyQueue(), src([sess('a', 1)], { a: { at: iso(T0), sent: false } }), T0, at300);
    const t = evenLayout(800)(ws(q));
    expect(t['mail:a']! - t['att:a']!).toBe(Math.round(CAT_W * 0.8));
  });

  it('uses the scene position when given, clamped inside the calçada', () => {
    const q = step(emptyQueue(), src([sess('a', 1), sess('b', 1, T0 - 1), sess('c', 1, T0 - 2)]), T0, at300);
    const pos = (id: string) => ({ a: 100, b: 5000 })[id];
    const t = evenLayout(800, pos)(ws(q));
    expect(t['att:a']).toBe(100 - CAT_W / 2);
    expect(t['att:b']).toBe(800 - CAT_W - 8);
    expect(t['att:c']).toBe(Math.round(600 - CAT_W / 2)); // unknown: even slot
  });
});

describe('sourceOf', () => {
  it('reads sessions and mail, tolerating no snapshot', () => {
    expect(sourceOf(null)).toEqual({ sessions: [], mail: {} });
    expect(sourceOf({ sessions: [sess('a', 1)], mail: null })).toEqual({ sessions: [sess('a', 1)], mail: {} });
  });
});
