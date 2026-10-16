// The GUI's state: everything the chrome, the scene, the inbox cat and the
// windows read. One immutable value, replaced by the pure reducers in
// reducers.ts and held in a signal by store.ts.

import type {
  Binding,
  CommandInfo,
  Hello,
  Pane,
  PendingConfirmation,
  Snapshot,
  Theme,
  TreeRow,
  ViewState,
} from '../client/types.ts';

/** Connection to the core, as the user should see it. */
export type ConnStatus = 'connecting' | 'open' | 'reconnecting' | 'closed' | 'error';

/** A message for the toast line (from `toast`, `notice` or a command Result). */
export interface Toast {
  id: number;
  text: string;
  err: boolean;
  at: string;
}

/** The tree as last built by POST /v1/view/tree (nulls normalised to []). */
export interface TreeView {
  rows: TreeRow[];
  /** What to send back on the next build (frozen order, folds, cursor…). */
  state: ViewState;
  inbox: string[];
  hidden: number;
  archived: number;
}

export interface NkState {
  status: ConnStatus;
  /** Last error worth showing (failed bootstrap/resync), or null. */
  error: string | null;
  hello: Hello | null;
  snapshot: Snapshot | null;
  tree: TreeView;
  /** True once the first snapshot and tree arrived. */
  ready: boolean;
  theme: Theme | null;
  keymap: Binding[];
  commands: CommandInfo[];
  panes: Pane[];
  /** Confirmations still open (snapshot.pending, then the confirm.* events). */
  pending: PendingConfirmation[];
  toasts: Toast[];
  /** Seq of the last bus event applied (0 before any). */
  lastSeq: number;
  /** The snapshot/tree are behind the bus: re-read them (sync.ts does). */
  needsResync: boolean;
}

/** How many toasts are kept (oldest dropped first). */
export const MAX_TOASTS = 5;

export function emptyTree(): TreeView {
  return { rows: [], state: {}, inbox: [], hidden: 0, archived: 0 };
}

export function initialState(): NkState {
  return {
    status: 'connecting',
    error: null,
    hello: null,
    snapshot: null,
    tree: emptyTree(),
    ready: false,
    theme: null,
    keymap: [],
    commands: [],
    panes: [],
    pending: [],
    toasts: [],
    lastSeq: 0,
    needsResync: false,
  };
}
