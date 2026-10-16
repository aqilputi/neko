// The bus events the inbox cat needs that the store does not keep:
// peer.message (the ✉'s text) and inbox.summoned (which session to open).
// The app feeds them in with `feedInboxEvent` (startSync's onEvent); the
// shift+space handler feeds the summon's own Result too, so the cat reacts
// even when the stream does not carry inbox.summoned. A summon of the same
// session seen twice within DEDUP_MS (Result + event) is delivered once.

import type { NkEvent, PeerMessage } from '../client/types.ts';

export const EV_INBOX_SUMMONED = 'inbox.summoned';
export const EV_PEER_MESSAGE = 'peer.message';

/** app.InboxSummoned: inbox.summon's Data and the event's payload. */
export interface InboxSummoned {
  session_id: string;
  label: string;
}

export type InboxEvent =
  | { kind: 'summoned'; sessionId: string; label: string }
  | { kind: 'peer'; to: string; from: string; text: string };

export const DEDUP_MS = 2000;

export function isSummoned(d: unknown): d is InboxSummoned {
  return !!d && typeof d === 'object' && typeof (d as InboxSummoned).session_id === 'string' && (d as InboxSummoned).session_id !== '';
}

/** A bus event as an InboxEvent, or null if the cat does not care. */
export function toInboxEvent(ev: Pick<NkEvent, 'name' | 'data'>): InboxEvent | null {
  if (ev.name === EV_INBOX_SUMMONED && isSummoned(ev.data)) {
    return { kind: 'summoned', sessionId: ev.data.session_id, label: ev.data.label ?? '' };
  }
  if (ev.name === EV_PEER_MESSAGE && ev.data && typeof ev.data === 'object') {
    const p = ev.data as PeerMessage;
    if (typeof p.to === 'string' && p.to) return { kind: 'peer', to: p.to, from: p.from ?? '', text: p.text ?? '' };
  }
  return null;
}

export interface InboxHub {
  /** Feeds a bus event (anything else is ignored). */
  feed(ev: Pick<NkEvent, 'name' | 'data'>): void;
  /** Feeds a summon pick directly (inbox.summon's Result.data). */
  summoned(pick: InboxSummoned): void;
  subscribe(fn: (e: InboxEvent) => void): () => void;
}

export function createInboxHub(now: () => number = Date.now): InboxHub {
  const subs = new Set<(e: InboxEvent) => void>();
  let last: { id: string; at: number } | null = null;
  const emit = (e: InboxEvent) => {
    if (e.kind === 'summoned') {
      const t = now();
      if (last && last.id === e.sessionId && t - last.at < DEDUP_MS) return;
      last = { id: e.sessionId, at: t };
    }
    for (const fn of [...subs]) fn(e);
  };
  return {
    feed(ev) {
      const e = toInboxEvent(ev);
      if (e) emit(e);
    },
    summoned(pick) {
      if (isSummoned(pick)) emit({ kind: 'summoned', sessionId: pick.session_id, label: pick.label ?? '' });
    },
    subscribe(fn) {
      subs.add(fn);
      return () => subs.delete(fn);
    },
  };
}

/** The app's hub: InboxWalk listens to it, installInboxKeys feeds it. */
export const inboxHub: InboxHub = createInboxHub();

/** Pass as startSync's onEvent (or call from it). */
export function feedInboxEvent(ev: NkEvent): void {
  inboxHub.feed(ev);
}
