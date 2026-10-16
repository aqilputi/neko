// The scene model: snapshot + tree rows + theme → what the WebGL (or the
// flat) scene shows. Pure — no three.js, no DOM — so the renderer only
// draws what this decides and the tests are plain tables.
//
// Stations: one per context, on a shallow desk, in a stable order (by name,
// so cats don't hop around when the tree re-sorts by activity). Cats: one
// per top-level session (every non-archived session that is not history,
// plus history sessions the tree shows, e.g. with view.show_all). Kittens:
// the session's running subagents. Collars: the first group's colour;
// lit for the peers of the hovered/selected session. Arcs: ✉ from the
// sender to the receiver of a mail cue.

import type { MailCue, Session, Snapshot, Theme, TreeRow } from '../client/types.ts';
import { contextColor } from '../state/theme.ts';

/** What a cat is doing (plan §4.2). */
export type Pose = 'typing' | 'alert' | 'sleeping' | 'sit' | 'puff';

/** How long a ✉ blinks after it was sent/received (mailCueFor: 90 s). */
export const MAIL_BLINK_MS = 90_000;
/** After this the ✉ is gone (mailCueFor: 15 min dim). */
export const MAIL_DIM_MS = 15 * 60_000;
/** A sent and a received cue this close in time are one message. */
export const MAIL_PAIR_MS = 5_000;

// layout, in world units (a cat is ~1 unit tall)
export const CAT_SLOT = 1.5;
export const KITTEN_SLOT = 0.5;
export const STATION_PAD = 0.45;
export const STATION_GAP = 0.7;
export const ROW_DEPTH = 2.6;
export const MAX_ROW_WIDTH = 16;

export interface KittenModel {
  id: string;
  label: string;
  /** World position (x, z); y is the desk. */
  x: number;
  z: number;
}

export interface CatModel {
  id: string;
  label: string;
  ctx: string;
  state: string;
  attention: number;
  pose: Pose;
  x: number;
  z: number;
  /** Collar colour (first group's), or null without a group. */
  collar: string | null;
  /** Peer of the hovered/selected session: collar lit (TUI's ┃). */
  collarLit: boolean;
  /** Hovered or selected itself. */
  focused: boolean;
  /** Stale/history/ended: drawn faded. */
  dim: boolean;
  /** Overlay above the head. */
  bubble: '!' | 'zZ' | null;
  /** Colour of the thin glow under the paws (busy), or null. */
  glow: string | null;
  /** Its ✉ cue, if any: blinking during the first 90 s. */
  mail: { sent: boolean; blink: boolean } | null;
  kittens: KittenModel[];
}

export interface StationModel {
  ctx: string;
  color: string;
  /** Centre and width along x; row depth along z. */
  x: number;
  z: number;
  width: number;
}

export interface ArcModel {
  /** sender → receiver session ids */
  from: string;
  to: string;
  at: string;
  blink: boolean;
}

export interface SceneColors {
  bg: string;
  desk: string;
  deskEdge: string;
  body: string;
  eyes: string;
  busy: string;
  blocked: string;
  done: string;
  idle: string;
  error: string;
  muted: string;
  accent: string;
  text: string;
}

export interface SceneModel {
  stations: StationModel[];
  cats: CatModel[];
  arcs: ArcModel[];
  colors: SceneColors;
  /** Extent of the desk (x from -width/2 to width/2, z from -depth+1 to 1). */
  width: number;
  depth: number;
  /** Next time (ms epoch) the model changes by itself (a ✉ stops blinking), or null. */
  nextChangeAt: number | null;
}

/** Hover/selection that light the peers' collars. */
export interface SceneFocus {
  hovered?: string | null;
  selected?: string | null;
}

/** The fields poseFor reads (a Session or a TreeRow both fit). */
export interface PoseInput {
  state?: string;
  attention?: number;
  ended?: boolean;
  finished?: boolean;
}

/** state/attention → pose (plan §4.2). Precedence: puff, alert, typing, sleeping, sit. */
export function poseFor(s: PoseInput): Pose {
  const state = s.state ?? '';
  if (s.ended || state === 'error' || state === 'failed') return 'puff';
  if (s.attention === 1 || state === 'blocked') return 'alert';
  if (state === 'busy') return 'typing';
  if (state === 'done' || s.finished || s.attention === 2) return 'sleeping';
  return 'sit';
}

export function bubbleFor(pose: Pose): CatModel['bubble'] {
  return pose === 'alert' ? '!' : pose === 'sleeping' ? 'zZ' : null;
}

function tok(theme: Theme, name: string, fallback: string): string {
  return theme.tokens[name] ?? fallback;
}

