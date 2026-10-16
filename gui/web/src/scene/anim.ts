// Cat motion as plain numbers: each pose is a set of joint parameters, the
// renderer only copies them onto the rig. Pure, so the poses, the tweens
// and the ambient wiggles are testable without WebGL.

import type { Pose } from './model.ts';

export interface CatParams {
  /** 0 sitting … 1 curled up asleep */
  lying: number;
  bodyY: number;
  bodyRotX: number;
  bodyScaleY: number;
  headX: number;
  headY: number;
  headZ: number;
  headRotX: number;
  headRotZ: number;
  /** ears flattened back (0 perked … 1 flat) */
  earsBack: number;
  /** 0 closed … 1 open */
  eyesOpen: number;
  legL: number;
  legR: number;
  legScale: number;
  tailYaw: number;
  tailPitch: number;
  tailCurl: number;
  tailScale: number;
}

const SIT: CatParams = {
  lying: 0,
  bodyY: 0.42,
  bodyRotX: -0.32,
  bodyScaleY: 1,
  headX: 0,
  headY: 0.9,
  headZ: 0.1,
  headRotX: 0,
  headRotZ: 0,
  earsBack: 0,
  eyesOpen: 1,
  legL: 0,
  legR: 0,
  legScale: 1,
  tailYaw: 0.7,
  tailPitch: 1.35,
  tailCurl: 0.55,
  tailScale: 1,
};

const POSES: Record<Pose, CatParams> = {
  sit: SIT,
  typing: {
    ...SIT,
    bodyRotX: -0.12,
    headY: 0.86,
    headZ: 0.2,
    headRotX: 0.3,
    legL: -0.55,
    legR: -0.55,
    tailYaw: 0.9,
  },
  alert: {
    ...SIT,
    bodyRotX: -0.42,
    headY: 0.97,
    headZ: 0.05,
    headRotX: -0.18,
    tailYaw: 0,
    tailPitch: 0.35,
    tailCurl: 0.35,
  },
  sleeping: {
    ...SIT,
    lying: 1,
    bodyY: 0.24,
    bodyRotX: 0,
    headX: 0.34,
    headY: 0.32,
    headZ: 0.18,
    headRotX: 0.35,
    headRotZ: 0.35,
    eyesOpen: 0,
    legScale: 0.01,
    tailYaw: 2.5,
    tailPitch: 1.5,
    tailCurl: 0.9,
  },
  // the settled state after the one-shot puff (puffAt) — a quiet, sleepy sit
  puff: { ...SIT, eyesOpen: 0.55, earsBack: 0.25, tailYaw: 0.4 },
};

export function poseParams(p: Pose): CatParams {
  return { ...POSES[p] };
}

export const PARAM_KEYS = Object.keys(SIT) as (keyof CatParams)[];

export function lerpParams(a: CatParams, b: CatParams, u: number): CatParams {
  const k = Math.min(1, Math.max(0, u));
  const out = { ...a };
  for (const key of PARAM_KEYS) out[key] = a[key] + (b[key] - a[key]) * k;
  return out;
}

export const easeInOut = (u: number): number => {
  const k = Math.min(1, Math.max(0, u));
  return k < 0.5 ? 2 * k * k : 1 - (-2 * k + 2) ** 2 / 2;
};

/** Small, cheap motion on top of a pose (t in seconds, phase per cat). */
export function ambient(pose: Pose, base: CatParams, t: number, phase: number): CatParams {
  const p = { ...base };
  switch (pose) {
    case 'typing':
      p.legL = base.legL + 0.28 * Math.sin(t * 13 + phase);
      p.legR = base.legR + 0.28 * Math.sin(t * 13 + phase + Math.PI);
      p.headRotX = base.headRotX + 0.05 * Math.sin(t * 6.5 + phase);
      break;
    case 'alert':
      p.tailYaw = base.tailYaw + 0.55 * Math.sin(t * 6 + phase);
      break;
    case 'sleeping':
      p.bodyScaleY = 1 + 0.04 * Math.sin(t * 2.4 + phase);
      break;
    default: {
      // sit / settled puff: a slow tail sway and a blink
      p.tailYaw = base.tailYaw + 0.15 * Math.sin(t * 1.8 + phase);
      const f = (t * 0.7 + phase) % 1;
      if (f < 0.1) p.eyesOpen = Math.min(base.eyesOpen, Math.abs(f - 0.05) * 20);
    }
  }
  return p;
}

/** The one-shot fur puff (u: 0 → 1 over PUFF_MS). */
export function puffAt(base: CatParams, u: number): CatParams {
  const k = Math.sin(Math.PI * Math.min(1, Math.max(0, u)));
  return {
    ...base,
    tailScale: base.tailScale + 0.7 * k,
    earsBack: base.earsBack + (1 - base.earsBack) * k,
    tailPitch: base.tailPitch - 1.0 * k,
    bodyScaleY: base.bodyScaleY + 0.08 * k,
  };
}

export const TWEEN_MS = 450;
export const PUFF_MS = 700;
export const FLIGHT_MS = 1400;
/** Ambient motion runs in short bouts so an idle scene really idles. */
export const BOUT_MS = 1600;
export const BOUT_EVERY_MS = 9000;
export const AMBIENT_FPS = 10;

/** Point on the quadratic arc from a to b (lifted by h at the middle). */
export function arcPoint(
  a: readonly [number, number, number],
  b: readonly [number, number, number],
  h: number,
  u: number,
): [number, number, number] {
  const k = Math.min(1, Math.max(0, u));
  const c = [(a[0] + b[0]) / 2, Math.max(a[1], b[1]) + h, (a[2] + b[2]) / 2];
  const m = (i: number) => (1 - k) * (1 - k) * a[i]! + 2 * (1 - k) * k * c[i]! + k * k * b[i]!;
  return [m(0), m(1), m(2)];
}

/** Stable per-cat phase in [0, 2π) so cats don't move in lockstep. */
export function phaseOf(id: string): number {
  let h = 7;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return ((h % 1000) / 1000) * Math.PI * 2;
}
