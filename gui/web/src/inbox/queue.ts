// The inbox cat's queue: which walkers are on the calçada, where each one
// walks to and when it gets there. Pure — every function takes `now` (ms)
// and returns a new queue, so the whole lifecycle (enter → walking →
// arrived → dismissed) is table-tested with a fake clock. The component
// only turns a queue into DOM and CSS transitions.

import { Attention, type MailCue, type Session } from '../client/types.ts';

/** What a walker brings: a session waiting on you, or a ✉. */
export type WalkerKind = 'blocked' | 'returned' | 'idle' | 'mail';

/** Where a walker is in its life (derived from the clock, never stored). */
export type Phase = 'queued' | 'waiting' | 'walking' | 'arrived';

export interface Walker {
  /** `att:<id>` or `mail:<id>`: one of each per session at most. */
  key: string;
  sessionId: string;
  label: string;
  kind: WalkerKind;
  /** Last activity (ms), the tie-break between equally urgent walkers. */
  activityAt: number;
  /** What it brings, as text: the ✉'s message or what the session needs. */
  text: string;
  /** Changes when the thing it brings changes (a dismissed walker comes
   * back only with a different signature). */
  sig: string;
  /** Mail walkers leave when the ✉ stops being fresh (ms), else null. */
  expiresAt: number | null;
  /** Walks from `from` (x px) at `startAt` to `to` at SPEED; startAt null =
   * not on the calçada (collapsed into the +N badge). */
  from: number;
  to: number;
  startAt: number | null;
}

export interface InboxQueue {
  walkers: Walker[];
  /** key → signature of what was dismissed (click, open, summon). */
  dismissed: Record<string, string>;
  /** Last peer.message text per recipient session. */
  texts: Record<string, string>;
  /** The walker inbox.summoned picked glows until `until`, then leaves. */
  highlight: { sessionId: string; until: number } | null;
  /** When the last walker started walking (single file: STAGGER apart). */
  lastStart: number;
}

/** Walking speed, px per ms (120 px/s: a calm oneko trot). */
export const SPEED = 0.12;
/** Gap between two walkers entering (ms): the single file. */
export const STAGGER = 450;
/** Where walkers come from: just off the left edge (px). */
export const START_X = -64;
/** Width of one walker (px), for layouts. */
export const CAT_W = 54;
/** Walkers on the calçada at once; the rest is a "+N" badge. */
export const MAX_VISIBLE = 5;
/** A ✉ walks in while the mail cue is this fresh (the TUI blinks 90 s). */
export const MAIL_FRESH_MS = 90_000;
/** How long the summoned walker glows before leaving (ms). */
export const HIGHLIGHT_MS = 1500;

/** The session fields the queue reads. */
export type SessionLike = Pick<Session, 'session_id' | 'label' | 'attention' | 'archived' | 'last_at' | 'needs'>;

/** Everything the queue derives its walkers from (snapshot.sessions, snapshot.mail). */
export interface InboxSource {
  sessions: readonly SessionLike[];
  mail: Readonly<Record<string, MailCue>>;
}

/** x (px) each visible walker should stop at, by key. */
export type Layout = (visible: readonly Walker[]) => Record<string, number>;

export function emptyQueue(): InboxQueue {
  return { walkers: [], dismissed: {}, texts: {}, highlight: null, lastStart: -Infinity };
}

const RANK: Record<WalkerKind, number> = { blocked: 0, returned: 1, mail: 2, idle: 3 };

/** Urgency order, as core.SortInbox: blocked > returned > (✉) > idle, then
 * the most recent activity, then the key (stable). */
export function compareUrgency(a: Walker, b: Walker): number {
  return RANK[a.kind] - RANK[b.kind] || b.activityAt - a.activityAt || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0);
}

export function ordered(q: InboxQueue): Walker[] {
  return [...q.walkers].sort(compareUrgency);
}

/** The walkers on the calçada, most urgent first. */
export function visible(q: InboxQueue): Walker[] {
  return ordered(q).slice(0, MAX_VISIBLE);
}

/** How many wait in the "+N" badge. */
export function overflow(q: InboxQueue): number {
  return Math.max(0, q.walkers.length - MAX_VISIBLE);
}

/** When a walker reaches `to` (ms), or null if it is not on the calçada. */
export function arriveAt(w: Walker): number | null {
  return w.startAt === null ? null : w.startAt + Math.abs(w.to - w.from) / SPEED;
}

