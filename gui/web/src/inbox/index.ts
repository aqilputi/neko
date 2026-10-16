// WP-6.4 — the inbox cat. A pure queue (queue.ts) over snapshot.sessions
// (attention blocked/returned) and snapshot.mail (✉), walked along the
// calçada by InboxWalk; shift+space → sync.run('inbox.summon').
//
// Wiring (App/main): render <InboxWalk store sync onOpenSession />, call
// installInboxKeys(window, sync), and pass `onEvent: feedInboxEvent` to
// startSync so peer.message / inbox.summoned reach the cat (inbox.summoned
// must be among the EventSource names the stream listens to).
export { InboxWalk, type InboxWalkProps } from './InboxWalk.tsx';
export { installInboxKeys, isSummonKey } from './keys.ts';
export { createInboxHub, feedInboxEvent, inboxHub, EV_INBOX_SUMMONED, type InboxHub, type InboxSummoned } from './hub.ts';
