// Pure reducers: (state, input) → new state. No I/O, no clocks, no
// mutation of the input — everything the GUI shows is derived from these,
// so they are table-tested without a server.

import {
  Ev,
  type Binding,
  type CommandInfo,
  type ConfirmResolved,
  type Hello,
  type MailCueEvent,
  type NkEvent,
  type Notice,
  type Pane,
  type PaneEvent,
  type PaneResized,
  type PendingConfirmation,
  type Result,
  type SessionStateChange,
  type Snapshot,
  type Theme,
  type ToastData,
  type Tree,
  type ViewState,
} from '../client/types.ts';
import { MAX_TOASTS, type ConnStatus, type NkState, type Toast } from './model.ts';

export function applyHello(s: NkState, hello: Hello): NkState {
  return { ...s, hello };
}

export function applyTheme(s: NkState, theme: Theme): NkState {
  return { ...s, theme };
}

export function applyKeymap(s: NkState, keymap: Binding[] | null): NkState {
  return { ...s, keymap: keymap ?? [] };
}

export function applyCommands(s: NkState, commands: CommandInfo[] | null): NkState {
  return { ...s, commands: commands ?? [] };
}

export function applyPanes(s: NkState, panes: Pane[] | null): NkState {
  return { ...s, panes: panes ?? [] };
}

/** A fresh snapshot: replaces sessions, inbox, mail…, and the pending list. */
export function applySnapshot(s: NkState, snapshot: Snapshot): NkState {
  return {
    ...s,
    snapshot,
    pending: snapshot.pending ?? [],
    needsResync: false,
  };
}

/** A fresh tree from POST /v1/view/tree. */
export function applyTree(s: NkState, tree: Tree): NkState {
  return {
    ...s,
    tree: {
      rows: tree.rows ?? [],
      state: tree.state ?? {},
      inbox: tree.inbox ?? [],
      hidden: tree.hidden ?? 0,
      archived: tree.archived ?? 0,
    },
  };
}

/** Marks both snapshot and tree loaded (sync.ts calls it after a resync). */
export function markReady(s: NkState): NkState {
  return s.ready && s.error === null ? s : { ...s, ready: true, error: null };
}

/** Local change to the tree's view state (tab, filter, fold, cursor…)
 * before asking the server to rebuild; `reset_order` is request-only. */
export function patchViewState(s: NkState, patch: ViewState): NkState {
  return { ...s, tree: { ...s.tree, state: { ...s.tree.state, ...patch } } };
}

export function setStatus(s: NkState, status: ConnStatus): NkState {
  return s.status === status ? s : { ...s, status };
}

export function setError(s: NkState, error: string | null): NkState {
  return s.error === error ? s : { ...s, error };
}

/** The bus said something was lost (gap, hard reconnect): re-read. */
export function applyResync(s: NkState): NkState {
  return s.needsResync ? s : { ...s, needsResync: true };
}

export function pushToast(s: NkState, t: Toast): NkState {
  return { ...s, toasts: [...s.toasts, t].slice(-MAX_TOASTS) };
}

export function dismissToast(s: NkState, id: number): NkState {
  const toasts = s.toasts.filter((t) => t.id !== id);
  return toasts.length === s.toasts.length ? s : { ...s, toasts };
}

/** What a command Result means for the state: its toast and its pending
 * confirmation (the confirm.requested event may also bring it). */
export function applyResult(s: NkState, r: Result, id: number, at: string): NkState {
  let out = s;
  if (r.toast) out = pushToast(out, { id, text: r.toast, err: !!r.err, at });
  if (r.pending) out = addPending(out, r.pending);
  return out;
}

function addPending(s: NkState, p: PendingConfirmation): NkState {
  if (s.pending.some((x) => x.token === p.token)) return s;
  return { ...s, pending: [...s.pending, p] };
}

function updateSessionState(s: NkState, c: SessionStateChange): NkState {
  let out = s;
  if (s.snapshot) {
    const sessions = s.snapshot.sessions.map((x) => (x.session_id === c.id ? { ...x, state: c.state } : x));
    out = { ...out, snapshot: { ...s.snapshot, sessions } };
  }
  const rows = s.tree.rows.map((r) => (r.kind === 'sess' && r.session_id === c.id ? { ...r, state: c.state } : r));
  return { ...out, tree: { ...out.tree, rows } };
}

/** Events whose payload is only a hint: the snapshot/tree must be re-read. */
const RESYNC_EVENTS: ReadonlySet<string> = new Set([
  Ev.SnapshotChanged,
  Ev.SessionState, // attention, inbox order and the tree move with it
  Ev.UsageUpdated,
  Ev.GroupChanged,
  Ev.TodosChanged,
  Ev.JiraChanged,
  Ev.Dispatched,
  Ev.MailCue,
]);

/** Applies one bus event. Unknown events only advance lastSeq. */
export function applyEvent(s: NkState, ev: NkEvent): NkState {
  let out: NkState = { ...s, lastSeq: Math.max(s.lastSeq, ev.seq) };
  switch (ev.name) {
    case Ev.SessionState:
      out = updateSessionState(out, ev.data as SessionStateChange);
      break;
    case Ev.Toast: {
      const t = ev.data as ToastData;
      out = pushToast(out, { id: ev.seq, text: t.text, err: !!t.err, at: ev.at });
      break;
    }
    case Ev.Notice: {
      const n = ev.data as Notice;
      if (n.toast) out = pushToast(out, { id: ev.seq, text: n.toast, err: !!n.toast_err, at: ev.at });
      break;
    }
    case Ev.ConfirmRequested:
      out = addPending(out, ev.data as PendingConfirmation);
      break;
    case Ev.ConfirmResolved: {
      const tok = (ev.data as ConfirmResolved).token;
      out = { ...out, pending: out.pending.filter((p) => p.token !== tok) };
      break;
    }
    case Ev.PaneOpened: {
      const p = ev.data as PaneEvent;
      if (!out.panes.some((x) => x.id === p.id)) {
        const pane: Pane = { id: p.id, cols: 0, rows: 0 };
        if (p.session_id) pane.session_id = p.session_id;
        out = { ...out, panes: [...out.panes, pane] };
      }
      break;
    }
    case Ev.PaneClosed:
    case Ev.PaneExited: {
      const id = (ev.data as PaneEvent).id;
      out = { ...out, panes: out.panes.filter((p) => p.id !== id) };
      break;
    }
    case Ev.PaneResized: {
      const r = ev.data as PaneResized;
      out = { ...out, panes: out.panes.map((p) => (p.id === r.id ? { ...p, cols: r.cols, rows: r.rows } : p)) };
      break;
    }
    case Ev.MailCue: {
      const m = ev.data as MailCueEvent;
      if (out.snapshot) {
        out = { ...out, snapshot: { ...out.snapshot, mail: { ...out.snapshot.mail, [m.id]: { at: ev.at, sent: m.sent } } } };
      }
      break;
    }
  }
  if (RESYNC_EVENTS.has(ev.name)) out = applyResync(out);
  return out;
}

/** Applies events in order (handy for replays and tests). */
export function applyEvents(s: NkState, evs: readonly NkEvent[]): NkState {
  return evs.reduce(applyEvent, s);
}
