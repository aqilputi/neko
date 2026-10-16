import { describe, expect, it } from 'vitest';
import { LEGS, runs, SPRITE_W } from './sprite.tsx';

describe('runs', () => {
  it('merges same-colour pixels of a row into one rect', () => {
    expect(runs(['.##e#.', '#....#'], 3)).toEqual([
      { x: 1, y: 3, w: 2, eye: false },
      { x: 3, y: 3, w: 1, eye: true },
      { x: 4, y: 3, w: 1, eye: false },
      { x: 0, y: 4, w: 1, eye: false },
      { x: 5, y: 4, w: 1, eye: false },
    ]);
  });

  it('every pose is as wide as the sprite and has 4 walk frames', () => {
    expect(LEGS.walk).toHaveLength(4);
    for (const rows of [LEGS.stand, ...LEGS.walk]) for (const r of rows) expect(r).toHaveLength(SPRITE_W);
  });
});
