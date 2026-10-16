// The three.js side of the scene: builds the desk, the stations and the
// cats from a SceneModel and draws only on demand (FrameLoop). The WebGL
// renderer is injected (RendererLike), so everything here also runs under
// Vitest with a fake one; picking and projection are plain three.js math.

import {
  BoxGeometry,
  BufferGeometry,
  Color,
  CylinderGeometry,
  DirectionalLight,
  Group,
  HemisphereLight,
  InstancedMesh,
  Line,
  LineDashedMaterial,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshToonMaterial,
  Object3D,
  OrthographicCamera,
  Raycaster,
  Scene as ThreeScene,
  Vector2,
  Vector3,
} from 'three';
import {
  AMBIENT_FPS,
  BOUT_EVERY_MS,
  BOUT_MS,
  FLIGHT_MS,
  PUFF_MS,
  TWEEN_MS,
  ambient,
  arcPoint,
  easeInOut,
  lerpParams,
  phaseOf,
  poseParams,
  puffAt,
  type CatParams,
} from './anim.ts';
import { CatMaterials, createCat, createKit, type CatKit, type CatRig } from './cat.ts';
import { FrameLoop, type LoopDeps } from './loop.ts';
import { ROW_DEPTH, type ArcModel, type CatModel, type Pose, type SceneModel } from './model.ts';

/** What SceneView needs from a WebGLRenderer. */
export interface RendererLike {
  render(scene: Object3D, camera: OrthographicCamera): void;
  setSize(w: number, h: number, updateStyle?: boolean): void;
  setPixelRatio(r: number): void;
  setClearColor(c: Color | string, alpha?: number): void;
  dispose(): void;
}

export const MAX_DPR = 1.5;

export interface OverlayItem {
  key: string;
  kind: 'bubble' | 'mail' | 'ctx';
  text: string;
  /** CSS px within the canvas */
  x: number;
  y: number;
  /** --nk-* token the item is painted with */
  token: string;
  blink?: boolean;
  dim?: boolean;
}

interface CatEntry {
  model: CatModel;
  rig: CatRig;
  phase: number;
  from: CatParams;
  to: CatParams;
  tweenStart: number;
  puffStart: number;
  shown: CatParams;
}

interface Flight {
  arc: ArcModel;
  start: number;
  mesh: Group;
}

export interface ViewOptions {
  renderer: RendererLike;
  deps: LoopDeps;
  reducedMotion?: boolean;
  dpr?: number;
  /** Called after every drawn frame (debug counters). */
  onFrame?(frames: number): void;
}

const CAMERA_DIR = new Vector3(1.3, 4.2, 8).normalize();

export class SceneView {
  readonly scene = new ThreeScene();
  readonly camera = new OrthographicCamera(-5, 5, 3, -3, 0.1, 100);
  readonly loop: FrameLoop;
  private readonly kit: CatKit;
  private readonly mats: CatMaterials;
  private readonly cats = new Map<string, CatEntry>();
  private readonly flights = new Map<string, Flight>();
  private readonly seenArcs = new Set<string>();
  private readonly world = new Group();
  private readonly deskGroup = new Group();
  private readonly arcGroup = new Group();
  private kittens: InstancedMesh | null = null;
  private kittenEyes: InstancedMesh | null = null;
  private kittenCount = 0;
  private readonly deskMat: MeshToonMaterial;
  private readonly edgeMat: MeshToonMaterial;
  private readonly arcMat: LineDashedMaterial;
  private readonly envMat: MeshBasicMaterial;
  private readonly envFlapMat: MeshBasicMaterial;
  private readonly envGeo = new BoxGeometry(0.34, 0.22, 0.03);
  private readonly flapGeo = new BoxGeometry(0.22, 0.03, 0.035);
  private readonly stationMats = new Map<string, MeshToonMaterial>();
  private readonly raycaster = new Raycaster();
  private model: SceneModel | null = null;
  private width = 1;
  private height = 1;
  private reduced: boolean;
  private boutUntil = -Infinity;
  private boutTimer: unknown = null;
  private visible = true;
  private disposed = false;