export function sceneColors(theme: Theme): SceneColors {
  return {
    bg: tok(theme, 'bg', '#1e1e2e'),
    desk: tok(theme, 'surface', '#313244'),
    deskEdge: tok(theme, 'overlay', '#45475a'),
    body: tok(theme, 'cat_body', '#11111b'),
    eyes: tok(theme, 'cat_eyes', '#f9e2af'),
    busy: tok(theme, 'busy', '#89dceb'),
    blocked: tok(theme, 'blocked', '#f38ba8'),
    done: tok(theme, 'done', '#a6e3a1'),
    idle: tok(theme, 'idle', '#9399b2'),
    error: tok(theme, 'error', '#eba0ac'),
    muted: tok(theme, 'muted', '#7f849c'),
    accent: tok(theme, 'accent', '#cba6f7'),
    text: tok(theme, 'text', '#cdd6f4'),
  };
}

const time = (s: string | undefined): number => {
  const t = s ? Date.parse(s) : NaN;
  return Number.isFinite(t) ? t : 0;
};

/** The sessions that get a cat, in station order. */
export function catSessions(snapshot: Snapshot | null, rows: readonly TreeRow[]): Session[] {
  const shown = new Set<string>();
  for (const r of rows) if (r.kind === 'sess' && r.session_id) shown.add(r.session_id);
  const out = (snapshot?.sessions ?? []).filter(
    (s) => !s.archived && (s.kind !== 'history' || shown.has(s.session_id)),
  );
  return out.sort(
    (a, b) =>
      (a.context ?? '').localeCompare(b.context ?? '') ||
      time(a.started) - time(b.started) ||
      a.session_id.localeCompare(b.session_id),
  );
}

/** Group tags of each session (from the rows, falling back to snapshot.groups). */
function groupsOf(snapshot: Snapshot | null, rows: readonly TreeRow[]): Map<string, { tag: string; color: number }[]> {
  const m = new Map<string, { tag: string; color: number }[]>();
  for (const r of rows) if (r.kind === 'sess' && r.session_id && r.groups?.length) m.set(r.session_id, r.groups);
  // sessions the tree doesn't show (collapsed context): tags from the snapshot,
  // colour from any row that carries the same tag
  const tagColor = new Map<string, number>();
  for (const gs of m.values()) for (const g of gs) tagColor.set(g.tag, g.color);
  for (const [tag, members] of Object.entries(snapshot?.groups ?? {})) {
    for (const mem of members ?? []) {
      if (m.has(mem.session_id) && m.get(mem.session_id)!.some((g) => g.tag === tag)) continue;
      const list = m.get(mem.session_id) ?? [];
      list.push({ tag, color: tagColor.get(tag) ?? hashColor(tag) });
      m.set(mem.session_id, list);
    }
  }
  return m;
}

/** FNV-1a of the tag — the same idea as core.GroupColorIndex, used only when no row tells the colour. */
export function hashColor(tag: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < tag.length; i++) {
    h ^= tag.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h;
}

/** The mail cue state: blink in the first 90 s, dim until 15 min, then none. */
export function mailState(cue: MailCue | undefined, now: number, fromRow?: string): 'blink' | 'dim' | null {
  if (fromRow === 'blink' || fromRow === 'dim') return fromRow;
  if (!cue) return null;
  const age = now - time(cue.at);
  if (age < MAIL_BLINK_MS) return 'blink';
  if (age < MAIL_DIM_MS) return 'dim';
  return null;
}

/** Pairs sent and received cues (same group, ≤5 s apart) into sender→receiver arcs. */
export function mailArcs(
  cues: ReadonlyMap<string, { cue: MailCue; state: 'blink' | 'dim' }>,
  groups: ReadonlyMap<string, { tag: string }[]>,
): ArcModel[] {
  const senders = [...cues].filter(([, c]) => c.cue.sent);
  const out: ArcModel[] = [];
  for (const [to, c] of cues) {
    if (c.cue.sent) continue;
    const tags = new Set((groups.get(to) ?? []).map((g) => g.tag));
    let best: { id: string; d: number } | null = null;
    for (const [from, s] of senders) {
      if (from === to) continue;
      const shares = tags.size === 0 || (groups.get(from) ?? []).some((g) => tags.has(g.tag));
      const d = Math.abs(time(s.cue.at) - time(c.cue.at));
      if (shares && d <= MAIL_PAIR_MS && (!best || d < best.d)) best = { id: from, d };
    }
    if (best) out.push({ from: best.id, to, at: c.cue.at, blink: c.state === 'blink' });
  }
  return out.sort((a, b) => a.to.localeCompare(b.to));
}

/** Station width for its cats (kittens widen a cat's slot past two). */
export function slotWidth(kittens: number): number {
  return CAT_SLOT + Math.max(0, kittens - 2) * KITTEN_SLOT;
}

