// Test helper: a fake clock with rAF at a 60 Hz vsync and plain timers.

import type { LoopDeps } from './loop.ts';

/** A fake clock with rAF at a 60 Hz vsync and plain timers. */
export class FakeClock implements LoopDeps {
  t = 0;
  private seq = 0;
  rafs = new Map<number, (t: number) => void>();
  timers = new Map<number, { at: number; cb: () => void }>();
  rafCalls = 0;

  raf = (cb: (t: number) => void) => {
    this.rafCalls++;
    this.rafs.set(++this.seq, cb);
    return this.seq;
  };
  caf = (id: number) => {
    this.rafs.delete(id);
  };
  now = () => this.t;
  setTimeout = (cb: () => void, ms: number) => {
    this.timers.set(++this.seq, { at: this.t + ms, cb });
    return this.seq;
  };
  clearTimeout = (h: unknown) => {
    this.timers.delete(h as number);
  };

  /** Advances time in 1 ms steps, firing timers and, every 16 ms, the rAF callbacks. */
  advance(ms: number): void {
    const end = this.t + ms;
    while (this.t < end) {
      this.t++;
      for (const [id, tm] of [...this.timers]) {
        if (tm.at <= this.t) {
          this.timers.delete(id);
          tm.cb();
        }
      }
      if (this.t % 16 === 0) {
        const cbs = [...this.rafs.values()];
        this.rafs.clear();
        for (const cb of cbs) cb(this.t);
      }
    }
  }
}