/** Its x (px) at `now`: linear from `from` to `to`. */
export function xAt(w: Walker, now: number): number {
  if (w.startAt === null || now <= w.startAt) return w.from;
  const d = (now - w.startAt) * SPEED;
  const span = w.to - w.from;
  return Math.abs(span) <= d ? w.to : w.from + Math.sign(span) * d;
}

export function phaseOf(w: Walker, now: number): Phase {
  if (w.startAt === null) return 'queued';
  if (now < w.startAt) return 'waiting';
  return now < arriveAt(w)! ? 'walking' : 'arrived';
}

function ms(t: string | undefined): number {
  const v = t ? Date.parse(t) : NaN;
  return Number.isFinite(v) ? v : 0;
}

function kindOf(attention: number): WalkerKind | null {
  switch (attention) {
    case Attention.Blocked:
      return 'blocked';
    case Attention.Returned:
      return 'returned';
    default:
      return null; // none, and idle sessions don't send a cat
  }
}

/** The walkers the source asks for (without positions). */
function wanted(src: InboxSource, texts: Record<string, string>, now: number): Omit<Walker, 'from' | 'to' | 'startAt'>[] {
  const out: Omit<Walker, 'from' | 'to' | 'startAt'>[] = [];
  const labels = new Map<string, string>();
  for (const s of src.sessions) {
    labels.set(s.session_id, s.label);
    const kind = kindOf(s.attention);
    if (!kind || s.archived) continue;
    out.push({
      key: `att:${s.session_id}`,
      sessionId: s.session_id,
      label: s.label,
      kind,
      activityAt: ms(s.last_at),
      text: s.needs ?? '',
      sig: `${s.attention}|${s.last_at ?? ''}`,
      expiresAt: null,
    });
  }
  for (const [id, cue] of Object.entries(src.mail)) {
    if (cue.sent) continue; // the sender's ✉ is not news to you
    const at = ms(cue.at);
    if (now - at >= MAIL_FRESH_MS) continue;
    out.push({
      key: `mail:${id}`,
      sessionId: id,
      label: labels.get(id) ?? id.slice(0, 8),
      kind: 'mail',
      activityAt: at,
      text: texts[id] ?? '',
      sig: cue.at,
      expiresAt: at + MAIL_FRESH_MS,
    });
  }
  return out;
}

/** Brings the queue in line with the source at `now`: new items enter
 * (unless dismissed with the same signature), resolved ones leave (a
 * session whose attention went back to none, a stale ✉), the summoned
 * walker leaves once its glow is over, and the visible ones get their
 * stops from `layout`. */
export function step(q: InboxQueue, src: InboxSource, now: number, layout: Layout): InboxQueue {
  let out = expireHighlight(q, now);
  const want = wanted(src, out.texts, now);
  const byKey = new Map(out.walkers.map((w) => [w.key, w]));
  const wantKeys = new Set(want.map((w) => w.key));
  const walkers: Walker[] = [];
  for (const w of want) {
    const cur = byKey.get(w.key);
    if (cur) walkers.push({ ...cur, ...w });
    else if (out.dismissed[w.key] !== w.sig) walkers.push({ ...w, from: START_X, to: START_X, startAt: null });
  }
  // a dismissal lasts while the item does: once resolved it may come back
  const dismissed: Record<string, string> = {};
  for (const [k, sig] of Object.entries(out.dismissed)) {
    const w = want.find((x) => x.key === k);
    if (wantKeys.has(k) && w?.sig === sig) dismissed[k] = sig;
  }
  out = { ...out, walkers, dismissed };
  return place(out, now, layout);
}

/** Starts, retargets or parks walkers so the MAX_VISIBLE most urgent walk
 * to their stops, entering single file STAGGER apart, most urgent first. */
