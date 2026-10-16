import { describe, expect, it } from 'vitest';
import { PARAM_KEYS, ambient, arcPoint, easeInOut, lerpParams, phaseOf, poseParams, puffAt } from './anim.ts';
import type { Pose } from './model.ts';

const POSES: Pose[] = ['typing', 'alert', 'sleeping', 'sit', 'puff'];

describe('poseParams', () => {
  it('every pose sets every parameter to a finite number', () => {
    for (const p of POSES) {
      const v = poseParams(p);
      for (const k of PARAM_KEYS) expect(Number.isFinite(v[k]), `${p}.${k}`).toBe(true);
    }
  });
  it.each<[Pose, (v: ReturnType<typeof poseParams>) => boolean]>([
    ['sleeping', (v) => v.lying === 1 && v.eyesOpen === 0],
    ['typing', (v) => v.legL < 0 && v.headRotX > 0],
    ['alert', (v) => v.headRotX < 0 && v.tailPitch < poseParams('sit').tailPitch],
    ['sit', (v) => v.lying === 0 && v.eyesOpen === 1],
    ['puff', (v) => v.eyesOpen < 1],
  ])('%s has its shape', (p, ok) => {
    expect(ok(poseParams(p))).toBe(true);
  });
  it('returns copies', () => {
    const a = poseParams('sit');
    a.bodyY = 99;
    expect(poseParams('sit').bodyY).not.toBe(99);
  });
});

describe('tweens', () => {
  it('lerp clamps and interpolates', () => {
    const a = poseParams('sit');
    const b = poseParams('sleeping');
    const close = (x: typeof a, y: typeof a) => PARAM_KEYS.forEach((k) => expect(x[k]).toBeCloseTo(y[k]));
    close(lerpParams(a, b, 0), a);
    close(lerpParams(a, b, 1), b);
    close(lerpParams(a, b, 2), b);
    expect(lerpParams(a, b, 0.5).lying).toBeCloseTo(0.5);
  });
  it.each([
    [-1, 0],
    [0, 0],
    [0.25, 0.125],
    [0.5, 0.5],
    [1, 1],
    [3, 1],
  ])('easeInOut(%f) = %f', (u, want) => {
    expect(easeInOut(u)).toBeCloseTo(want);
  });
});

describe('ambient', () => {
  it.each<[Pose, keyof ReturnType<typeof poseParams>]>([
    ['typing', 'legL'],
    ['alert', 'tailYaw'],
    ['sleeping', 'bodyScaleY'],
    ['sit', 'tailYaw'],
  ])('%s moves %s over time', (pose, key) => {
    const base = poseParams(pose);
    const vals = new Set([0.1, 0.3, 0.5, 0.7].map((t) => ambient(pose, base, t, 0)[key].toFixed(4)));
    expect(vals.size).toBeGreaterThan(1);
  });
  it('typing paws alternate', () => {
    const p = ambient('typing', poseParams('typing'), 0.12, 0);
    const b = poseParams('typing');
    expect(Math.sign(p.legL - b.legL)).toBe(-Math.sign(p.legR - b.legR));
  });
  it('sitting cats blink now and then', () => {
    const base = poseParams('sit');
    const eyes = Array.from({ length: 200 }, (_, i) => ambient('sit', base, i / 50, 0.3).eyesOpen);
    expect(Math.min(...eyes)).toBeLessThan(0.5);
    expect(eyes.filter((e) => e === 1).length).toBeGreaterThan(150);
  });
  it('does not mutate the base', () => {
    const base = poseParams('alert');
    const copy = { ...base };
    ambient('alert', base, 1, 0);
    expect(base).toEqual(copy);
  });
});

describe('puffAt', () => {
  it('puffs in the middle and settles at both ends', () => {
    const base = poseParams('puff');
    expect(puffAt(base, 0).tailScale).toBeCloseTo(base.tailScale);
    expect(puffAt(base, 0.5).tailScale).toBeCloseTo(base.tailScale + 0.7);
    expect(puffAt(base, 0.5).earsBack).toBeCloseTo(1);
    expect(puffAt(base, 1).tailScale).toBeCloseTo(base.tailScale);
  });
});

describe('arcPoint', () => {
  it('starts at a, ends at b and rises in the middle', () => {
    const a = [0, 1, 0] as const;
    const b = [4, 1, 0] as const;
    expect(arcPoint(a, b, 2, 0)).toEqual([0, 1, 0]);
    expect(arcPoint(a, b, 2, 1)).toEqual([4, 1, 0]);
    const mid = arcPoint(a, b, 2, 0.5);
    expect(mid[0]).toBeCloseTo(2);
    expect(mid[1]).toBeCloseTo(2);
  });
});

describe('phaseOf', () => {
  it('is stable and in range', () => {
    expect(phaseOf('abc')).toBe(phaseOf('abc'));
    expect(phaseOf('abc')).not.toBe(phaseOf('abd'));
    for (const id of ['', 'x', 'd3e0a001-0000-4000-8000-000000000001']) {
      expect(phaseOf(id)).toBeGreaterThanOrEqual(0);
      expect(phaseOf(id)).toBeLessThan(Math.PI * 2);
    }
  });
});