  constructor(private readonly opts: ViewOptions) {
    this.reduced = !!opts.reducedMotion;
    this.loop = new FrameLoop((now) => this.draw(now), opts.deps);
    this.kit = createKit();
    this.mats = new CatMaterials(this.kit.ramp);
    this.deskMat = new MeshToonMaterial({ gradientMap: this.kit.ramp });
    this.edgeMat = new MeshToonMaterial({ gradientMap: this.kit.ramp });
    this.arcMat = new LineDashedMaterial({ dashSize: 0.12, gapSize: 0.1, transparent: true, opacity: 0.8 });
    this.envMat = new MeshBasicMaterial();
    this.envFlapMat = new MeshBasicMaterial();
    opts.renderer.setPixelRatio(Math.min(opts.dpr ?? 1, MAX_DPR));

    const hemi = new HemisphereLight(0xffffff, 0x444466, 1.4);
    const sun = new DirectionalLight(0xffffff, 1.8);
    sun.position.set(-3, 8, 6);
    this.scene.add(hemi, sun, this.world);
    this.world.add(this.deskGroup, this.arcGroup);
  }

  get frames(): number {
    return this.loop.frames;
  }

  setSize(w: number, h: number): void {
    this.width = Math.max(1, w);
    this.height = Math.max(1, h);
    this.opts.renderer.setSize(this.width, this.height, false);
    this.fit();
    this.loop.invalidate();
  }

  setReducedMotion(r: boolean): void {
    if (r === this.reduced) return;
    this.reduced = r;
    if (r) {
      this.boutUntil = -Infinity;
      this.clearBout();
      for (const f of this.flights.values()) this.arcGroup.remove(f.mesh);
      this.flights.clear();
    } else {
      this.scheduleBout();
    }
    this.loop.invalidate();
  }

  setVisible(v: boolean): void {
    if (v === this.visible) return;
    this.visible = v;
    this.loop.setVisible(v);
    if (v) this.scheduleBout();
    else this.clearBout();
  }

  /** Applies a new model; draws only if something visible changed. */
  setModel(m: SceneModel): void {
    const prev = this.model;
    this.model = m;
    const now = this.opts.deps.now();
    if (!prev || JSON.stringify(prev.colors) !== JSON.stringify(m.colors)) this.applyColors(m);
    if (!prev || prev.width !== m.width || prev.depth !== m.depth || JSON.stringify(prev.stations) !== JSON.stringify(m.stations)) {
      this.buildDesk(m);
      this.fit();
    }
    let animUntil = now;
    const live = new Set<string>();
    for (const c of m.cats) {
      live.add(c.id);
      let e = this.cats.get(c.id);
      if (!e) {
        const rig = createCat(this.kit, this.mats, c.id);
        const p = poseParams(c.pose);
        e = { model: c, rig, phase: phaseOf(c.id), from: p, to: p, tweenStart: -Infinity, puffStart: -Infinity, shown: p };
        this.cats.set(c.id, e);
        this.world.add(rig.root);
        rig.apply(p);
      } else if (e.model.pose !== c.pose) {
        e.from = e.shown;
        e.to = poseParams(c.pose);
        if (!this.reduced) {
          e.tweenStart = now;
          animUntil = Math.max(animUntil, now + TWEEN_MS);
          if (c.pose === 'puff') {
            e.puffStart = now;
            animUntil = Math.max(animUntil, now + PUFF_MS);
          }
        } else {
          e.shown = e.to;
        }
      }
      e.model = c;
      e.rig.root.position.set(c.x, 0, c.z);
      e.rig.setLook({
        dim: c.dim,
        collar: c.collar,
        collarLit: c.collarLit,
        glow: c.glow !== null,
        focused: c.focused,
        typing: c.pose === 'typing',
      });
    }
    for (const [id, e] of this.cats) {
      if (live.has(id)) continue;
      this.world.remove(e.rig.root);
      this.cats.delete(id);
    }
    this.buildKittens(m);
    animUntil = Math.max(animUntil, this.buildArcs(m, now));
    if (animUntil > now) this.loop.animate(animUntil, 60);
    else this.loop.invalidate();
    if (!prev) this.scheduleBout();
  }

