import { describe, expect, it } from 'vitest';
import type { CommandInfo, Session } from '../client/types.ts';
import { DEMO_COMMANDS } from '../keymap/demo-fixture.ts';
import { snapshot } from '../test/fixtures.ts';
import {
  argCommand,
  argsUsage,
  cleanTag,
  filterPalette,
  fold,
  fuzzyFilter,
  fuzzyScore,
  listedCommand,
  paletteItems,
  stateGlyph,
  stateLabel,
  switcherItems,
  typedItem,
  type PaletteInput,
} from './palette.ts';

const cmd = (name: string) => DEMO_COMMANDS.find((c) => c.name === name)!;

describe('fuzzy', () => {
  it('folds accents and case', () => expect(fold('Migração DO Banco')).toBe('migracao do banco'));
  it.each([
    ['', 'anything', 0],
    ['xyz', 'abc', null],
    ['mig', 'migração do banco', 'number'],
    ['mgb', 'migração do banco', 'number'],
  ] as const)('%s in %s', (q, t, want) => {
    const s = fuzzyScore(q, t);
    if (want === 'number') expect(typeof s).toBe('number');
    else expect(s).toBe(want);
  });
  it('prefers word starts and runs', () => {
    expect(fuzzyScore('par', 'parar a sessão')!).toBeGreaterThan(fuzzyScore('par', 'comparar')!);
    expect(fuzzyScore('mig', 'migração')!).toBeGreaterThan(fuzzyScore('mig', 'xmxixg')!);
  });
  it('filters and sorts, stable on ties', () => {
    const items = [
      { title: 'comparar', hint: '' },
      { title: 'parar a sessão', hint: 'x' },
      { title: 'nada', hint: '' },
    ];
    expect(fuzzyFilter(items, 'par').map((i) => i.title)).toEqual(['parar a sessão', 'comparar']);
    expect(fuzzyFilter(items, '  ')).toHaveLength(3);
  });
});

describe('paletteItems', () => {
  const sessions = snapshot().sessions;
  const base: PaletteInput = {
    commands: DEMO_COMMANDS.map((c) => ({ ...c, available: true })),
    sessions: [...sessions, { ...sessions[0]!, session_id: 'old', label: 'velha', stale: true }],
    showAll: false,
    themes: ['catppuccin-mocha', 'dracula'],
    activeTheme: 'dracula',
    pickedN: 0,
    groups: { 'deploy-rc': 4 },
  };
  const items = paletteItems(base);
  const byId = (id: string) => items.find((i) => i.id === id);

  it('lists commands with their first key', () => {
    expect(byId('cmd:session.stop')).toEqual({
      id: 'cmd:session.stop',
      title: 'parar a sessão em background',
      hint: 'x',
      action: { kind: 'run', name: 'session.stop', key: 'x' },
    });
    expect(byId('cmd:lua.reload')?.action).toEqual({ kind: 'run', name: 'lua.reload' });
  });
  it('leaves out itself, groups and sequences', () => {
    expect(byId('cmd:palette.open')).toBeUndefined();
    expect(byId('cmd:group.add')).toBeUndefined();
    expect(byId('cmd:session.remove')).toBeUndefined();
    expect(byId('cmd:nav.top')).toBeUndefined();
  });
  it('commands with required args open typed', () => {
    expect(byId('cmd:session.new')?.action).toEqual({ kind: 'prefill', text: 'session.new ' });
  });
  it('typed group items, dissolve per group, sessions, themes', () => {
    expect(byId('typed:group')?.title).toContain('a sessão do cursor');
    expect(paletteItems({ ...base, pickedN: 2 }).find((i) => i.id === 'typed:group')?.title).toContain('as 2 selecionadas');
    expect(byId('dissolve:deploy-rc')?.action).toEqual({ kind: 'typed', name: 'group.dissolve', args: { tag: 'deploy-rc' } });
    expect(byId(`goto:${sessions[0]!.session_id}`)?.title).toBe('ir: api-gateway');
    expect(byId('goto:old')).toBeUndefined();
    expect(paletteItems({ ...base, showAll: true }).some((i) => i.id === 'goto:old')).toBe(true);
    expect(byId('theme:dracula')).toMatchObject({ hint: 'atual', action: { kind: 'theme', name: 'dracula' } });
    expect(byId('theme:catppuccin-mocha')?.hint).toBe('');
  });
  it('unavailable commands are left out', () => {
    expect(listedCommand({ ...cmd('session.stop'), available: false })).toBe(false);
  });
  it('a command without title shows its name', () => {
    const c: CommandInfo = { ...cmd('lua.reload'), title: '', name: 'x.y', hint: 'h' };
    expect(paletteItems({ ...base, commands: [c] })[0]).toMatchObject({ title: 'x.y', hint: 'h' });
  });
});

