// Wire types of the core's protocol v1, hand-written from the Go DTOs in
// internal/rpc/proto (proto.go, view.go) and the core/app types they embed.
// Field names are exactly the JSON tags; fields tagged omitempty/omitzero
// in Go are optional here. Times are RFC 3339 strings. Within v1 changes
// are additive only: new fields arrive optional and unknown ones are
// ignored.

/** proto.Version: hello.protocol and the /v1 prefix. */
export const PROTOCOL_VERSION = 1;

/** Capabilities a server may announce in hello. */
export const Cap = {
  Panes: 'panes',
  Lua: 'lua',
  Demo: 'demo',
} as const;

/** Error codes in ErrorBody.code. */
export const Code = {
  BadRequest: 'bad_request',
  Unauthorized: 'unauthorized',
  NotFound: 'not_found',
  UnknownCommand: 'unknown_command',
  ClientCommand: 'client_command',
  Unavailable: 'unavailable',
  MissingArg: 'missing_arg',
  Failed: 'failed',
  UnknownToken: 'unknown_token',
  Expired: 'expired',
  NotImplemented: 'not_implemented',
} as const;
export type ErrorCode = (typeof Code)[keyof typeof Code];

/** proto.Error: the body of every non-2xx answer. */
export interface ErrorBody {
  error: string;
  code: string;
}

/** proto.Hello: GET /v1/hello. */
export interface Hello {
  app: string;
  version: string;
  protocol: number;
  capabilities: string[];
  pid: number;
}

/** proto.Context: the calling client's selection. */
export interface Context {
  session_id?: string;
  picked?: string[];
  tab?: string;
  todo_id?: number;
  issue_key?: string;
}

/** core.Arg. */
export interface Arg {
  name: string;
  type: 'string' | 'session' | 'dir' | 'tag' | 'int' | (string & {});
  prompt: string;
  required: boolean;
}

/** core.Command (no omitempty: every field is present). */
export interface Command {
  name: string;
  title: string;
  hint: string;
  keys: string[] | null;
  mode: 'normal' | 'insert' | 'input' | '*' | (string & {});
  args: Arg[] | null;
  scope: 'core' | 'client' | (string & {});
  confirm: boolean;
  source: string;
}

/** proto.CommandInfo: GET /v1/commands entry. */
export interface CommandInfo extends Command {
  available: boolean;
}

/** core.Help. */
export interface Help {
  key: string;
  desc: string;
}

/** core.Binding: GET /v1/keymap entry. */
export interface Binding {
  command: string;
  keys: string[] | null;
  mode: string;
  help: Help;
}

/** proto.RunRequest: body of POST /v1/commands/{name}. */
export interface RunRequest {
  args?: Record<string, string>;
  context: Context;
}

/** proto.ConfirmRequest. */
export interface ConfirmRequest {
  yes: boolean;
}

/** core.PendingConfirmation. */
export interface PendingConfirmation {
  token: string;
  question: string;
  expires_at: string;
}

/** core.ExecRequest. */
export interface ExecRequest {
  argv: string[];
  dir: string;
}

/** proto.Result: what a command (or a confirmation) hands back. */
export interface Result {
  toast?: string;
  err?: boolean;
  pending?: PendingConfirmation;
  exec?: ExecRequest;
  /** Command-specific (e.g. app.SessionEffect); may be ignored. */
  data?: unknown;
}

/** app.SessionEffect, the data of the session.* commands. */
export interface SessionEffect {
  close_panes?: string[];
  refresh?: boolean;
}

/** proto.Event: the data of one SSE message on GET /v1/events. */
export interface NkEvent<D = unknown> {
  seq: number;
  at: string;
  name: string;
  data?: D;
}

/** proto.Gap: data of the id-less `gap` SSE event. */
export interface Gap {
  since: number;
  oldest: number;
}

/** claude.Option. */
export interface QuestionOption {
  label: string;
  description: string;
}

