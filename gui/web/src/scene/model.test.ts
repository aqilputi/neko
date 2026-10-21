import { describe, expect, it } from 'vitest';
import type { Session, Snapshot, TreeRow } from '../client/types.ts';
import { FALLBACK_THEME, contextColor } from '../state/theme.ts';
import { GW, IDX, MIG, snapshot, tree } from '../test/fixtures.ts';
import {
  CAT_SLOT,
  KITTEN_SLOT,
  MAIL_BLINK_MS,
  MAIL_DIM_MS,
  MAX_ROW_WIDTH,
  ROW_DEPTH,
  buildScene,
  catSessions,
  hashColor,
  mailArcs,
  mailState,
  modelKey,
  poseFor,
  posesById,
  slotWidth,
  type PoseInput,
} from './model.ts';

const theme = FALLBACK_THEME;
const NOW = Date.parse('2026-10-16T19:00:00Z');
const iso = (ms: number) => new Date(ms).toISOString();

function sess(id: string, over: Partial<Session> = {}): Session {
  return { session_id: id, label: id, cwd: '/x', kind: 'background', state: 'idle', attention: 0, cost: 0, context: 'c', ...over };
}

function snap(sessions: Session[], over: Partial<Snapshot> = {}): Snapshot {
  return { ...snapshot(), sessions, groups: {}, mail: {}, ...over };
}

function row(id: string, over: Partial<TreeRow> = {}): TreeRow {
  return { key: `sess:${id}`, kind: 'sess', ctx: 'c', depth: 1, color: 0, session_id: id, attention: 0, ...over };
}

describe('poseFor', () => {
  const cases: [string, PoseInput, string][] = [
    ['busy types', { state: 'busy', attention: 0 }, 'typing'],
    ['blocked is alert', { state: 'blocked', attention: 1 }, 'alert'],
    ['attention blocked wins over busy', { state: 'busy', attention: 1 }, 'alert'],
    ['done sleeps', { state: 'done', attention: 0 }, 'sleeping'],
    ['returned sleeps', { state: 'idle', attention: 2 }, 'sleeping'],
    ['finished sleeps', { state: 'stopped', attention: 0, finished: true }, 'sleeping'],
    ['idle sits', { state: 'idle', attention: 0 }, 'sit'],
    ['interactive idle (attention 3) sits', { state: 'idle', attention: 3 }, 'sit'],
    ['stopped sits', { state: 'stopped', attention: 0 }, 'sit'],
    ['history sits', { state: 'history', attention: 0 }, 'sit'],
    ['unknown state sits', {}, 'sit'],
    ['ended puffs', { state: 'done', attention: 2, ended: true }, 'puff'],
    ['error puffs', { state: 'error' }, 'puff'],
    ['failed puffs', { state: 'failed', attention: 1 }, 'puff'],
  ];
  it.each(cases)('%s', (_n, input, want) => {
    expect(poseFor(input)).toBe(want);
  });
});

describe('mailState', () => {
  const cases: [string, number | null, string | undefined, string | null][] = [
    ['no cue', null, undefined, null],
    ['fresh blinks', 1000, undefined, 'blink'],
    ['just before 90 s blinks', MAIL_BLINK_MS - 1, undefined, 'blink'],
    ['at 90 s dims', MAIL_BLINK_MS, undefined, 'dim'],
    ['after 15 min is gone', MAIL_DIM_MS, undefined, null],
    ['the row wins (blink)', MAIL_DIM_MS * 2, 'blink', 'blink'],
    ['the row wins (dim)', 0, 'dim', 'dim'],
  ];
  it.each(cases)('%s', (_n, age, fromRow, want) => {
    const cue = age === null ? undefined : { at: iso(NOW - age), sent: false };
    expect(mailState(cue, NOW, fromRow)).toBe(want);
  });
});

