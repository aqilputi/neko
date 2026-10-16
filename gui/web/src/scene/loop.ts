// Render on demand (plan §4.7): no frame unless something asked for one.
// invalidate() → exactly one frame; animate(until, fps) → frames until
// `until`, at most `fps` per second (ambient motion runs at 10, a tween or
// an envelope flight at 60). Hidden page → nothing at all. The clock,
// requestAnimationFrame and timers are injected so the tests can prove
// that an idle scene draws zero frames.

export interface LoopDeps {
  raf(cb: (t: number) => void): number;
  caf(id: number): void;
  now(): number;
  setTimeout(cb: () => void, ms: number): unknown;
  clearTimeout(h: unknown): void;
}

export function browserDeps(): LoopDeps {
  return {
    raf: (cb) => requestAnimationFrame(cb),
    caf: (id) => cancelAnimationFrame(id),
    now: () => performance.now(),
    setTimeout: (cb, ms) => setTimeout(cb, ms),
    clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
  };
}

/** A frame due within about one vsync is simply left to the next rAF. */
const VSYNC_MS = 17;

export class FrameLoop {
  /** Frames drawn so far. */
  frames = 0;
  private dirty = false;
  private anims: { until: number; fps: number }[] = [];
  private rafId: number | null = null;
  private timer: unknown = null;
  private last = -Infinity;
  private visible = true;
  private disposed = false;

  constructor(
    private readonly draw: (now: number) => void,
    private readonly deps: LoopDeps,
  ) {}

  /** One frame as soon as possible. */
  invalidate(): void {
    this.dirty = true;
    this.schedule();
  }

  /** Keep drawing until `until` (deps.now() clock) at ≤ fps. */
  animate(until: number, fps = 60): void {
    if (until <= this.deps.now()) return this.invalidate();
    this.anims.push({ until, fps });
    this.schedule();
  }

  /** True while a frame or a timer is pending. */
  get busy(): boolean {
    return this.rafId !== null || this.timer !== null;
  }

  /** The latest time an animation still wants frames until, or null. */
  get activeUntil(): number | null {
    const now = this.deps.now();
    const live = this.anims.filter((a) => a.until > now);
    return live.length ? Math.max(...live.map((a) => a.until)) : null;
  }

  setVisible(v: boolean): void {
    if (v === this.visible) return;
    this.visible = v;
    if (!v) return this.cancel();
    this.invalidate(); // redraw what changed while hidden
  }

  dispose(): void {
    this.disposed = true;
    this.cancel();
  }

  private cancel(): void {
    if (this.rafId !== null) this.deps.caf(this.rafId);
    if (this.timer !== null) this.deps.clearTimeout(this.timer);
    this.rafId = null;
    this.timer = null;
  }

  private schedule(): void {
    if (this.disposed || !this.visible || this.rafId !== null) return;
    if (this.dirty) {
      if (this.timer !== null) {
        this.deps.clearTimeout(this.timer);
        this.timer = null;
      }
      this.rafId = this.deps.raf(() => this.frame());
      return;
    }
    if (this.timer !== null) return;
    const now = this.deps.now();
    this.anims = this.anims.filter((a) => a.until > this.last || a.until > now);
    if (this.anims.length === 0) return;
    const fps = Math.max(...this.anims.map((a) => a.fps));
    const due = this.last + 1000 / fps;
    if (due - now <= VSYNC_MS) {
      this.rafId = this.deps.raf(() => this.frame());
    } else {
      this.timer = this.deps.setTimeout(() => {
        this.timer = null;
        this.schedule();
      }, due - now - VSYNC_MS / 2);
    }
  }

  private frame(): void {
    this.rafId = null;
    if (this.disposed || !this.visible) return;
    const now = this.deps.now();
    this.dirty = false;
    this.last = now;
    this.frames++;
    this.draw(now);
    // an animation is done once a frame was drawn at/after its end
    this.anims = this.anims.filter((a) => a.until > now);
    this.schedule();
  }
}