/** claude.Question. */
export interface Question {
  question: string;
  options: QuestionOption[] | null;
}

/** claude.Child. */
export interface Child {
  id: string;
  href: string;
  kind: string;
  title: string;
}

/** proto.Sub: a session's subagent. */
export interface Sub {
  id: string;
  description?: string;
  type?: string;
  model?: string;
  last_text?: string;
  cost?: number;
  last_at?: string;
  done?: boolean;
}

/** store.Session.State values. */
export type SessionStateName =
  | 'busy'
  | 'blocked'
  | 'done'
  | 'stopped'
  | 'idle'
  | 'history'
  | (string & {});

/** store.Session.Attention: 0 none, 1 blocked, 2 returned, 3 idle. */
export const Attention = { None: 0, Blocked: 1, Returned: 2, Idle: 3 } as const;

/** proto.Session: store.Session on the wire. */
export interface Session {
  session_id: string;
  short_id?: string;
  label: string;
  name?: string;
  title?: string;
  custom_name?: string;
  cwd: string;
  context?: string;
  context_set?: boolean;
  kind: 'background' | 'interactive' | 'history' | (string & {});
  state: SessionStateName;
  attention: number;
  pid?: number;
  started?: string;
  last_at?: string;
  model?: string;
  cost: number;
  window_cost?: number;
  cost_today?: number;
  tokens?: number;
  first_user?: string;
  last_user?: string;
  last_text?: string;
  last_tool?: string;
  waiting_tool?: string;
  waiting_for?: string;
  detail?: string;
  needs?: string;
  suggested_reply?: string;
  questions?: Question[];
  links?: Child[];
  result?: string;
  subs?: Sub[];
  stale?: boolean;
  archived?: boolean;
  archived_at?: string;
  ended?: boolean;
  finished?: boolean;
}

/** proto.MailCue: the ✉ next to a session. */
export interface MailCue {
  at: string;
  sent: boolean;
}

/** proto.Limit. */
export interface Limit {
  name: string;
  pct: number;
  resets?: string;
}

/** proto.Usage. */
export interface Usage {
  limits: Limit[];
  at?: string;
  err?: string;
}

/** groups.Member. */
export interface GroupMember {
  session_id: string;
  name: string;
  label?: string;
  cwd?: string;
}

/** todo.Note. */
export interface TodoNote {
  at: string;
  text: string;
}

/** todo.Ref. */
export interface TodoRef {
  kind: string;
  value: string;
  url: string;
}

/** todo.Task. */
export interface TodoTask {
  id: number;
  title: string;
  desc: string;
  refs: TodoRef[] | null;
  status: string;
  initiative: string;
  contexts: string[] | null;
  tags: string[] | null;
  when: string | null;
  due: string | null;
  priority: number | null;
  waiting_on: string | null;
  links: string[] | null;
  notes: TodoNote[] | null;
}

/** todo.Group. */
export interface TodoGroup {
  group: string;
  tasks: TodoTask[] | null;
}

/** proto.Todos. */
export interface Todos {
  groups: TodoGroup[] | null;
  err?: string;
}

/** jira.Issue. */
export interface JiraIssue {
  key: string;
  summary: string;
  status: string;
  status_cat: string;
  type: string;
  priority: string;
  updated: string;
  url: string;
  desc?: string;
}

/** jira.Cache. */
export interface JiraCache {
  fetched_at: string;
  issues: JiraIssue[] | null;
  cost_usd: number;
}

/** proto.Snapshot: GET /v1/snapshot. */
export interface Snapshot {
  sessions: Session[];
  inbox: string[];
  groups: Record<string, GroupMember[]>;
  mail: Record<string, MailCue>;
  usage: Usage;
  todos: Todos;
  jira: JiraCache;
  pending: PendingConfirmation[];
  at: string;
  err?: string;
  today_cost: number;
}