export function buildScene(
  snapshot: Snapshot | null,
  rows: readonly TreeRow[],
  theme: Theme,
  now: number,
  focus: SceneFocus = {},
): SceneModel {
  const colors = sceneColors(theme);
  const sessions = catSessions(snapshot, rows);
  const rowById = new Map<string, TreeRow>();
  const ctxColor = new Map<string, number>();
  for (const r of rows) {
    if (r.kind === 'sess' && r.session_id) rowById.set(r.session_id, r);
    if (!ctxColor.has(r.ctx)) ctxColor.set(r.ctx, r.color);
  }
  const groups = groupsOf(snapshot, rows);

  // mail cues → per-session state and arcs
  const cues = new Map<string, { cue: MailCue; state: 'blink' | 'dim' }>();
  let nextChangeAt: number | null = null;
  const ids = new Set(sessions.map((s) => s.session_id));
  for (const s of sessions) {
    const row = rowById.get(s.session_id);
    const cue = row?.mail ?? snapshot?.mail?.[s.session_id];
    const st = mailState(cue, now, row?.mail_state);
    if (cue && st) {
      cues.set(s.session_id, { cue, state: st });
      const end = time(cue.at) + (st === 'blink' ? MAIL_BLINK_MS : MAIL_DIM_MS);
      if (end > now && (nextChangeAt === null || end < nextChangeAt)) nextChangeAt = end;
    }
  }
  const arcs = mailArcs(cues, groups).filter((a) => ids.has(a.from) && ids.has(a.to));

  // collars lit: peers (sharing a tag) of the hovered, else the selected session
  const focusId = focus.hovered ?? focus.selected ?? null;
  const focusTags = new Set((focusId ? groups.get(focusId) : undefined)?.map((g) => g.tag) ?? []);

  // stations, packed into rows of at most MAX_ROW_WIDTH
  const byCtx = new Map<string, Session[]>();
  for (const s of sessions) {
    const k = s.context ?? '';
    const list = byCtx.get(k) ?? [];
    list.push(s);
    byCtx.set(k, list);
  }
  const running = (s: Session) => (s.subs ?? []).filter((x) => !x.done);
  type Pending = { ctx: string; sessions: Session[]; width: number };
  const deskRows: Pending[][] = [];
  let cur: Pending[] = [];
  let curW = 0;
  let ctxIndex = 0;
  for (const [ctx, list] of byCtx) {
    const width = list.reduce((w, s) => w + slotWidth(running(s).length), 0) + 2 * STATION_PAD;
    if (cur.length > 0 && curW + STATION_GAP + width > MAX_ROW_WIDTH) {
      deskRows.push(cur);
      cur = [];
      curW = 0;
    }
    curW += (cur.length ? STATION_GAP : 0) + width;
    cur.push({ ctx, sessions: list, width });
    if (!ctxColor.has(ctx)) ctxColor.set(ctx, ctxIndex);
    ctxIndex++;
  }
  if (cur.length) deskRows.push(cur);

  const stations: StationModel[] = [];
  const cats: CatModel[] = [];
  let width = 0;
  deskRows.forEach((row, ri) => {
    const rowW = row.reduce((w, p) => w + p.width, 0) + STATION_GAP * (row.length - 1);
    width = Math.max(width, rowW);
    const z = -ri * ROW_DEPTH;
    let x = -rowW / 2;
    for (const p of row) {
      stations.push({ ctx: p.ctx, color: contextColor(theme, ctxColor.get(p.ctx) ?? 0), x: x + p.width / 2, z, width: p.width });
      let cx = x + STATION_PAD;
      for (const s of p.sessions) {
        const kits = running(s);
        const w = slotWidth(kits.length);
        const catX = cx + w / 2;
        const pose = poseFor(s);
        const tags = groups.get(s.session_id) ?? [];
        const c = cues.get(s.session_id);
        cats.push({
          id: s.session_id,
          label: s.label || s.name || s.short_id || s.session_id.slice(0, 8),
          ctx: p.ctx,
          state: s.state,
          attention: s.attention,
          pose,
          x: catX,
          z,
          collar: tags[0] ? contextColor(theme, tags[0].color) : null,
          collarLit: focusTags.size > 0 && tags.some((g) => focusTags.has(g.tag)),
          focused: s.session_id === focusId,
          dim: !!(s.stale || s.ended || s.kind === 'history'),
          bubble: bubbleFor(pose),
          glow: pose === 'typing' ? colors.busy : null,
          mail: c ? { sent: c.cue.sent, blink: c.state === 'blink' } : null,
          kittens: kits.map((k, i) => ({
            id: k.id,
            label: k.description || k.type || k.id,
            x: catX + (i - (kits.length - 1) / 2) * KITTEN_SLOT,
            z: z + 0.75,
          })),
        });
        cx += w;
      }
      x += p.width + STATION_GAP;
    }
  });

  return {
    stations,
    cats,
    arcs,
    colors,
    width,
    depth: Math.max(1, deskRows.length) * ROW_DEPTH,
    nextChangeAt,
  };
}

/** A stable string of everything drawn: equal keys ⇒ nothing to redraw. */
export function modelKey(m: SceneModel): string {
  return JSON.stringify([m.stations, m.cats, m.arcs, m.colors]);
}

/** Poses by session id (the debug hook and tests). */
export function posesById(m: SceneModel): Record<string, Pose> {
  const out: Record<string, Pose> = {};
  for (const c of m.cats) out[c.id] = c.pose;
  return out;
}
