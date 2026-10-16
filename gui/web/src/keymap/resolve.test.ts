import { describe, expect, it } from 'vitest';
import type { Binding } from '../client/types.ts';
import { DEMO_KEYMAP } from './demo-fixture.ts';
import {
  IDLE,
  feedKey,
  helpKey,
  isSequence,
  keysOf,
  normalMode,
  parentMode,
  resolve,
  startsSequence,
} from './resolve.ts';

describe('parentMode', () => {
  it.each([
    ['normal.todos', 'normal'],
    ['a.b.c', 'a.b'],
    ['normal', '*'],
    ['insert', '*'],
    ['*', ''],
    ['', ''],
  ])('%s → %s', (m, want) => expect(parentMode(m)).toBe(want));
});

describe('resolve over the demo keymap', () => {
  const cases: Array<[string, string, string | undefined]> = [
    ['normal', 'j', 'nav.down'],
    ['normal', 'down', 'nav.down'],
    ['normal', 'k', 'nav.up'],
    ['normal', 'G', 'nav.bottom'],
    ['normal', 'gg', 'nav.top'],
    ['normal', 'dd', 'session.remove'],
    ['normal', 'enter', 'session.open'],
    ['normal', 'o', 'session.preview'],
    ['normal', 'x', 'session.stop'],
    ['normal', 'w', 'pane.close'],
    ['normal', 'z', 'pane.zoom'],
    ['normal', ':', 'palette.open'],
    ['normal', 'ctrl+p', 'palette.open'],
    ['normal', '?', 'help.open'],
    ['normal', 'shift+space', 'inbox.summon'],
    ['normal', 'shift+tab', 'tab.prev'],
    ['normal', 'ctrl+k', 'switcher.open'], // falls back to "*"
    ['insert', 'ctrl+k', 'switcher.open'],
    ['insert', 'j', undefined],
    // the todos sub-mode shadows its parent and falls back to it
    ['normal.todos', 'enter', 'todo.dispatch'],
    ['normal.todos', 'o', 'jira.open_url'],
    ['normal.todos', 's', 'todo.start'],
    ['normal.todos', 'j', 'nav.down'],
    ['normal.todos', 'ctrl+k', 'switcher.open'],
    ['normal', 's', undefined],
    ['normal', 'g', undefined],
    ['normal', 'ç', undefined],
  ];
  it.each(cases)('%s %s → %s', (mode, key, want) => {
    expect(resolve(DEMO_KEYMAP, mode, key)).toBe(want);
  });

  it('first binding in order wins within a mode', () => {
    const km: Binding[] = [
      { command: 'a', keys: ['x'], mode: 'normal', help: { key: 'x', desc: '' } },
      { command: 'b', keys: ['x'], mode: 'normal', help: { key: 'x', desc: '' } },
      { command: 'c', keys: null, mode: 'normal', help: { key: '', desc: '' } },
    ];
    expect(resolve(km, 'normal', 'x')).toBe('a');
  });
});

describe('sequences', () => {
  it.each([
    ['gg', true],
    ['dd', true],
    ['g', false],
    ['gd', false],
    ['ggg', false],
    ['çç', true],
  ])('isSequence(%s) = %s', (k, want) => expect(isSequence(k)).toBe(want));

  it.each([
    ['normal', 'g', true],
    ['normal', 'd', true],
    ['normal', 'j', false],
    ['normal.todos', 'g', true],
    ['insert', 'g', false],
  ])('startsSequence(%s, %s) = %s', (mode, k, want) => expect(startsSequence(DEMO_KEYMAP, mode, k)).toBe(want));

  it('g then g within the timeout is nav.top', () => {
    const a = feedKey(DEMO_KEYMAP, 'normal', IDLE, 'g', 1000);
    expect(a).toEqual({ waiting: true, key: 'g', state: { pending: 'g', at: 1000 } });
    const b = feedKey(DEMO_KEYMAP, 'normal', a.state, 'g', 1500);
    expect(b).toEqual({ state: IDLE, key: 'gg', command: 'nav.top', waiting: false });
  });

  it('a late second press starts over', () => {
    const a = feedKey(DEMO_KEYMAP, 'normal', IDLE, 'd', 0);
    const b = feedKey(DEMO_KEYMAP, 'normal', a.state, 'd', 5000);
    expect(b.waiting).toBe(true);
    expect(b.command).toBeUndefined();
  });

  it('another key drops the wait and resolves alone', () => {
    const a = feedKey(DEMO_KEYMAP, 'normal', IDLE, 'g', 0);
    const b = feedKey(DEMO_KEYMAP, 'normal', a.state, 'j', 10);
    expect(b).toEqual({ state: IDLE, key: 'j', command: 'nav.down', waiting: false });
  });

  it('unbound key resolves to nothing', () => {
    expect(feedKey(DEMO_KEYMAP, 'normal', IDLE, 'y', 0)).toEqual({ state: IDLE, key: 'y', waiting: false });
  });

  it('a completed but unbound sequence resolves to nothing', () => {
    const km: Binding[] = [{ command: 'x', keys: ['qq'], mode: 'normal.todos', help: { key: 'qq', desc: '' } }];
    const a = feedKey(km, 'normal.todos', IDLE, 'q', 0);
    expect(a.waiting).toBe(true);
    expect(feedKey(km, 'normal', a.state, 'q', 1)).toEqual({ state: IDLE, key: 'qq', waiting: false });
  });
});

describe('helpers', () => {
  it('normalMode follows the tab', () => {
    expect(normalMode('todos')).toBe('normal.todos');
    expect(normalMode('sessions')).toBe('normal');
    expect(normalMode(undefined)).toBe('normal');
  });
  it('keysOf and helpKey', () => {
    expect(keysOf(DEMO_KEYMAP, 'nav.down')).toEqual(['j', 'down']);
    expect(keysOf(DEMO_KEYMAP, 'nope')).toEqual([]);
    expect(helpKey(DEMO_KEYMAP, 'nav.down')).toBe('j/down');
    expect(helpKey(DEMO_KEYMAP, 'nope')).toBe('');
    expect(helpKey([{ command: 'a', keys: ['x', 'y'], mode: 'normal', help: { key: '', desc: '' } }], 'a')).toBe('x/y');
  });
});