/** Wire tab names (core.Tab order). */
export type TabName = 'sessions' | 'inbox' | 'todos' | 'archive';

/** Sort modes (core.SortModes). */
export type SortMode = 'atividade' | 'custo' | 'nome';

/** proto.ViewState: the client's tree state; the server keeps none. */
export interface ViewState {
  tab?: TabName;
  filter?: string;
  show_all?: boolean;
  sort?: SortMode;
  order?: Record<string, number>;
  new_rank?: number;
  collapsed?: Record<string, boolean>;
  ctx_color?: Record<string, number>;
  cursor?: number;
  cursor_key?: string;
  inbox_cursor?: number;
  /** Request only: re-sort by the current criterion. */
  reset_order?: boolean;
}

/** proto.TreeRequest: body of POST /v1/view/tree. */
export interface TreeRequest {
  state: ViewState;
}

/** proto.RowGroup. */
export interface RowGroup {
  tag: string;
  color: number;
}

/** proto.TreeRow: one line of the tree. */
export interface TreeRow {
  key: string;
  kind: 'ctx' | 'sess' | 'sub';
  ctx: string;
  depth: number;
  color: number;
  collapsed?: boolean;
  ctx_cost?: number;
  ctx_n?: number;
  session_id?: string;
  short_id?: string;
  label?: string;
  session_kind?: string;
  state?: SessionStateName;
  attention: number;
  cost?: number;
  stale?: boolean;
  archived?: boolean;
  ended?: boolean;
  finished?: boolean;
  subs_running?: number;
  options?: number;
  groups?: RowGroup[];
  mail?: MailCue;
  mail_state?: 'blink' | 'dim' | (string & {});
  sub?: Sub;
}

/** proto.Tree: answer of POST /v1/view/tree. */
export interface Tree {
  rows: TreeRow[] | null;
  state: ViewState;
  inbox: string[] | null;
  hidden: number;
  archived: number;
}

/** Theme token names (theme.Theme.Tokens). */
export type ThemeToken =
  | 'bg'
  | 'surface'
  | 'overlay'
  | 'border'
  | 'text'
  | 'subtle'
  | 'muted'
  | 'accent'
  | 'accent2'
  | 'busy'
  | 'blocked'
  | 'done'
  | 'idle'
  | 'error'
  | 'cost_low'
  | 'cost_high'
  | 'cat_body'
  | 'cat_eyes';

/** proto.Theme: GET /v1/theme. */
export interface Theme {
  name: string;
  tokens: Partial<Record<ThemeToken, string>> & Record<string, string>;
  contexts: string[];
  themes: string[];
  active: string;
}

/** proto.LuaEvalRequest. */
export interface LuaEvalRequest {
  code: string;
}

/** proto.LuaEval: result or Lua error (both 200). */
export interface LuaEval {
  result?: string;
  error?: string;
}

/** proto.PaneOpenRequest: body of POST /v1/panes. */
export interface PaneOpenRequest {
  session_id: string;
  cols?: number;
  rows?: number;
  reply?: string;
}

/** proto.PaneOpened. */
export interface PaneOpened {
  pane_id: string;
  reused?: boolean;
}

/** proto.Pane: GET /v1/panes entry. */
export interface Pane {
  id: string;
  session_id?: string;
  title?: string;
  cols: number;
  rows: number;
}

/** proto.Resize. */
export interface Resize {
  cols: number;
  rows: number;
}

/** proto.Control: a text frame of WS /v1/panes/{id}/ws. */
export interface Control {
  type: 'resize' | 'title' | 'exit' | (string & {});
  cols?: number;
  rows?: number;
  text?: string;
  err?: string;
}

// ---- event names and payloads (internal/app Ev* constants) ----

