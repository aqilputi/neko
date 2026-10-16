// The procedural toon cat (plan §4.8): icosphere head, cone ears, capsule
// body, cylinder legs, a three-segment tail; MeshToonMaterial with a
// 3-band ramp. No assets, no licences, colours straight from the theme.
// Kittens are one merged geometry drawn through InstancedMesh.

import {
  BoxGeometry,
  BufferGeometry,
  CapsuleGeometry,
  Color,
  ConeGeometry,
  CylinderGeometry,
  DataTexture,
  Group,
  IcosahedronGeometry,
  Material,
  Mesh,
  MeshBasicMaterial,
  MeshToonMaterial,
  NearestFilter,
  RedFormat,
  RingGeometry,
  SphereGeometry,
  TorusGeometry,
} from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import type { CatParams } from './anim.ts';

/** 3-band toon ramp (dark, mid, lit). */
export function toonRamp(): DataTexture {
  const t = new DataTexture(new Uint8Array([70, 150, 255]), 3, 1, RedFormat);
  t.minFilter = NearestFilter;
  t.magFilter = NearestFilter;
  t.generateMipmaps = false;
  t.needsUpdate = true;
  return t;
}

/** Geometries shared by every cat (built once per renderer). */
export interface CatKit {
  body: BufferGeometry;
  haunch: BufferGeometry;
  head: BufferGeometry;
  ear: BufferGeometry;
  eye: BufferGeometry;
  pupil: BufferGeometry;
  leg: BufferGeometry;
  paw: BufferGeometry;
  tailSeg: BufferGeometry;
  collar: BufferGeometry;
  ring: BufferGeometry;
  keyboard: BufferGeometry;
  hit: BufferGeometry;
  kitten: BufferGeometry;
  kittenEyes: BufferGeometry;
  ramp: DataTexture;
  dispose(): void;
}

export const TAIL_SEG = 0.2;

export function createKit(): CatKit {
  const body = new CapsuleGeometry(0.27, 0.32, 3, 8);
  const haunch = new IcosahedronGeometry(0.17, 1);
  const head = new IcosahedronGeometry(0.3, 1);
  const ear = new ConeGeometry(0.1, 0.2, 4);
  const eye = new SphereGeometry(0.065, 8, 6);
  const pupil = new BoxGeometry(0.022, 0.1, 0.02);
  const leg = new CylinderGeometry(0.05, 0.055, 0.36, 6);
  leg.translate(0, -0.18, 0);
  const paw = new IcosahedronGeometry(0.065, 0);
  const tailSeg = new CylinderGeometry(0.045, 0.055, TAIL_SEG, 6);
  tailSeg.translate(0, TAIL_SEG / 2, 0);
  const collar = new TorusGeometry(0.2, 0.045, 5, 14);
  const ring = new RingGeometry(0.34, 0.5, 14);
  ring.rotateX(-Math.PI / 2);
  const keyboard = new BoxGeometry(0.5, 0.05, 0.2);
  const hit = new BoxGeometry(1.1, 1.3, 1.1);
  hit.translate(0, 0.62, 0);
  const { kitten, kittenEyes } = kittenGeometry();
  const ramp = toonRamp();
  const all = [body, haunch, head, ear, eye, pupil, leg, paw, tailSeg, collar, ring, keyboard, hit, kitten, kittenEyes];
  return {
    body,
    haunch,
    head,
    ear,
    eye,
    pupil,
    leg,
    paw,
    tailSeg,
    collar,
    ring,
    keyboard,
    hit,
    kitten,
    kittenEyes,
    ramp,
    dispose() {
      for (const g of all) g.dispose();
      ramp.dispose();
    },
  };
}