  /** Starts an ambient bout now (e.g. on hover), unless reduced motion. */
  wiggle(): void {
    if (this.reduced || !this.visible || this.cats.size === 0) return;
    const now = this.opts.deps.now();
    this.boutUntil = now + BOUT_MS;
    this.loop.animate(this.boutUntil, AMBIENT_FPS);
  }

  /** Session id of the cat under the normalised device point, or null. */
  pick(ndcX: number, ndcY: number): string | null {
    this.scene.updateMatrixWorld();
    this.raycaster.setFromCamera(new Vector2(ndcX, ndcY), this.camera);
    const hits = this.raycaster.intersectObjects([...this.cats.values()].map((e) => e.rig.hit), false);
    return (hits[0]?.object.userData.sessionId as string | undefined) ?? null;
  }

  /** CSS px of a world point within the canvas. */
  project(x: number, y: number, z: number): { x: number; y: number } {
    this.camera.updateMatrixWorld();
    const v = new Vector3(x, y, z).project(this.camera);
    return { x: ((v.x + 1) / 2) * this.width, y: ((1 - v.y) / 2) * this.height };
  }

  /** Screen position above a cat's head (debug hook, tooltips). */
  catScreen(id: string): { x: number; y: number } | null {
    const e = this.cats.get(id);
    if (!e) return null;
    const head = e.model.pose === 'sleeping' ? 0.5 : 0.8;
    return this.project(e.model.x, head, e.model.z);
  }

  /** DOM overlays: ! / zZ bubbles, ✉ cues and the context labels. */
  overlay(): OverlayItem[] {
    const m = this.model;
    if (!m) return [];
    const out: OverlayItem[] = [];
    for (const s of m.stations) {
      const p = this.project(s.x, 0, s.z + 0.95);
      out.push({ key: `ctx:${s.ctx}`, kind: 'ctx', text: s.ctx || '·', ...p, token: 'subtle' });
    }
    for (const c of m.cats) {
      if (c.bubble) {
        const sleeping = c.pose === 'sleeping';
        const p = this.project(c.x + (sleeping ? 0.45 : 0.15), sleeping ? 0.85 : 1.5, c.z);
        out.push({ key: `b:${c.id}`, kind: 'bubble', text: c.bubble, ...p, token: sleeping ? 'subtle' : 'blocked', dim: c.dim });
      }
      if (c.mail) {
        const p = this.project(c.x - 0.5, 1.2, c.z);
        out.push({ key: `m:${c.id}`, kind: 'mail', text: '✉', ...p, token: 'accent', blink: c.mail.blink, dim: !c.mail.blink });
      }
    }
    return out;
  }

  /** Poses by session id as last applied. */
  poses(): Record<string, Pose> {
    const out: Record<string, Pose> = {};
    for (const [id, e] of this.cats) out[id] = e.model.pose;
    return out;
  }

  get catCount(): number {
    return this.cats.size;
  }

  get kittenTotal(): number {
    return this.kittenCount;
  }

  dispose(): void {
    this.disposed = true;
    this.clearBout();
    this.loop.dispose();
    this.disposeDesk();
    this.kittens?.dispose();
    this.kittenEyes?.dispose();
    this.kit.dispose();
    this.mats.dispose();
    for (const m of [this.deskMat, this.edgeMat, this.arcMat, this.envMat, this.envFlapMat, ...this.stationMats.values()]) m.dispose();
    this.envGeo.dispose();
    this.flapGeo.dispose();
    this.opts.renderer.dispose();
  }

  // ---- internals ----