export const Ev = {
  SnapshotChanged: 'snapshot.changed',
  Toast: 'toast',
  ConfirmRequested: 'confirm.requested',
  ConfirmResolved: 'confirm.resolved',
  CommandRan: 'command.ran',
  Dispatched: 'dispatched',
  SessionState: 'session.state',
  SessionNotification: 'session.notification',
  Notice: 'notice',
  UsageUpdated: 'usage.updated',
  PaneOpened: 'pane.opened',
  PaneClosed: 'pane.closed',
  PaneExited: 'pane.exited',
  PaneResized: 'pane.resized',
  GroupChanged: 'group.changed',
  PeerMessage: 'peer.message',
  MailCue: 'mail.cue',
  TodosChanged: 'todos.changed',
  JiraChanged: 'jira.changed',
  InboxSummoned: 'inbox.summoned',
} as const;
export type EventName = (typeof Ev)[keyof typeof Ev];

/** proto.EvGap. */
export const EV_GAP = 'gap';

/** Every named SSE event the client listens to (EventSource has no wildcard). */
export const EVENT_NAMES: readonly string[] = Object.values(Ev);

/** app.SnapshotChanged. */
export interface SnapshotChanged {
  at: string;
  sessions: number;
  err?: string;
}
/** app.ToastData. */
export interface ToastData {
  text: string;
  err?: boolean;
}
/** app.ConfirmResolved; outcome is yes | no | expired. */
export interface ConfirmResolved {
  token: string;
  outcome: 'yes' | 'no' | 'expired' | (string & {});
}
/** app.CommandRan. */
export interface CommandRan {
  name: string;
  err?: string;
}
/** app.Dispatched. */
export interface Dispatched {
  id: string;
  dir: string;
  name?: string;
  task_id?: number;
}
/** app.SessionState: payload of session.state. */
export interface SessionStateChange {
  id: string;
  prev: string;
  state: SessionStateName;
}
/** app.SessionNotification. */
export interface SessionNotification {
  id: string;
  type?: string;
  message?: string;
}
/** app.Notice. */
export interface Notice {
  meow: string;
  bell?: boolean;
  toast?: string;
  toast_err?: boolean;
}
/** app.UsageUpdated. */
export interface UsageUpdated {
  at?: string;
  err?: string;
}
/** app.PaneEvent: payload of pane.opened/closed/exited. */
export interface PaneEvent {
  id: string;
  session_id?: string;
  err?: string;
}
/** proto.PaneResized. */
export interface PaneResized {
  id: string;
  cols: number;
  rows: number;
}
/** app.GroupChanged. */
export interface GroupChanged {
  tag: string;
}
/** app.PeerMessage. */
export interface PeerMessage {
  to: string;
  from: string;
  from_id?: string;
  text: string;
}
/** app.MailCueEvent. */
export interface MailCueEvent {
  id: string;
  sent: boolean;
}
/** app.JiraChanged. */
export interface JiraChanged {
  fetched_at: string;
  issues: number;
  cost_usd: number;
  err?: string;
}
/** app.TodoList: payload of todos.changed. */
export type TodosChanged = Todos;
/** app.InboxSummoned: the session inbox.summon picked (any client ran it). */
export interface InboxSummoned {
  session_id: string;
  label: string;
}

/** Payload type of each known event. */
export interface EventPayloads {
  'snapshot.changed': SnapshotChanged;
  toast: ToastData;
  'confirm.requested': PendingConfirmation;
  'confirm.resolved': ConfirmResolved;
  'command.ran': CommandRan;
  dispatched: Dispatched;
  'session.state': SessionStateChange;
  'session.notification': SessionNotification;
  notice: Notice;
  'usage.updated': UsageUpdated;
  'pane.opened': PaneEvent;
  'pane.closed': PaneEvent;
  'pane.exited': PaneEvent;
  'pane.resized': PaneResized;
  'group.changed': GroupChanged;
  'peer.message': PeerMessage;
  'mail.cue': MailCueEvent;
  'todos.changed': TodosChanged;
  'jira.changed': JiraChanged;
  'inbox.summoned': InboxSummoned;
}
