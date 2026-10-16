import { describe, expect, it } from 'vitest';
import type { VNode } from 'preact';
import { FALLBACK_THEME } from '../state/theme.ts';
import { GW, MIG, snapshot } from '../test/fixtures.ts';
import { FlatScene, flatCatText } from './Flat.tsx';
import { buildScene, type CatModel } from './model.ts';

const cat = (over: Partial<CatModel>): CatModel => ({
  id: 'x',
  label: 'x',
  ctx: 'c',
  state: 'idle',
  attention: 0,
  pose: 'sit',
  x: 0,
  z: 0,
  collar: null,
  collarLit: false,
  focused: false,
  dim: false,
  bubble: null,
  glow: null,
  mail: null,
  kittens: [],
  ...over,
});

describe('flatCatText', () => {
  it.each([
    ['sit', null, 'ᓚᘏᗢ'],
    ['alert', null, 'ᓚᘏᗢ !'],
    ['sleeping', null, 'ᓚᘏᗢ zZ'],
    ['typing', null, 'ᓚᘏᗢ ⌨'],
    ['puff', null, 'ᓚᘏᗢ ✶'],
    ['alert', { sent: false, blink: true }, 'ᓚᘏᗢ ! ✉'],
  ] as const)('%s %o → %s', (pose, mail, want) => {
    expect(flatCatText(cat({ pose, mail }))).toBe(want);
  });
});

/** Every element vnode under v, depth first. */
function walk(v: unknown, out: VNode[] = []): VNode[] {
  if (Array.isArray(v)) {
    for (const x of v) walk(x, out);
  } else if (v && typeof v === 'object' && 'props' in v) {
    const n = v as VNode<{ children?: unknown }>;
    out.push(n);
    walk(n.props.children, out);
  }
  return out;
}

describe('FlatScene', () => {
  it('one clickable text cat per session, grouped by station', () => {
    const m = buildScene(snapshot(), [], FALLBACK_THEME, Date.now());
    const opened: string[] = [];
    const nodes = walk(FlatScene({ model: m, onOpenSession: (id) => opened.push(id), reason: 'webgl' }));
    const buttons = nodes.filter((n) => n.type === 'button') as VNode<{ 'data-session': string; 'data-pose': string; onClick(): void }>[];
    expect(buttons.map((b) => [b.props['data-session'], b.props['data-pose']])).toEqual([
      [GW, 'typing'],
      [MIG, 'alert'],
    ]);
    buttons[1]!.props.onClick();
    expect(opened).toEqual([MIG]);
    expect(nodes.some((n) => n.props && (n.props as { class?: string }).class === 'nk-flat-note')).toBe(true);
  });
});