/** A sitting kitten as one geometry (+ its eyes as another), unit scale. */
export function kittenGeometry(): { kitten: BufferGeometry; kittenEyes: BufferGeometry } {
  const parts: BufferGeometry[] = [];
  const body = new CapsuleGeometry(0.27, 0.3, 2, 7);
  body.rotateX(-0.3);
  body.translate(0, 0.42, -0.04);
  parts.push(body);
  const head = new IcosahedronGeometry(0.3, 1);
  head.translate(0, 0.9, 0.08);
  parts.push(head);
  for (const s of [-1, 1]) {
    const ear = new ConeGeometry(0.1, 0.2, 4);
    ear.rotateZ(-s * 0.3);
    ear.translate(s * 0.16, 1.16, 0.06);
    parts.push(ear);
  }
  const tail = new CylinderGeometry(0.05, 0.05, 0.5, 5);
  tail.rotateZ(Math.PI / 2.4);
  tail.translate(0.3, 0.1, -0.2);
  parts.push(tail);
  const kitten = mergeGeometries(parts.map((g) => (g.index ? g.toNonIndexed() : g)));
  const eyes: BufferGeometry[] = [];
  for (const s of [-1, 1]) {
    const e = new SphereGeometry(0.07, 6, 5);
    e.scale(1, 1.1, 0.5);
    e.translate(s * 0.11, 0.93, 0.36);
    eyes.push(e);
  }
  const kittenEyes = mergeGeometries(eyes);
  for (const g of [...parts, ...eyes]) g.dispose();
  return { kitten: kitten!, kittenEyes: kittenEyes! };
}

/** Materials shared by all cats; collars get one material per colour. */
export class CatMaterials {
  readonly body: MeshToonMaterial;
  readonly bodyDim: MeshToonMaterial;
  readonly eyes: MeshBasicMaterial;
  readonly eyesDim: MeshBasicMaterial;
  readonly keyboard: MeshToonMaterial;
  readonly glow: MeshBasicMaterial;
  readonly focus: MeshBasicMaterial;
  readonly hit: MeshBasicMaterial;
  private collars = new Map<string, MeshToonMaterial>();

  constructor(private readonly ramp: DataTexture) {
    this.body = new MeshToonMaterial({ gradientMap: ramp });
    this.bodyDim = new MeshToonMaterial({ gradientMap: ramp, transparent: true, opacity: 0.55 });
    this.eyes = new MeshBasicMaterial();
    this.eyesDim = new MeshBasicMaterial();
    this.keyboard = new MeshToonMaterial({ gradientMap: ramp });
    this.glow = new MeshBasicMaterial({ transparent: true, opacity: 0.55, depthWrite: false });
    this.focus = new MeshBasicMaterial({ transparent: true, opacity: 0.7, depthWrite: false });
    this.hit = new MeshBasicMaterial({ visible: false });
  }

  setColors(c: { body: string; eyes: string; bg: string; muted: string; busy: string; accent: string; deskEdge: string }): void {
    this.body.color.set(c.body);
    this.bodyDim.color.set(c.body).lerp(new Color(c.muted), 0.25);
    this.eyes.color.set(c.eyes);
    this.eyesDim.color.set(c.eyes).lerp(new Color(c.bg), 0.55);
    this.glow.color.set(c.busy);
    this.focus.color.set(c.accent);
    this.keyboard.color.set(c.deskEdge);
  }

  /** The collar material for a colour, lit (emissive) or not. */
  collar(color: string, lit: boolean): MeshToonMaterial {
    const key = `${color}|${lit ? 1 : 0}`;
    let m = this.collars.get(key);
    if (!m) {
      m = new MeshToonMaterial({ gradientMap: this.ramp, color });
      if (lit) {
        m.emissive.set(color);
        m.emissiveIntensity = 0.85;
      }
      this.collars.set(key, m);
    }
    return m;
  }

  dispose(): void {
    const all: Material[] = [this.body, this.bodyDim, this.eyes, this.eyesDim, this.keyboard, this.glow, this.focus, this.hit, ...this.collars.values()];
    for (const m of all) m.dispose();
    this.collars.clear();
  }
}

export interface CatRig {
  root: Group;
  hit: Mesh;
  apply(p: CatParams): void;
  setLook(o: { dim: boolean; collar: string | null; collarLit: boolean; glow: boolean; focused: boolean; typing: boolean }): void;
}