describe('mailArcs', () => {
  const g = new Map([
    ['a', [{ tag: 't' }]],
    ['b', [{ tag: 't' }]],
    ['c', [{ tag: 'other' }]],
  ]);
  it('pairs a sender and a receiver of the same group', () => {
    const cues = new Map([
      ['a', { cue: { at: iso(NOW), sent: true }, state: 'blink' as const }],
      ['b', { cue: { at: iso(NOW + 10), sent: false }, state: 'blink' as const }],
    ]);
    expect(mailArcs(cues, g)).toEqual([{ from: 'a', to: 'b', at: iso(NOW + 10), blink: true }]);
  });
  it('ignores senders of another group and far-apart cues', () => {
    const cues = new Map([
      ['c', { cue: { at: iso(NOW), sent: true }, state: 'blink' as const }],
      ['a', { cue: { at: iso(NOW - 60_000), sent: true }, state: 'dim' as const }],
      ['b', { cue: { at: iso(NOW), sent: false }, state: 'blink' as const }],
    ]);
    expect(mailArcs(cues, g)).toEqual([]);
  });
  it('picks the closest sender in time; dim arcs do not blink', () => {
    const g2 = new Map([...g, ['d', [{ tag: 't' }]]]);
    const cues = new Map([
      ['a', { cue: { at: iso(NOW - 4000), sent: true }, state: 'dim' as const }],
      ['d', { cue: { at: iso(NOW - 100), sent: true }, state: 'dim' as const }],
      ['b', { cue: { at: iso(NOW), sent: false }, state: 'dim' as const }],
    ]);
    expect(mailArcs(cues, g2)).toEqual([{ from: 'd', to: 'b', at: iso(NOW), blink: false }]);
  });
  it('a receiver without groups pairs with any sender', () => {
    const cues = new Map([
      ['x', { cue: { at: iso(NOW), sent: true }, state: 'blink' as const }],
      ['y', { cue: { at: iso(NOW), sent: false }, state: 'blink' as const }],
    ]);
    expect(mailArcs(cues, new Map())).toEqual([{ from: 'x', to: 'y', at: iso(NOW), blink: true }]);
  });
});

describe('catSessions', () => {
  it('keeps live sessions and the history the tree shows, drops archived; sorted by context then start', () => {
    const s = snap([
      sess('h1', { kind: 'history', state: 'history', context: 'a' }),
      sess('h2', { kind: 'history', state: 'history', context: 'a' }),
      sess('z', { context: 'b', started: '2026-10-16T10:00:00Z' }),
      sess('y', { context: 'b', started: '2026-10-16T09:00:00Z' }),
      sess('arch', { archived: true }),
    ]);
    const ids = catSessions(s, [row('h2')]).map((x) => x.session_id);
    expect(ids).toEqual(['h2', 'y', 'z']);
  });
  it('no snapshot, no cats', () => {
    expect(catSessions(null, [])).toEqual([]);
  });
});

