// The window manager's state: where each terminal window is, which one is
// on top, which is active, which is zoomed. Pure reducers over an Area
// (the part of the screen windows may use: right of the tree column,
// below the top bar, above the 72px "calçada" where the inbox cat walks).

/** The calçada: the strip at the base windows never enter (plan §4.1). */
export const CALCADA_PX = 72;

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type WinStatus = 'opening' | 'open' | 'exited' | 'error';

export interface Win extends Rect {
  /** The session shown (one window per session). */
  id: string;
  title: string;
  z: number;
  zoomed: boolean;
  status: WinStatus;
  paneId?: string;
  error?: string;
  /** The program's own title (OSC 0/2, relayed by the core). */
  subtitle?: string;
}

export interface WinState {
  wins: Win[];
  /** The window w/z/i act on (last focused), or null. */
  active: string | null;
  nextZ: number;
}

export const MIN_W = 320;
export const MIN_H = 180;
export const DEFAULT_W = 780;
export const DEFAULT_H = 470;
const CASCADE = 6;

export function emptyWins(): WinState {
  return { wins: [], active: null, nextZ: 1 };
}

/** The area windows live in for a viewport and the chrome around it. */
export function windowArea(vw: number, vh: number, chrome: { left: number; top: number; bottom: number }): Rect {
  const x = chrome.left;
  const y = chrome.top;
  return { x, y, w: Math.max(vw - x, 0), h: Math.max(vh - chrome.bottom - CALCADA_PX - y, 0) };
}

/** Keeps r inside area (shrinking it first if it does not fit). */
export function clampRect(r: Rect, area: Rect): Rect {
  const w = Math.max(Math.min(r.w, area.w), Math.min(MIN_W, area.w));
  const h = Math.max(Math.min(r.h, area.h), Math.min(MIN_H, area.h));
  const x = Math.min(Math.max(r.x, area.x), area.x + area.w - w);
  const y = Math.min(Math.max(r.y, area.y), area.y + area.h - h);
  return { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) };
}

/** The rect a window is drawn at (the whole area when zoomed). */
export function shownRect(w: Win, area: Rect): Rect {
  return w.zoomed ? { ...area } : clampRect(w, area);
}

export function find(s: WinState, id: string | null | undefined): Win | undefined {
  return id ? s.wins.find((w) => w.id === id) : undefined;
}

function patch(s: WinState, id: string, f: (w: Win) => Win): WinState {
  let hit = false;
  const wins = s.wins.map((w) => {
    if (w.id !== id) return w;
    hit = true;
    return f(w);
  });
  return hit ? { ...s, wins } : s;
}

/** Raises a window and makes it active. */
export function focusWin(s: WinState, id: string): WinState {
  const w = find(s, id);
  if (!w) return s;
  const top = s.wins.every((x) => x.id === id || x.z < w.z);
  if (top && s.active === id) return s;
  return { ...patch(s, id, (x) => ({ ...x, z: s.nextZ })), active: id, nextZ: s.nextZ + 1 };
}

/** Opens a window for a session (cascading from the area's corner), or
 * raises the one already open. */
export function openWin(s: WinState, id: string, title: string, area: Rect): WinState {
  if (find(s, id)) return focusWin(s, id);
  const n = s.wins.length % CASCADE;
  const r = clampRect(
    {
      x: area.x + 24 + n * 36,
      y: area.y + 16 + n * 30,
      w: Math.min(DEFAULT_W, area.w - 48),
      h: Math.min(DEFAULT_H, area.h - 32),
    },
    area,
  );
  const w: Win = { id, title, ...r, z: s.nextZ, zoomed: false, status: 'opening' };
  return { wins: [...s.wins, w], active: id, nextZ: s.nextZ + 1 };
}

/** Closes a window; the active one passes to the topmost left. */
export function closeWin(s: WinState, id: string): WinState {
  if (!find(s, id)) return s;
  const wins = s.wins.filter((w) => w.id !== id);
  let active = s.active;
  if (active === id) {
    const top = wins.reduce<Win | undefined>((a, w) => (!a || w.z > a.z ? w : a), undefined);
    active = top?.id ?? null;
  }
  return { ...s, wins, active };
}

/** Moves a window to (x, y), kept inside the area. */
export function moveWin(s: WinState, id: string, x: number, y: number, area: Rect): WinState {
  return patch(s, id, (w) => ({ ...w, ...clampRect({ x, y, w: w.w, h: w.h }, area) }));
}

/** Moves a window by (dx, dy) — the arrow keys of keyboard users. */
export function nudgeWin(s: WinState, id: string, dx: number, dy: number, area: Rect): WinState {
  const w = find(s, id);
  return w ? moveWin(s, id, w.x + dx, w.y + dy, area) : s;
}

/** Resizes a window, kept inside the area. */
export function resizeWin(s: WinState, id: string, width: number, height: number, area: Rect): WinState {
  return patch(s, id, (w) => ({ ...w, ...clampRect({ x: w.x, y: w.y, w: width, h: height }, area) }));
}

/** z: toggles a window filling the area. */
export function toggleZoom(s: WinState, id: string): WinState {
  return patch(s, id, (w) => ({ ...w, zoomed: !w.zoomed }));
}

/** Re-fits every window after the viewport changed. */
export function clampAll(s: WinState, area: Rect): WinState {
  return { ...s, wins: s.wins.map((w) => ({ ...w, ...clampRect(w, area) })) };
}

export function setPane(s: WinState, id: string, paneId: string): WinState {
  return patch(s, id, (w) => ({ ...w, paneId, status: 'open', error: undefined }));
}

export function setStatus(s: WinState, id: string, status: WinStatus, error?: string): WinState {
  return patch(s, id, (w) => ({ ...w, status, error }));
}

/** The program's title (OSC); the window's own title stays the label. */
export function setTitle(s: WinState, id: string, subtitle: string): WinState {
  return patch(s, id, (w) => (w.subtitle === subtitle ? w : { ...w, subtitle }));
}

/** Renames a window (the session's label changed). */
export function setLabel(s: WinState, id: string, title: string): WinState {
  return patch(s, id, (w) => (w.title === title ? w : { ...w, title }));
}

/** The window showing a pane. */
export function byPane(s: WinState, paneId: string): Win | undefined {
  return s.wins.find((w) => w.paneId === paneId);
}
