import { describe, expect, it } from 'vitest';
import { FALLBACK_THEME, contextColor, cssVar, stateToken, themeVars } from './theme.ts';

describe('theme helpers', () => {
  it('maps tokens to CSS variables', () => {
    expect(cssVar('cost_low')).toBe('--nk-cost-low');
    const v = themeVars(FALLBACK_THEME);
    expect(v['--nk-bg']).toBe('#1e1e2e');
    expect(v['--nk-cat-eyes']).toBe('#f9e2af');
    expect(v['--nk-ctx-0']).toBe('#f5c2e7');
    expect(v['--nk-ctx-6']).toBe('#b4befe');
  });

  it.each([
    [0, '#f5c2e7'],
    [7, '#f5c2e7'],
    [18, '#94e2d5'], // core.GroupColorIndex can exceed the palette
    [-1, '#b4befe'],
  ])('contextColor(%i)', (i, want) => {
    expect(contextColor(FALLBACK_THEME, i)).toBe(want);
  });

  it('contextColor falls back to the accent with an empty palette', () => {
    expect(contextColor({ ...FALLBACK_THEME, contexts: [] }, 3)).toBe('#cba6f7');
    expect(contextColor({ ...FALLBACK_THEME, contexts: [], tokens: {} }, 3)).toBe('#ffffff');
  });

  it.each([
    ['busy', 'busy'],
    ['blocked', 'blocked'],
    ['done', 'done'],
    ['idle', 'idle'],
    ['stopped', 'idle'],
    ['history', 'muted'],
    [undefined, 'muted'],
  ])('stateToken(%s) = %s', (s, want) => {
    expect(stateToken(s)).toBe(want);
  });
});