describe('buildScene', () => {
  it('turns the demo fixture into stations, cats and kittens', () => {
    const s = snapshot();
    s.sessions.push(
      sess(IDX, {
        label: 'indexador',
        context: 'oss',
        state: 'busy',
        subs: [
          { id: 'k1', description: 'mapear' },
          { id: 'k2', type: 'Explore' },
          { id: 'k3', done: true },
        ],
      }),
    );
    const m = buildScene(s, tree().rows ?? [], theme, NOW);
    expect(m.stations.map((x) => x.ctx)).toEqual(['acme', 'oss']);
    expect(posesById(m)).toEqual({ [IDX]: 'typing', [GW]: 'typing', [MIG]: 'alert' });
    const idx = m.cats.find((c) => c.id === IDX)!;
    expect(idx.kittens.map((k) => k.label)).toEqual(['mapear', 'Explore']);
    expect(idx.kittens.every((k) => k.z > idx.z)).toBe(true);
    // kittens sit on both sides of the parent
    expect(idx.kittens[0]!.x).toBeCloseTo(idx.x - KITTEN_SLOT / 2);
    expect(idx.kittens[1]!.x).toBeCloseTo(idx.x + KITTEN_SLOT / 2);
    const gw = m.cats.find((c) => c.id === GW)!;
    expect(gw.collar).toBe(contextColor(theme, 18));
    expect(gw.glow).toBe(theme.tokens.busy);
    expect(gw.bubble).toBeNull();
    const mig = m.cats.find((c) => c.id === MIG)!;
    expect(mig.bubble).toBe('!');
    expect(mig.collar).toBeNull();
    // station colour: the tree's ctx colour (acme = 0)
    expect(m.stations.find((x) => x.ctx === 'acme')!.color).toBe(contextColor(theme, 0));
    expect(m.colors.body).toBe(theme.tokens.cat_body);
    expect(m.colors.eyes).toBe(theme.tokens.cat_eyes);
    expect(m.nextChangeAt).toBeNull();
  });

  it('lays cats left to right, centred, inside their station', () => {
    const s = snap([sess('a', { context: 'x' }), sess('b', { context: 'x' }), sess('c', { context: 'y' })]);
    const m = buildScene(s, [], theme, NOW);
    const [a, b, c] = m.cats;
    expect(b!.x - a!.x).toBeCloseTo(CAT_SLOT);
    expect(c!.x).toBeGreaterThan(b!.x);
    const xs = m.stations.map((st) => [st.x - st.width / 2, st.x + st.width / 2]);
    expect(xs[0]![0]).toBeCloseTo(-m.width / 2);
    expect(xs[1]![1]).toBeCloseTo(m.width / 2);
    for (const cat of m.cats) {
      const st = m.stations.find((x) => x.ctx === cat.ctx)!;
      expect(Math.abs(cat.x - st.x)).toBeLessThan(st.width / 2);
    }
    // contexts the tree doesn't colour get their order as index
    expect(m.stations.map((x) => x.color)).toEqual([contextColor(theme, 0), contextColor(theme, 1)]);
  });

  it('wraps stations into rows deeper on the desk when too wide', () => {
    const many = Array.from({ length: 14 }, (_, i) => sess(`s${String(i).padStart(2, '0')}`, { context: `ctx${i % 7}` }));
    const m = buildScene(snap(many), [], theme, NOW);
    const zs = [...new Set(m.stations.map((s) => s.z))];
    expect(zs.length).toBeGreaterThan(1);
    expect(zs).toContain(-ROW_DEPTH);
    expect(m.width).toBeLessThanOrEqual(MAX_ROW_WIDTH);
    expect(m.depth).toBe(zs.length * ROW_DEPTH);
  });

  it('dims stale, history and ended cats', () => {
    const s = snap([
      sess('st', { stale: true }),
      sess('hi', { kind: 'history', state: 'history' }),
      sess('en', { ended: true }),
      sess('ok'),
    ]);
    const m = buildScene(s, [row('hi')], theme, NOW);
    expect(Object.fromEntries(m.cats.map((c) => [c.id, c.dim]))).toEqual({ st: true, hi: true, en: true, ok: false });
    expect(m.cats.find((c) => c.id === 'en')!.pose).toBe('puff');
  });

  it('lights the collars of the hovered (else the selected) session peers', () => {
    const s = snap([sess('a'), sess('b'), sess('c'), sess('d')], {
      groups: {
        t: [
          { session_id: 'a', name: 'a' },
          { session_id: 'b', name: 'b' },
        ],
        u: [{ session_id: 'c', name: 'c' }],
      },
    });
    const rows = [row('a', { groups: [{ tag: 't', color: 3 }] })];
    const lit = (f: { hovered?: string | null; selected?: string | null }) =>
      buildScene(s, rows, theme, NOW, f)
        .cats.filter((c) => c.collarLit)
        .map((c) => c.id);
    expect(lit({})).toEqual([]);
    expect(lit({ selected: 'b' })).toEqual(['a', 'b']);
    expect(lit({ selected: 'b', hovered: 'c' })).toEqual(['c']);
    expect(lit({ hovered: 'd' })).toEqual([]);
    const m = buildScene(s, rows, theme, NOW, { hovered: 'a' });
    // b's tag colour comes from the row that carries the same tag
    expect(m.cats.find((c) => c.id === 'b')!.collar).toBe(contextColor(theme, 3));
    // c's tag has no row: hashed like the core does
    expect(m.cats.find((c) => c.id === 'c')!.collar).toBe(contextColor(theme, hashColor('u')));
    expect(m.cats.find((c) => c.id === 'a')!.focused).toBe(true);
    expect(m.cats.find((c) => c.id === 'd')!.collar).toBeNull();
  });

  it('turns mail cues into a sender→receiver arc that blinks for 90 s', () => {
    const at = iso(NOW - 10_000);
    const s = snap([sess('a'), sess('b')], {
      groups: {
        t: [
          { session_id: 'a', name: 'a' },
          { session_id: 'b', name: 'b' },
        ],
      },
      mail: { a: { at, sent: true }, b: { at, sent: false } },
    });
    const m = buildScene(s, [], theme, NOW);
    expect(m.arcs).toEqual([{ from: 'a', to: 'b', at, blink: true }]);
    expect(m.cats.map((c) => c.mail)).toEqual([
      { sent: true, blink: true },
      { sent: false, blink: true },
    ]);
    expect(m.nextChangeAt).toBe(NOW - 10_000 + MAIL_BLINK_MS);
    const later = buildScene(s, [], theme, NOW + MAIL_BLINK_MS);
    expect(later.arcs[0]!.blink).toBe(false);
    expect(later.nextChangeAt).toBe(NOW - 10_000 + MAIL_DIM_MS);
    expect(buildScene(s, [], theme, NOW + MAIL_DIM_MS).arcs).toEqual([]);
  });

  it('uses the row mail_state when the tree has it', () => {
    const at = iso(NOW - 10 * 60_000);
    const s = snap([sess('a'), sess('b')], { mail: { a: { at, sent: true }, b: { at, sent: false } } });
    const rows = [row('a', { mail: { at, sent: true }, mail_state: 'blink' }), row('b', { mail: { at, sent: false }, mail_state: 'blink' })];
    expect(buildScene(s, rows, theme, NOW).arcs[0]!.blink).toBe(true);
  });

  it('labels fall back to name, short id, id prefix', () => {
    const s = snap([
      sess('aaaaaaaaaaaa', { label: '', name: 'nome' }),
      sess('bbbbbbbbbbbb', { label: '', short_id: 'curto' }),
      sess('cccccccccccc', { label: '' }),
    ]);
    expect(buildScene(s, [], theme, NOW).cats.map((c) => c.label)).toEqual(['nome', 'curto', 'cccccccc']);
  });

  it('an empty world is an empty desk', () => {
    const m = buildScene(null, [], theme, NOW);
    expect(m.cats).toEqual([]);
    expect(m.stations).toEqual([]);
    expect(m.depth).toBe(ROW_DEPTH);
  });

  it('theme without tokens falls back to catppuccin', () => {
    const m = buildScene(null, [], { ...theme, tokens: {}, contexts: [] }, NOW);
    expect(m.colors.body).toBe('#11111b');
    expect(m.colors.eyes).toBe('#f9e2af');
  });

  it('modelKey changes only with what is drawn', () => {
    const s = snapshot();
    const a = buildScene(s, tree().rows ?? [], theme, NOW);
    const b = buildScene(snapshot(), tree().rows ?? [], theme, NOW + 5000);
    expect(modelKey(a)).toBe(modelKey(b));
    s.sessions[0]!.state = 'blocked';
    expect(modelKey(buildScene(s, tree().rows ?? [], theme, NOW))).not.toBe(modelKey(a));
  });

  it('slotWidth grows only past two kittens', () => {
    expect(slotWidth(0)).toBe(CAT_SLOT);
    expect(slotWidth(2)).toBe(CAT_SLOT);
    expect(slotWidth(4)).toBe(CAT_SLOT + 2 * KITTEN_SLOT);
  });
});
