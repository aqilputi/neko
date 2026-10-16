import { describe, expect, it } from 'vitest';
import {
  CALCADA_PX,
  MIN_H,
  MIN_W,
  byPane,
  clampAll,
  clampRect,
  closeWin,
  emptyWins,
  find,
  focusWin,
  moveWin,
  nudgeWin,
  openWin,
  resizeWin,
  setPane,
  setStatus,
  setLabel,
  setTitle,
  shownRect,
  toggleZoom,
  windowArea,
  type Rect,
} from './layout.ts';

const AREA: Rect = { x: 300, y: 56, w: 1300, h: 700 };

describe('windowArea', () => {
  it('leaves the chrome and the calçada out', () => {
    expect(windowArea(1600, 900, { left: 300, top: 56, bottom: 32 })).toEqual({
      x: 300,
      y: 56,
      w: 1300,
      h: 900 - 32 - CALCADA_PX - 56,
    });
  });
  it('never negative', () => {
    expect(windowArea(100, 100, { left: 300, top: 56, bottom: 32 })).toEqual({ x: 300, y: 56, w: 0, h: 0 });
  });
});

describe('clampRect', () => {
  it.each([
    ['inside stays', { x: 400, y: 100, w: 500, h: 300 }, { x: 400, y: 100, w: 500, h: 300 }],
    ['left of the area', { x: 0, y: 100, w: 500, h: 300 }, { x: 300, y: 100, w: 500, h: 300 }],
    ['into the calçada', { x: 400, y: 700, w: 500, h: 300 }, { x: 400, y: 456, w: 500, h: 300 }],
    ['above the top bar', { x: 400, y: -50, w: 500, h: 300 }, { x: 400, y: 56, w: 500, h: 300 }],
    ['past the right edge', { x: 1500, y: 100, w: 500, h: 300 }, { x: 1100, y: 100, w: 500, h: 300 }],
    ['too big shrinks', { x: 0, y: 0, w: 5000, h: 5000 }, { x: 300, y: 56, w: 1300, h: 700 }],
    ['too small grows', { x: 400, y: 100, w: 10, h: 10 }, { x: 400, y: 100, w: MIN_W, h: MIN_H }],
    ['rounds', { x: 400.4, y: 100.6, w: 500.5, h: 300.2 }, { x: 400, y: 101, w: 501, h: 300 }],
  ])('%s', (_n, r, want) => expect(clampRect(r, AREA)).toEqual(want));

  it('a tiny area wins over the minimum size', () => {
    expect(clampRect({ x: 0, y: 0, w: 500, h: 500 }, { x: 0, y: 0, w: 100, h: 50 })).toEqual({ x: 0, y: 0, w: 100, h: 50 });
  });
});

describe('open / focus / close', () => {
  it('opens cascading, each on top and active', () => {
    let s = openWin(emptyWins(), 'a', 'A', AREA);
    s = openWin(s, 'b', 'B', AREA);
    expect(s.active).toBe('b');
    const [a, b] = s.wins;
    expect(a).toMatchObject({ id: 'a', title: 'A', x: 324, y: 72, w: 780, h: 470, z: 1, status: 'opening', zoomed: false });
    expect(b).toMatchObject({ id: 'b', x: 360, y: 102, z: 2 });
  });

  it('reopening raises instead of duplicating', () => {
    let s = openWin(emptyWins(), 'a', 'A', AREA);
    s = openWin(s, 'b', 'B', AREA);
    s = openWin(s, 'a', 'A', AREA);
    expect(s.wins).toHaveLength(2);
    expect(s.active).toBe('a');
    expect(find(s, 'a')!.z).toBe(3);
  });

  it('focusing the top active window is a no-op', () => {
    const s = openWin(emptyWins(), 'a', 'A', AREA);
    expect(focusWin(s, 'a')).toBe(s);
    expect(focusWin(s, 'zz')).toBe(s);
  });

  it('closing the active hands it to the topmost', () => {
    let s = openWin(emptyWins(), 'a', 'A', AREA);
    s = openWin(s, 'b', 'B', AREA);
    s = openWin(s, 'c', 'C', AREA);
    s = focusWin(s, 'a');
    s = closeWin(s, 'a');
    expect(s.active).toBe('c');
    s = closeWin(s, 'b');
    expect(s.active).toBe('c');
    s = closeWin(s, 'c');
    expect(s.active).toBeNull();
    expect(closeWin(s, 'c')).toBe(s);
  });
});

describe('move / nudge / resize / zoom', () => {
  const s0 = openWin(emptyWins(), 'a', 'A', AREA);
  it('move is clamped', () => {
    expect(find(moveWin(s0, 'a', 2000, 2000, AREA), 'a')).toMatchObject({ x: 820, y: 286 });
  });
  it('nudge moves by a delta', () => {
    expect(find(nudgeWin(s0, 'a', 10, 20, AREA), 'a')).toMatchObject({ x: 334, y: 92 });
    expect(nudgeWin(s0, 'zz', 10, 20, AREA)).toBe(s0);
  });
  it('resize is clamped', () => {
    expect(find(resizeWin(s0, 'a', 100, 9999, AREA), 'a')).toMatchObject({ w: MIN_W, h: 700, y: 56 });
  });
  it('zoom fills the area and toggles back', () => {
    const z = toggleZoom(s0, 'a');
    expect(shownRect(find(z, 'a')!, AREA)).toEqual(AREA);
    const back = toggleZoom(z, 'a');
    expect(shownRect(find(back, 'a')!, AREA)).toEqual({ x: 324, y: 72, w: 780, h: 470 });
  });
  it('unknown ids leave the state alone', () => {
    expect(toggleZoom(s0, 'zz')).toBe(s0);
  });
  it('clampAll refits after the viewport shrank', () => {
    const small: Rect = { x: 300, y: 56, w: 600, h: 400 };
    expect(find(clampAll(s0, small), 'a')).toMatchObject({ x: 300, y: 56, w: 600, h: 400 });
  });
});

describe('pane bookkeeping', () => {
  const s0 = openWin(emptyWins(), 'a', 'A', AREA);
  it('setPane opens and indexes by pane', () => {
    const s = setPane(setStatus(s0, 'a', 'error', 'x'), 'a', 'p1');
    expect(find(s, 'a')).toMatchObject({ paneId: 'p1', status: 'open', error: undefined });
    expect(byPane(s, 'p1')?.id).toBe('a');
    expect(byPane(s, 'p2')).toBeUndefined();
  });
  it('setStatus and setTitle', () => {
    expect(find(setStatus(s0, 'a', 'exited'), 'a')!.status).toBe('exited');
    const t = setTitle(s0, 'a', 'claude · api');
    expect(find(t, 'a')!.subtitle).toBe('claude · api');
    expect(find(setTitle(t, 'a', 'claude · api'), 'a')).toBe(find(t, 'a'));
    expect(find(setLabel(s0, 'a', 'B'), 'a')!.title).toBe('B');
    expect(find(setLabel(s0, 'a', 'A'), 'a')).toBe(find(s0, 'a'));
  });
});
