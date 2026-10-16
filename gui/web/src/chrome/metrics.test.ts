import { describe, expect, it } from 'vitest';
import { CALCADA_PX, windowArea } from '../windows/layout.ts';
import { STATUS_H, TOP_H, TREE_W, chromeInsets, metricVars } from './metrics.ts';

describe('metrics', () => {
  it('CSS variables for the sizes', () => {
    expect(metricVars()).toEqual({
      '--nk-top-h': `${TOP_H}px`,
      '--nk-status-h': `${STATUS_H}px`,
      '--nk-tree-w': `${TREE_W}px`,
      '--nk-calcada-h': `${CALCADA_PX}px`,
    });
  });
  it('the window area leaves the tree only when it is open', () => {
    expect(chromeInsets(true)).toEqual({ left: TREE_W, top: TOP_H, bottom: STATUS_H });
    expect(chromeInsets(false).left).toBe(0);
    const a = windowArea(1600, 900, chromeInsets(true));
    expect(a.y + a.h).toBe(900 - STATUS_H - CALCADA_PX);
  });
});
