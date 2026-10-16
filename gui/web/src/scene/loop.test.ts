import { describe, expect, it } from 'vitest';
import { FakeClock } from './fake-clock.ts';
import { FrameLoop } from './loop.ts';

function setup() {
  const clock = new FakeClock();
  const drawn: number[] = [];
  const loop = new FrameLoop((t) => drawn.push(t), clock);
  return { clock, drawn, loop };
}

describe('FrameLoop', () => {
  it('draws nothing while idle', () => {
    const { clock, drawn, loop } = setup();
    clock.advance(10_000);
    expect(drawn).toEqual([]);
    expect(loop.frames).toBe(0);
    expect(clock.rafCalls).toBe(0);
    expect(loop.busy).toBe(false);
  });

  it('invalidate → exactly one frame, however many calls', () => {
    const { clock, drawn, loop } = setup();
    loop.invalidate();
    loop.invalidate();
    loop.invalidate();
    clock.advance(5000);
    expect(drawn.length).toBe(1);
    expect(loop.frames).toBe(1);
    expect(loop.busy).toBe(false);
  });

  it('animate at 60 fps draws every vsync until the end, then stops', () => {
    const { clock, drawn, loop } = setup();
    loop.animate(500, 60);
    clock.advance(2000);
    // ~500 ms / 16 ms vsync, plus the frame at/after the end
    expect(drawn.length).toBeGreaterThanOrEqual(28);
    expect(drawn.length).toBeLessThanOrEqual(34);
    expect(drawn.at(-1)!).toBeGreaterThanOrEqual(500);
    const after = loop.frames;
    clock.advance(5000);
    expect(loop.frames).toBe(after);
    expect(loop.activeUntil).toBeNull();
  });

  it('ambient at 10 fps is capped: ~10 frames per second, not 60', () => {
    const { clock, drawn, loop } = setup();
    loop.animate(1000, 10);
    clock.advance(1500);
    expect(drawn.length).toBeGreaterThanOrEqual(9);
    expect(drawn.length).toBeLessThanOrEqual(12);
    for (let i = 1; i < drawn.length; i++) expect(drawn[i]! - drawn[i - 1]!).toBeGreaterThanOrEqual(96);
  });

  it('the fastest active animation sets the pace', () => {
    const { clock, drawn, loop } = setup();
    loop.animate(2000, 10);
    loop.animate(300, 60);
    clock.advance(300);
    const fast = drawn.length;
    expect(fast).toBeGreaterThanOrEqual(15);
    clock.advance(1000);
    expect(drawn.length - fast).toBeLessThanOrEqual(11);
    expect(loop.activeUntil).toBe(2000);
  });

  it('invalidate during a slow animation draws right away', () => {
    const { clock, drawn, loop } = setup();
    loop.animate(5000, 10);
    clock.advance(20); // first frame at 16
    expect(drawn).toEqual([16]);
    loop.invalidate();
    clock.advance(16);
    expect(drawn).toEqual([16, 32]);
  });

  it('animate with a past end is just an invalidate', () => {
    const { clock, drawn, loop } = setup();
    clock.advance(100);
    loop.animate(50);
    clock.advance(1000);
    expect(drawn.length).toBe(1);
  });

  it('hidden: no frames at all; visible again: one catch-up frame', () => {
    const { clock, drawn, loop } = setup();
    loop.animate(10_000, 60);
    clock.advance(100);
    const before = drawn.length;
    loop.setVisible(false);
    loop.invalidate();
    clock.advance(3000);
    expect(drawn.length).toBe(before);
    expect(loop.busy).toBe(false);
    loop.setVisible(false); // no-op
    loop.setVisible(true);
    clock.advance(16);
    expect(drawn.length).toBe(before + 1);
  });

  it('dispose cancels pending frames and timers', () => {
    const { clock, drawn, loop } = setup();
    loop.animate(1000, 10);
    clock.advance(20);
    loop.dispose();
    clock.advance(2000);
    expect(drawn.length).toBe(1);
    loop.invalidate();
    clock.advance(100);
    expect(drawn.length).toBe(1);
  });
});