  private draw(now: number): void {
    const ambientOn = !this.reduced && now < this.boutUntil;
    const t = now / 1000;
    for (const e of this.cats.values()) {
      const pose = e.model.pose;
      let p = e.to;
      const u = (now - e.tweenStart) / TWEEN_MS;
      if (u < 1) p = lerpParams(e.from, e.to, easeInOut(u));
      const pu = (now - e.puffStart) / PUFF_MS;
      if (pu < 1) p = puffAt(p, pu);
      if (ambientOn) p = ambient(pose, p, t, e.phase);
      e.shown = p;
      e.rig.apply(p);
    }
    if (this.kittenCount > 0) this.placeKittens(ambientOn ? t : null);
    for (const [k, f] of this.flights) {
      const u = (now - f.start) / FLIGHT_MS;
      const a = this.cats.get(f.arc.from)?.model;
      const b = this.cats.get(f.arc.to)?.model;
      if (u >= 1 || !a || !b) {
        this.arcGroup.remove(f.mesh);
        this.flights.delete(k);
        continue;
      }
      const [x, y, z] = arcPoint([a.x, 1.1, a.z], [b.x, 1.1, b.z], 1.4, easeInOut(u));
      f.mesh.position.set(x, y, z);
      f.mesh.rotation.z = Math.sin(u * Math.PI * 4) * 0.3;
    }
    this.opts.renderer.render(this.scene, this.camera);
    this.opts.onFrame?.(this.loop.frames);
  }

  private applyColors(m: SceneModel): void {
    const c = m.colors;
    this.opts.renderer.setClearColor(c.bg, 1);
    this.mats.setColors(c);
    this.deskMat.color.set(c.desk);
    this.edgeMat.color.set(c.deskEdge);
    this.arcMat.color.set(c.accent);
    this.envMat.color.set(c.text);
    this.envFlapMat.color.set(c.accent);
    for (const mat of this.stationMats.values()) mat.dispose();
    this.stationMats.clear();
    if (this.model) this.buildDesk(this.model);
  }

  private disposeDesk(): void {
    for (const o of [...this.deskGroup.children]) {
      this.deskGroup.remove(o);
      (o as Mesh).geometry?.dispose();
    }
  }

  private buildDesk(m: SceneModel): void {
    this.disposeDesk();
    const w = Math.max(m.width, 3) + 1.4;
    const d = m.depth + 0.6;
    const top = new Mesh(new BoxGeometry(w, 0.3, d), this.deskMat);
    top.position.set(0, -0.15, 1.3 - d / 2);
    const lip = new Mesh(new BoxGeometry(w + 0.1, 0.12, 0.16), this.edgeMat);
    lip.position.set(0, -0.06, 1.3 + 0.02);
    this.deskGroup.add(top, lip);
    for (const s of m.stations) {
      let mat = this.stationMats.get(s.color);
      if (!mat) {
        mat = new MeshToonMaterial({ gradientMap: this.kit.ramp, color: new Color(s.color).lerp(new Color(m.colors.desk), 0.45) });
        this.stationMats.set(s.color, mat);
      }
      const cushion = new Mesh(new CylinderGeometry(0.5, 0.55, 0.08, 8), mat);
      cushion.scale.set(s.width, 1, 1.7);
      cushion.position.set(s.x, 0.04, s.z + 0.15);
      this.deskGroup.add(cushion);
    }
  }

  private buildKittens(m: SceneModel): void {
    const n = m.cats.reduce((k, c) => k + c.kittens.length, 0);
    if (n > (this.kittens?.count ?? 0) || !this.kittens) {
      const cap = Math.max(4, n);
      for (const im of [this.kittens, this.kittenEyes]) if (im) this.world.remove(im);
      this.kittens?.dispose();
      this.kittenEyes?.dispose();
      this.kittens = new InstancedMesh(this.kit.kitten, this.mats.body, cap);
      this.kittenEyes = new InstancedMesh(this.kit.kittenEyes, this.mats.eyes, cap);
      this.kittens.name = 'kittens';
      this.world.add(this.kittens, this.kittenEyes);
    }
    this.kittenCount = n;
    this.placeKittens(null);
  }