describe('typed commands', () => {
  const inp = { commands: DEMO_COMMANDS, pickedN: 0, cursorTags: ['deploy-rc'], lua: true };
  it.each([
    ['', null],
    ['group', { title: 'group <tag>', action: { kind: 'none' } }],
    ['group Deploy RC', { title: '▶ group #deploy-rc', action: { kind: 'typed', name: 'group.add', args: { tag: 'deploy-rc' } } }],
    ['g x', { action: { kind: 'typed', name: 'group.add', args: { tag: 'x' } } }],
    ['ungroup', { title: '▶ ungroup a sessão do cursor', action: { kind: 'typed', name: 'group.remove', args: {} } }],
    ['ug #x', { title: '▶ ungroup #x (a sessão do cursor)', action: { kind: 'typed', name: 'group.remove', args: { tag: 'x' } } }],
    ['notify #ops oi gente', { action: { kind: 'typed', name: 'group.notify', args: { tag: 'ops', text: 'oi gente' } } }],
    ['notify bom dia', { action: { kind: 'typed', name: 'group.notify', args: { tag: 'deploy-rc', text: 'bom dia' } } }],
    ['notify #ops', { title: 'notify #tag <mensagem>', action: { kind: 'none' } }],
    ['lua', { action: { kind: 'none' } }],
    ['lua return 1', { action: { kind: 'lua', code: 'return 1' } }],
    ['session.rename novo nome', { title: '▶ session.rename novo nome', action: { kind: 'typed', name: 'session.rename', args: { name: 'novo nome' } } }],
    ['session.new', { title: 'session.new <dir> <prompt> [perm] [name]', action: { kind: 'none' } }],
    [
      'session.new ~/x faça tudo',
      { action: { kind: 'typed', name: 'session.new', args: { dir: '~/x', prompt: 'faça', perm: 'tudo' } } },
    ],
    ['session.reply', { title: 'session.reply <text>' }],
    ['parar', null],
    ['nav.down x', null], // client command
    ['sessions.refresh x', null], // no args
  ])('%s', (q, want) => {
    const got = typedItem(q, inp);
    if (want === null) expect(got).toBeNull();
    else expect(got).toMatchObject(want);
  });

  it('notify with no group to default to waits', () => {
    expect(typedItem('notify oi', { ...inp, cursorTags: [] })).toMatchObject({ hint: 'sem grupo', action: { kind: 'none' } });
  });
  it('lua is off without the capability', () => expect(typedItem('lua x', { ...inp, lua: false })).toBeNull());
  it('picked count in the hint', () => {
    expect(typedItem('group x', { ...inp, pickedN: 3 })?.hint).toBe('3 sessões');
  });
  it('argCommand skips an empty optional in the middle', () => {
    const c: CommandInfo = {
      ...cmd('session.new'),
      name: 't.x',
      args: [
        { name: 'a', type: 'string', prompt: '', required: false },
        { name: 'b', type: 'string', prompt: '', required: false },
      ],
    };
    expect(argCommand('t.x', [c])).toMatchObject({ title: '▶ t.x', action: { kind: 'typed', args: {} } });
  });
  it('helpers', () => {
    expect(cleanTag(' ##Deploy  RC ')).toBe('deploy-rc');
    expect(argsUsage({ args: null })).toBe('');
  });
  it('filterPalette puts the typed item on top', () => {
    const items = [{ id: 'a', title: 'group <tag>', hint: '', action: { kind: 'none' } as const }];
    const typed = typedItem('group x', inp);
    expect(filterPalette(items, 'group x', typed)[0]).toBe(typed);
    expect(filterPalette(items, 'grp', null)).toHaveLength(1);
  });
});

describe('switcher', () => {
  const s = (id: string, extra: Partial<Session>): Session => ({
    session_id: id,
    label: id,
    cwd: '/',
    kind: 'background',
    state: 'idle',
    attention: 0,
    cost: 0,
    ...extra,
  });
  it('waiting first, then by activity; dead and archived left out', () => {
    const items = switcherItems([
      s('busy-old', { state: 'busy', last_at: '2026-10-16T10:00:00Z' }),
      s('busy-new', { state: 'busy', last_at: '2026-10-16T12:00:00Z', context: 'visio' }),
      s('waiting', { state: 'blocked', attention: 1 }),
      s('idle', {}),
      s('ended', { state: 'busy', ended: true }),
      s('archived', { attention: 2, archived: true }),
      s('returned', { state: 'done', attention: 2 }),
    ]);
    expect(items.map((i) => i.id)).toEqual(['sw:waiting', 'sw:returned', 'sw:busy-new', 'sw:busy-old']);
    expect(items[0]).toMatchObject({ title: '◆ waiting', hint: 'te esperando', action: { kind: 'goto', sessionId: 'waiting', open: true } });
    expect(items[2]!.hint).toBe('rodando · visio');
  });
  it.each([
    ['busy', 0, '●', 'rodando'],
    ['blocked', 0, '◆', 'te esperando'],
    ['done', 0, '✓', 'pronta'],
    ['done', 2, '✓', 'voltou'],
    ['idle', 0, '○', 'ociosa'],
    ['idle', 3, '○', 'ociosa'],
    ['history', 0, '·', 'history'],
    [undefined, 0, '·', ''],
  ] as const)('%s/%d → %s %s', (st, att, g, l) => {
    expect(stateGlyph(st, att)).toBe(g);
    expect(stateLabel(st, att)).toBe(l);
  });
});
