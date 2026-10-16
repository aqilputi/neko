// The chrome's fixed sizes, shared by the CSS (as --nk-* variables set on
// :root at start) and by the window manager's area. The calçada (72px, the
// inbox cat's strip) is the very bottom; the status bar sits right above
// it; windows live between the top bar and the status bar, right of the
// tree column.

import { CALCADA_PX } from '../windows/layout.ts';

export const TOP_H = 56;
export const STATUS_H = 40;
export const TREE_W = 340;

/** CSS variables for the sizes (set once on :root). */
export function metricVars(): Record<string, string> {
  return {
    '--nk-top-h': `${TOP_H}px`,
    '--nk-status-h': `${STATUS_H}px`,
    '--nk-tree-w': `${TREE_W}px`,
    '--nk-calcada-h': `${CALCADA_PX}px`,
  };
}

/** The chrome around the window area, for windowArea(). */
export function chromeInsets(treeOpen: boolean): { left: number; top: number; bottom: number } {
  return { left: treeOpen ? TREE_W : 0, top: TOP_H, bottom: STATUS_H };
}
