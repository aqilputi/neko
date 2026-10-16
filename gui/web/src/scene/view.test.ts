import { describe, expect, it } from 'vitest';
import type { Session, Snapshot } from '../client/types.ts';
import { FALLBACK_THEME } from '../state/theme.ts';
import { snapshot } from '../test/fixtures.ts';
import { BOUT_EVERY_MS, BOUT_MS, FLIGHT_MS, TWEEN_MS } from './anim.ts';
import { FakeClock } from './fake-clock.ts';
import { buildScene } from './model.ts';
import { MAX_DPR, SceneView, type RendererLike } from './view.ts';

const NOW = Date.parse('2026-10-16T19:00:00Z');

class FakeRenderer implements RendererLike {
  renders = 0;
  dpr = 0;
  size: [number, number] = [0, 0];
  clear = '';
  disposed = false;
  render() {
    this.renders++;
  }
  setSize(w: number, h: number) {
    this.size = [w, h];
  }
  setPixelRatio(r: number) {
    this.dpr = r;
  }
  setClearColor(c: unknown) {
    this.clear = String(c);
  }
  dispose() {
    this.disposed = true;
  }
}

function sess(id: string, over: Partial<Session> = {}): Session {
  return { session_id: id, label: id, cwd: '/x', kind: 'background', state: 'idle', attention: 0, cost: 0, context: 'c', ...over };
}

function world(sessions: Session[], over: Partial<Snapshot> = {}): Snapshot {
  return { ...snapshot(), sessions, groups: {}, mail: {}, ...over };
}

function setup(opts: { reduced?: boolean; dpr?: number } = {}) {
  const clock = new FakeClock();
  const renderer = new FakeRenderer();
  const view = new SceneView({ renderer, deps: clock, reducedMotion: opts.reduced, dpr: opts.dpr ?? 2 });
  view.setSize(800, 500);
  return { clock, renderer, view };
}

const model = (s: Snapshot, now = NOW, focus = {}) => buildScene(s, [], FALLBACK_THEME, now, focus);