  private placeKittens(t: number | null): void {
    const km = this.kittens!;
    const ke = this.kittenEyes!;
    const mtx = new Matrix4();
    let i = 0;
    for (const c of this.model?.cats ?? []) {
      for (const k of c.kittens) {
        const hop = t === null ? 0 : 0.06 * Math.abs(Math.sin(t * 7 + i));
        mtx.makeScale(0.5, 0.5, 0.5).setPosition(k.x, hop, k.z);
        km.setMatrixAt(i, mtx);
        ke.setMatrixAt(i, mtx);
        i++;
      }
    }
    km.count = i;
    ke.count = i;
    km.instanceMatrix.needsUpdate = true;
    ke.instanceMatrix.needsUpdate = true;
  }

  /** Dashed arcs for blinking ✉ and a flying envelope for new ones; returns until when to animate. */
  private buildArcs(m: SceneModel, now: number): number {
    for (const o of [...this.arcGroup.children]) {
      if (o instanceof Line) {
        this.arcGroup.remove(o);
        o.geometry.dispose();
      }
    }
    let until = now;
    const pos = new Map(m.cats.map((c) => [c.id, c]));
    for (const a of m.arcs) {
      const from = pos.get(a.from);
      const to = pos.get(a.to);
      if (!from || !to) continue;
      if (a.blink) {
        const pts: Vector3[] = [];
        for (let i = 0; i <= 24; i++) pts.push(new Vector3(...arcPoint([from.x, 1.1, from.z], [to.x, 1.1, to.z], 1.4, i / 24)));
        const line = new Line(new BufferGeometry().setFromPoints(pts), this.arcMat);
        line.computeLineDistances();
        this.arcGroup.add(line);
      }
      const key = `${a.from}>${a.to}@${a.at}`;
      if (a.blink && !this.seenArcs.has(key)) {
        this.seenArcs.add(key);
        if (!this.reduced) {
          const env = new Group();
          const flap = new Mesh(this.flapGeo, this.envFlapMat);
          flap.position.set(0, 0.04, 0.02);
          env.add(new Mesh(this.envGeo, this.envMat), flap);
          this.arcGroup.add(env);
          this.flights.set(key, { arc: a, start: now, mesh: env });
          until = Math.max(until, now + FLIGHT_MS);
        }
      }
    }
    return until;
  }

  /** Ortho frustum that frames the desk with a margin, keeping the aspect. */
  private fit(): void {
    const m = this.model;
    const w = Math.max(m?.width ?? 4, 4) + 2;
    const depth = m?.depth ?? ROW_DEPTH;
    const target = new Vector3(0, 0.5, 1.3 - depth / 2);
    this.camera.position.copy(target).addScaledVector(CAMERA_DIR, 20);
    this.camera.lookAt(target);
    this.camera.updateMatrixWorld();
    const inv = this.camera.matrixWorldInverse;
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const x of [-w / 2, w / 2])
      for (const y of [-0.3, 1.9])
        for (const z of [1.6, 1.3 - depth - 0.2]) {
          const v = new Vector3(x, y, z).applyMatrix4(inv);
          minX = Math.min(minX, v.x);
          maxX = Math.max(maxX, v.x);
          minY = Math.min(minY, v.y);
          maxY = Math.max(maxY, v.y);
        }
    let halfW = (maxX - minX) / 2;
    let halfH = (maxY - minY) / 2;
    const aspect = this.width / this.height;
    if (halfW / halfH > aspect) halfH = halfW / aspect;
    else halfW = halfH * aspect;
    const cx = (maxX + minX) / 2;
    const cy = (maxY + minY) / 2;
    this.camera.left = cx - halfW;
    this.camera.right = cx + halfW;
    this.camera.top = cy + halfH;
    this.camera.bottom = cy - halfH;
    this.camera.updateProjectionMatrix();
  }

  private clearBout(): void {
    if (this.boutTimer !== null) this.opts.deps.clearTimeout(this.boutTimer);
    this.boutTimer = null;
  }

  /** Ambient motion: a short 10 fps bout every BOUT_EVERY_MS, nothing in between. */
  private scheduleBout(): void {
    this.clearBout();
    if (this.reduced || !this.visible || this.disposed) return;
    this.boutTimer = this.opts.deps.setTimeout(() => {
      this.boutTimer = null;
      this.wiggle();
      this.scheduleBout();
    }, BOUT_EVERY_MS);
  }
}
