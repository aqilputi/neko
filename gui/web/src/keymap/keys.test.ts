import { describe, expect, it } from 'vitest';
import { isLeaveInsert, keyString, type KeyLike } from './keys.ts';

describe('keyString', () => {
  const cases: Array<[string, KeyLike, string | null]> = [
    ['letter', { key: 'j' }, 'j'],
    ['shifted letter keeps its case', { key: 'G', shiftKey: true }, 'G'],
    ['shifted symbol', { key: ':', shiftKey: true }, ':'],
    ['question mark', { key: '?', shiftKey: true }, '?'],
    ['slash', { key: '/' }, '/'],
    ['digit', { key: '1' }, '1'],
    ['enter', { key: 'Enter' }, 'enter'],
    ['escape', { key: 'Escape' }, 'esc'],
    ['tab', { key: 'Tab' }, 'tab'],
    ['shift+tab', { key: 'Tab', shiftKey: true }, 'shift+tab'],
    ['space', { key: ' ' }, 'space'],
    ['shift+space', { key: ' ', shiftKey: true }, 'shift+space'],
    ['ctrl+space', { key: ' ', ctrlKey: true }, 'ctrl+space'],
    ['arrows', { key: 'ArrowDown' }, 'down'],
    ['left', { key: 'ArrowLeft' }, 'left'],
    ['page down', { key: 'PageDown' }, 'pgdown'],
    ['page up', { key: 'PageUp' }, 'pgup'],
    ['alt+arrow', { key: 'ArrowUp', altKey: true }, 'alt+up'],
    ['shift+alt+arrow', { key: 'ArrowUp', altKey: true, shiftKey: true }, 'alt+shift+up'],
    ['ctrl+k', { key: 'k', ctrlKey: true }, 'ctrl+k'],
    ['ctrl+shift+K', { key: 'K', ctrlKey: true, shiftKey: true }, 'ctrl+shift+k'],
    ['ctrl+/', { key: '/', ctrlKey: true }, 'ctrl+/'],
    ['ctrl+_ drops shift', { key: '_', ctrlKey: true, shiftKey: true }, 'ctrl+_'],
    ['ctrl+]', { key: ']', ctrlKey: true }, 'ctrl+]'],
    ['alt+x', { key: 'x', altKey: true }, 'alt+x'],
    ['f-key', { key: 'F5' }, 'f5'],
    ['lone shift', { key: 'Shift', shiftKey: true }, null],
    ['lone control', { key: 'Control', ctrlKey: true }, null],
    ['dead key', { key: 'Dead' }, null],
    ['composing', { key: 'a', isComposing: true }, null],
    ['super combo', { key: 'r', metaKey: true }, null],
    ['unknown named key', { key: 'MediaPlayPause' }, null],
    ['accented letter', { key: 'ç' }, 'ç'],
  ];
  it.each(cases)('%s', (_n, ev, want) => {
    expect(keyString(ev)).toBe(want);
  });
});

describe('isLeaveInsert', () => {
  it.each([
    [{ key: ']', ctrlKey: true }, true],
    [{ key: '5', ctrlKey: true, code: 'BracketRight' }, true],
    [{ key: ']' }, false],
    [{ key: ']', ctrlKey: true, altKey: true }, false],
    [{ key: 'k', ctrlKey: true }, false],
  ] as Array<[KeyLike, boolean]>)('%j → %s', (ev, want) => {
    expect(isLeaveInsert(ev)).toBe(want);
  });
});