describe('SceneView', () => {
  it('clamps the device pixel ratio', () => {
    expect(setup({ dpr: 3 }).renderer.dpr).toBe(MAX_DPR);
    expect(setup({ dpr: 1 }).renderer.dpr).toBe(1);
  });

  it('builds one cat per session, with kittens instanced, and draws once', () => {
    const { clock, renderer, view } = setup();
    const s = world([
      sess('a', { state: 'busy', subs: [{ id: 'k1' }, { id: 'k2' }, { id: 'k3', done: true }] }),
      sess('b', { state: 'blocked', attention: 1 }),
      sess('c', { state: 'done', context: 'd' }),
    ]);
    view.setModel(model(s));
    clock.advance(100);
    expect(view.catCount).toBe(3);
    expect(view.kittenTotal).toBe(2);
    expect(view.poses()).toEqual({ a: 'typing', b: 'alert', c: 'sleeping' });
    expect(renderer.renders).toBe(1);
    expect(view.frames).toBe(1);
    expect(renderer.clear).not.toBe('');
    const kittens = view.scene.getObjectByName('kittens') as unknown as { count: number };
    expect(kittens.count).toBe(2);
  });

  it('idle: only short 10 fps ambient bouts, nothing in between', () => {
    const { clock, view } = setup();
    view.setModel(model(world([sess('a'), sess('b', { state: 'busy' })])));
    clock.advance(BOUT_EVERY_MS - 100);
    expect(view.frames).toBe(1);
    clock.advance(100 + BOUT_MS + 200);
    const bout = view.frames - 1;
    expect(bout).toBeGreaterThanOrEqual(BOUT_MS / 100 - 2);
    expect(bout).toBeLessThanOrEqual(BOUT_MS / 100 + 2);
    const settled = view.frames;
    clock.advance(BOUT_EVERY_MS - BOUT_MS - 400);
    expect(view.frames).toBe(settled);
  });

  it('reduced motion: static poses, no ambient bouts, no tweens', () => {
    const { clock, view } = setup({ reduced: true });
    const s = world([sess('a', { state: 'busy' })]);
    view.setModel(model(s));
    clock.advance(BOUT_EVERY_MS * 3);
    expect(view.frames).toBe(1);
    s.sessions[0]!.state = 'blocked';
    view.setModel(model(s));
    clock.advance(1000);
    expect(view.frames).toBe(2);
    view.wiggle();
    clock.advance(1000);
    expect(view.frames).toBe(2);
  });

  it('a pose change tweens at 60 fps, then stops', () => {
    const { clock, view } = setup();
    const s = world([sess('a', { state: 'busy' })]);
    view.setModel(model(s));
    clock.advance(50);
    s.sessions[0]!.state = 'done';
    view.setModel(model(s));
    clock.advance(TWEEN_MS + 100);
    const tween = view.frames - 1;
    expect(tween).toBeGreaterThanOrEqual(TWEEN_MS / 17 - 2);
    expect(view.poses().a).toBe('sleeping');
    const after = view.frames;
    clock.advance(2000);
    expect(view.frames).toBe(after);
  });

  it('ended plays the puff once', () => {
    const { clock, view } = setup();
    const s = world([sess('a', { state: 'busy' })]);
    view.setModel(model(s));
    s.sessions[0]!.ended = true;
    view.setModel(model(s));
    clock.advance(1500);
    expect(view.poses().a).toBe('puff');
    const after = view.frames;
    clock.advance(1500);
    expect(view.frames).toBe(after);
  });

  it('hidden: no frames, no bouts; visible: redraw', () => {
    const { clock, view } = setup();
    view.setModel(model(world([sess('a')])));
    clock.advance(50);
    view.setVisible(false);
    view.setModel(model(world([sess('a'), sess('b')])));
    clock.advance(BOUT_EVERY_MS * 2);
    expect(view.frames).toBe(1);
    view.setVisible(true);
    clock.advance(50);
    expect(view.frames).toBe(2);
    expect(view.catCount).toBe(2);
  });

  it('removes cats whose session went away', () => {
    const { view } = setup();
    view.setModel(model(world([sess('a'), sess('b')])));
    view.setModel(model(world([sess('b')])));
    expect(view.catCount).toBe(1);
    expect(view.scene.getObjectByName('cat:a')).toBeUndefined();
  });

  it('picks the cat under a screen point and nothing elsewhere', () => {
    const { view } = setup();
    view.setModel(model(world([sess('a'), sess('b'), sess('c', { context: 'z' })])));
    for (const id of ['a', 'b', 'c']) {
      const p = view.catScreen(id)!;
      const ndc = [(p.x / 800) * 2 - 1, -(p.y / 500) * 2 + 1] as const;
      expect(view.pick(...ndc)).toBe(id);
    }
    expect(view.pick(-0.99, 0.99)).toBeNull();
    expect(view.catScreen('nope')).toBeNull();
  });

  it('overlays: station labels, ! and zZ bubbles, ✉ cues', () => {
    const { view } = setup();
    const at = new Date(NOW - 1000).toISOString();
    const s = world(
      [sess('a', { state: 'blocked', attention: 1 }), sess('b', { state: 'done', context: 'd' }), sess('c', { context: 'd' })],
      {
        groups: { t: [{ session_id: 'a', name: 'a' }, { session_id: 'c', name: 'c' }] },
        mail: { a: { at, sent: true }, c: { at, sent: false } },
      },
    );
    view.setModel(model(s));
    const ov = view.overlay();
    const by = (k: string) => ov.find((o) => o.key === k);
    expect(by('ctx:c')?.text).toBe('c');
    expect(by('ctx:d')?.text).toBe('d');
    expect(by('b:a')).toMatchObject({ text: '!', token: 'blocked' });
    expect(by('b:b')).toMatchObject({ text: 'zZ' });
    expect(by('m:a')).toMatchObject({ text: '✉', blink: true });
    expect(by('m:c')).toMatchObject({ text: '✉', blink: true });
    for (const o of ov) {
      expect(o.x).toBeGreaterThan(0);
      expect(o.x).toBeLessThan(800);
      expect(o.y).toBeGreaterThan(0);
      expect(o.y).toBeLessThan(500);
    }
  });

  it('a new ✉ flies once from sender to receiver (not under reduced motion)', () => {
    const at = new Date(NOW - 1000).toISOString();
    const s = world([sess('a'), sess('b')], {
      groups: { t: [{ session_id: 'a', name: 'a' }, { session_id: 'b', name: 'b' }] },
      mail: { a: { at, sent: true }, b: { at, sent: false } },
    });
    const { clock, view } = setup();
    view.setModel(model(s));
    clock.advance(FLIGHT_MS + 100);
    expect(view.frames).toBeGreaterThan(FLIGHT_MS / 17 - 5);
    const after = view.frames;
    view.setModel(model(s, NOW + 10)); // same message: no second flight
    clock.advance(FLIGHT_MS);
    expect(view.frames).toBe(after + 1);

    const r = setup({ reduced: true });
    r.view.setModel(model(s));
    r.clock.advance(FLIGHT_MS);
    expect(r.view.frames).toBe(1);
  });

  it('reduced motion can be switched both ways', () => {
    const { clock, view } = setup();
    view.setModel(model(world([sess('a')])));
    view.setReducedMotion(true);
    view.setReducedMotion(true);
    clock.advance(BOUT_EVERY_MS * 2);
    expect(view.frames).toBe(1);
    view.setReducedMotion(false);
    clock.advance(BOUT_EVERY_MS + BOUT_MS);
    expect(view.frames).toBeGreaterThan(5);
  });

  it('lit collars, focus ring and theme changes re-skin without new cats', () => {
    const { view } = setup();
    const s = world([sess('a'), sess('b')], { groups: { t: [{ session_id: 'a', name: 'a' }, { session_id: 'b', name: 'b' }] } });
    view.setModel(model(s));
    view.setModel(model(s, NOW, { hovered: 'a' }));
    view.setModel(buildScene(s, [], { ...FALLBACK_THEME, tokens: { ...FALLBACK_THEME.tokens, bg: '#000000' } }, NOW));
    expect(view.catCount).toBe(2);
  });

  it('a wide world wraps and still frames every cat', () => {
    const { view } = setup();
    const many = Array.from({ length: 20 }, (_, i) => sess(`s${String(i).padStart(2, '0')}`, { context: `c${i % 6}` }));
    view.setModel(model(world(many)));
    for (const c of many) {
      const p = view.catScreen(c.session_id)!;
      expect(p.x).toBeGreaterThan(0);
      expect(p.x).toBeLessThan(800);
      expect(p.y).toBeGreaterThan(0);
      expect(p.y).toBeLessThan(500);
    }
  });

  it('dispose releases the renderer and stops everything', () => {
    const { clock, renderer, view } = setup();
    view.setModel(model(world([sess('a')])));
    view.dispose();
    clock.advance(BOUT_EVERY_MS * 2);
    expect(renderer.disposed).toBe(true);
    expect(view.frames).toBe(0);
  });
});