export function place(q: InboxQueue, now: number, layout: Layout): InboxQueue {
  const vis = visible(q);
  const targets = layout(vis);
  const onCalcada = new Set(vis.map((w) => w.key));
  let lastStart = q.lastStart;
  const next = new Map<string, Walker>();
  for (const w of vis) {
    const to = targets[w.key] ?? w.to;
    if (w.startAt === null) {
      const startAt = Math.max(now, lastStart + STAGGER);
      lastStart = startAt;
      next.set(w.key, { ...w, from: START_X, to, startAt });
    } else if (to !== w.to) {
      // not yet started: keep the slot in the file; walking: turn from here
      next.set(w.key, w.startAt > now ? { ...w, to } : { ...w, from: xAt(w, now), to, startAt: now });
    } else {
      next.set(w.key, w);
    }
  }
  let changed = lastStart !== q.lastStart;
  const walkers = q.walkers.map((w) => {
    const n = onCalcada.has(w.key) ? next.get(w.key)! : w.startAt === null ? w : { ...w, from: START_X, to: START_X, startAt: null };
    if (n !== w) changed = true;
    return n;
  });
  return changed ? { ...q, walkers, lastStart } : q;
}

function without(q: InboxQueue, gone: (w: Walker) => boolean): InboxQueue {
  const dismissed = { ...q.dismissed };
  const walkers = q.walkers.filter((w) => {
    if (!gone(w)) return true;
    dismissed[w.key] = w.sig;
    return false;
  });
  return walkers.length === q.walkers.length ? q : { ...q, walkers, dismissed };
}

/** Removes one walker (seen) until what it brings changes. */
export function dismiss(q: InboxQueue, key: string): InboxQueue {
  return without(q, (w) => w.key === key);
}

/** Removes every walker of a session (it was opened: all of it is seen). */
export function dismissSession(q: InboxQueue, sessionId: string): InboxQueue {
  const out = without(q, (w) => w.sessionId === sessionId);
  return out.highlight?.sessionId === sessionId ? { ...out, highlight: null } : out;
}

/** inbox.summoned picked this session: its walker glows for HIGHLIGHT_MS. */
export function summon(q: InboxQueue, sessionId: string, now: number): InboxQueue {
  return { ...q, highlight: { sessionId, until: now + HIGHLIGHT_MS } };
}

function expireHighlight(q: InboxQueue, now: number): InboxQueue {
  if (!q.highlight || now < q.highlight.until) return q;
  return { ...dismissSession(q, q.highlight.sessionId), highlight: null };
}

/** A peer.message: remember its text for the recipient's ✉ walker. */
export function notePeer(q: InboxQueue, to: string, from: string, text: string): InboxQueue {
  const t = from ? `${from}: ${text}` : text;
  const walkers = q.walkers.map((w) => (w.key === `mail:${to}` ? { ...w, text: t } : w));
  return { ...q, texts: { ...q.texts, [to]: t }, walkers };
}

/** The next moment the queue changes by itself (a walker starts or
 * arrives, a ✉ goes stale, a glow ends), or null: nothing to wait for. */
export function nextWake(q: InboxQueue, now: number): number | null {
  let next = Infinity;
  const consider = (t: number | null) => {
    if (t !== null && t > now && t < next) next = t;
  };
  for (const w of q.walkers) {
    consider(w.startAt);
    consider(arriveAt(w));
    consider(w.expiresAt);
  }
  consider(q.highlight?.until ?? null);
  return next === Infinity ? null : next;
}

/** Default stops: the visible sessions evenly spaced across `width`; a
 * second walker of the same session stands next to the first. With
 * `positionOf` (the scene's x of a session's cat/window, in px), walkers
 * stop under it instead. */
export function evenLayout(width: number, positionOf?: (sessionId: string) => number | undefined): Layout {
  return (vis) => {
    const ids: string[] = [];
    for (const w of vis) if (!ids.includes(w.sessionId)) ids.push(w.sessionId);
    const out: Record<string, number> = {};
    const seen = new Map<string, number>();
    const max = Math.max(8, width - CAT_W - 8);
    for (const w of vis) {
      const i = ids.indexOf(w.sessionId);
      const center = positionOf?.(w.sessionId) ?? ((i + 1) * width) / (ids.length + 1);
      const n = seen.get(w.sessionId) ?? 0;
      seen.set(w.sessionId, n + 1);
      out[w.key] = Math.round(Math.min(max, Math.max(8, center - CAT_W / 2 + n * CAT_W * 0.8)));
    }
    return out;
  };
}

/** The source as the store holds it. */
export function sourceOf(snapshot: { sessions: readonly SessionLike[]; mail?: Record<string, MailCue> | null } | null): InboxSource {
  return { sessions: snapshot?.sessions ?? [], mail: snapshot?.mail ?? {} };
}