/** One cat, facing +z, standing on y = 0 at the root's origin. */
export function createCat(kit: CatKit, mats: CatMaterials, id: string): CatRig {
  const root = new Group();
  root.name = `cat:${id}`;
  const mesh = (g: BufferGeometry, m: Material) => new Mesh(g, m);

  const body = mesh(kit.body, mats.body);
  const haunches = [-1, 1].map((s) => {
    const h = mesh(kit.haunch, mats.body);
    h.position.set(s * 0.17, 0.17, -0.08);
    h.scale.set(0.85, 1, 1.15);
    return h;
  });

  const head = new Group();
  const skull = mesh(kit.head, mats.body);
  skull.scale.set(1, 0.88, 0.92);
  head.add(skull);
  const ears = [-1, 1].map((s) => {
    const pivot = new Group();
    pivot.position.set(s * 0.15, 0.2, 0);
    pivot.rotation.z = -s * 0.3;
    const e = mesh(kit.ear, mats.body);
    e.position.y = 0.08;
    pivot.add(e);
    head.add(pivot);
    return pivot;
  });
  const eyes = [-1, 1].map((s) => {
    const e = mesh(kit.eye, mats.eyes);
    e.position.set(s * 0.11, 0.03, 0.245);
    const pupil = mesh(kit.pupil, mats.body);
    pupil.position.z = 0.05;
    e.add(pupil);
    head.add(e);
    return e;
  });
  const collar = mesh(kit.collar, mats.body);
  collar.position.set(0, -0.26, -0.03);
  collar.rotation.x = Math.PI / 2 - 0.25;
  head.add(collar);

  const legs = [-1, 1].map((s) => {
    const pivot = new Group();
    pivot.position.set(s * 0.11, 0.36, 0.13);
    pivot.add(mesh(kit.leg, mats.body));
    const paw = mesh(kit.paw, mats.body);
    paw.position.set(0, -0.35, 0.03);
    pivot.add(paw);
    return pivot;
  });

  const tail = new Group();
  tail.position.set(0, 0.1, -0.26);
  tail.rotation.order = 'YXZ';
  const segs: Mesh[] = [];
  let parent: Group | Mesh = tail;
  for (let i = 0; i < 3; i++) {
    const seg = mesh(kit.tailSeg, mats.body);
    if (i > 0) seg.position.y = 0.2;
    parent.add(seg);
    segs.push(seg);
    parent = seg;
  }

  const glow = mesh(kit.ring, mats.glow);
  glow.position.y = 0.095;
  const focus = mesh(kit.ring, mats.focus);
  focus.position.y = 0.09;
  focus.scale.setScalar(1.25);
  const keyboard = mesh(kit.keyboard, mats.keyboard);
  keyboard.position.set(0, 0.105, 0.45);
  const hit = mesh(kit.hit, mats.hit);
  hit.userData.sessionId = id;

  root.add(body, ...haunches, head, ...legs, tail, glow, focus, keyboard, hit);
  const furred = [body, skull, ...haunches, ...segs, ...legs.flatMap((l) => l.children as Mesh[]), ...ears.map((e) => e.children[0] as Mesh)];

  return {
    root,
    hit,
    apply(p) {
      const lie = p.lying;
      body.position.set(0, p.bodyY, -0.05 * (1 - lie));
      body.rotation.set(p.bodyRotX, 0, (lie * Math.PI) / 2);
      body.scale.set(1, p.bodyScaleY, 1 + 0.15 * lie);
      for (const h of haunches) h.visible = lie < 0.6;
      head.position.set(p.headX, p.headY, p.headZ);
      head.rotation.set(p.headRotX, 0, p.headRotZ);
      for (const e of ears) e.rotation.x = -1.1 * p.earsBack;
      for (const e of eyes) e.scale.set(1, Math.max(0.12, p.eyesOpen), 0.55);
      legs[0]!.rotation.x = p.legL;
      legs[1]!.rotation.x = p.legR;
      for (const l of legs) l.scale.setScalar(Math.max(0.01, p.legScale));
      tail.rotation.set(-p.tailPitch, p.tailYaw, 0);
      for (const [i, s] of segs.entries()) {
        if (i > 0) s.rotation.z = p.tailCurl * 0.7;
        s.scale.set(p.tailScale, 1, p.tailScale);
      }
    },
    setLook(o) {
      const m = o.dim ? mats.bodyDim : mats.body;
      for (const f of furred) f.material = m;
      for (const e of eyes) e.material = o.dim ? mats.eyesDim : mats.eyes;
      collar.visible = o.collar !== null;
      if (o.collar) collar.material = mats.collar(o.collar, o.collarLit);
      glow.visible = o.glow;
      focus.visible = o.focused;
      keyboard.visible = o.typing;
    },
  };
}
