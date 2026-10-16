import { describe, expect, it } from 'vitest';
import type { Result } from '../client/types.ts';
import { createInboxHub } from './hub.ts';
import { installInboxKeys, isSummonKey, isTyping, type KeyLike, type KeyTarget } from './keys.ts';

const el = (matches: boolean, editable = false) => ({ closest: () => (matches ? {} : null), isContentEditable: editable });
const key = (k: Partial<KeyLike>): KeyLike => ({
  key: ' ',
  code: 'Space',
  shiftKey: true,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
  target: el(false),
  ...k,
});

describe('isSummonKey', () => {
  it.each([
    ['shift+space', key({}), true],
    ['by code only', key({ key: 'x', code: 'Space' }), true],
    ['old Spacebar', key({ key: 'Spacebar', code: '' }), true],
    ['space without shift', key({ shiftKey: false }), false],
    ['ctrl+shift+space', key({ ctrlKey: true }), false],
    ['alt', key({ altKey: true }), false],
    ['meta', key({ metaKey: true }), false],
    ['repeat', key({ repeat: true }), false],
    ['already handled', key({ defaultPrevented: true }), false],
    ['shift+a', key({ key: 'A', code: 'KeyA' }), false],
    ['inside an xterm window (INSERT)', key({ target: el(true) }), false],
    ['contenteditable', key({ target: el(false, true) }), false],
    ['no target', key({ target: undefined }), true],
  ])('%s', (_n, e, want) => {
    expect(isSummonKey(e)).toBe(want);
  });

  it('isTyping ignores non-elements', () => {
    expect(isTyping('x')).toBe(false);
    expect(isTyping({})).toBe(false);
  });
});

class FakeTarget implements KeyTarget {
  fns = new Set<(e: Event) => void>();
  addEventListener(_t: string, fn: (e: Event) => void) {
    this.fns.add(fn);
  }
  removeEventListener(_t: string, fn: (e: Event) => void) {
    this.fns.delete(fn);
  }
  press(k: Partial<KeyLike>) {
    let prevented = false;
    const e = { ...key(k), preventDefault: () => (prevented = true) } as unknown as Event;
    for (const fn of this.fns) fn(e);
    return prevented;
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('installInboxKeys', () => {
  it('shift+space runs inbox.summon and hands the pick to the hub', async () => {
    const t = new FakeTarget();
    const runs: string[] = [];
    const hub = createInboxHub(() => 0);
    const picks: string[] = [];
    hub.subscribe((e) => e.kind === 'summoned' && picks.push(e.sessionId));
    const sync = {
      run: async (name: string): Promise<Result> => {
        runs.push(name);
        return { toast: '→ x', data: { session_id: 'mig', label: 'migração do banco' } };
      },
    };
    const off = installInboxKeys(t, sync, hub);
    expect(t.press({})).toBe(true);
    expect(t.press({ shiftKey: false })).toBe(false);
    expect(t.press({ target: el(true) })).toBe(false);
    await tick();
    expect(runs).toEqual(['inbox.summon']);
    expect(picks).toEqual(['mig']);
    off();
    expect(t.fns.size).toBe(0);
  });

  it('nobody waiting / failures: no pick', async () => {
    const t = new FakeTarget();
    const hub = createInboxHub(() => 0);
    const picks: string[] = [];
    hub.subscribe((e) => e.kind === 'summoned' && picks.push(e.sessionId));
    let fail = false;
    installInboxKeys(
      t,
      {
        run: async () => {
          if (fail) throw new Error('down');
          return { toast: 'ninguém te esperando ✨' };
        },
      },
      hub,
    );
    t.press({});
    fail = true;
    t.press({});
    await tick();
    expect(picks).toEqual([]);
  });
});
