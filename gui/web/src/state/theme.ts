// Theme helpers: /v1/theme tokens → CSS custom properties, and the colour
// lookups the TUI does (Theme.ContextColor, state colours). Pure.

import type { Theme } from '../client/types.ts';

/** catppuccin-mocha, used until /v1/theme answers (same values the core serves). */
export const FALLBACK_THEME: Theme = {
  name: 'catppuccin-mocha',
  tokens: {
    bg: '#1e1e2e',
    surface: '#313244',
    overlay: '#45475a',
    border: '#585b70',
    text: '#cdd6f4',
    subtle: '#a6adc8',
    muted: '#7f849c',
    accent: '#cba6f7',
    accent2: '#89b4fa',
    busy: '#89dceb',
    blocked: '#f38ba8',
    done: '#a6e3a1',
    idle: '#9399b2',
    error: '#eba0ac',
    cost_low: '#a6e3a1',
    cost_high: '#f38ba8',
    cat_body: '#11111b',
    cat_eyes: '#f9e2af',
  },
  contexts: ['#f5c2e7', '#89b4fa', '#a6e3a1', '#fab387', '#94e2d5', '#f9e2af', '#b4befe'],
  themes: ['catppuccin-mocha'],
  active: 'catppuccin-mocha',
};

/** CSS variable name of a token: cost_low → --nk-cost-low. */
export function cssVar(token: string): string {
  return `--nk-${token.replace(/_/g, '-')}`;
}

/** Every token as a CSS variable, plus --nk-ctx-<i> for the context palette. */
export function themeVars(theme: Theme): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(theme.tokens)) out[cssVar(k)] = v;
  theme.contexts.forEach((c, i) => {
    out[`--nk-ctx-${i}`] = c;
  });
  return out;
}

/** The context/collar colour for palette index i (contexts[i % len]). */
export function contextColor(theme: Theme, i: number): string {
  const n = theme.contexts.length;
  if (n === 0) return theme.tokens.accent ?? '#ffffff';
  return theme.contexts[((i % n) + n) % n]!;
}

/** The token that colours a session state (as the TUI's pills). */
export function stateToken(state: string | undefined): 'busy' | 'blocked' | 'done' | 'idle' | 'muted' {
  switch (state) {
    case 'busy':
      return 'busy';
    case 'blocked':
      return 'blocked';
    case 'done':
      return 'done';
    case 'idle':
    case 'stopped':
      return 'idle';
    default:
      return 'muted';
  }
}
