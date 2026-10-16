// KeyboardEvent → key string in the notation the core's keymap uses (the
// TUI's Bubble Tea v2 keystrokes): "j", "G", ":", "enter", "esc", "tab",
// "shift+tab", "space", "shift+space", "ctrl+k", "ctrl+/", "pgdown", "up"…
// Pure: takes the plain fields of the event, so tests need no DOM.

/** The fields of a KeyboardEvent the notation depends on. */
export interface KeyLike {
  key: string;
  code?: string;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
  isComposing?: boolean;
}

/** DOM key names → the TUI's names. */
const NAMED: Record<string, string> = {
  Enter: 'enter',
  Escape: 'esc',
  Tab: 'tab',
  Backspace: 'backspace',
  Delete: 'delete',
  Insert: 'insert',
  ArrowUp: 'up',
  ArrowDown: 'down',
  ArrowLeft: 'left',
  ArrowRight: 'right',
  PageUp: 'pgup',
  PageDown: 'pgdown',
  Home: 'home',
  End: 'end',
  ' ': 'space',
  Spacebar: 'space',
};

/** Keys that are only modifiers or IME noise: never a keystroke. */
const IGNORED = new Set([
  'Shift',
  'Control',
  'Alt',
  'AltGraph',
  'Meta',
  'OS',
  'Super',
  'Hyper',
  'CapsLock',
  'NumLock',
  'ScrollLock',
  'Fn',
  'FnLock',
  'Dead',
  'Unidentified',
  'Process',
  'Compose',
]);

/**
 * The key string of an event, or null when it is not a keystroke the
 * keymap can bind (a lone modifier, IME composition, a super/meta combo
 * the desktop owns).
 *
 * Printable characters already carry shift ("G", ":", "?", "_"), so shift
 * is dropped for them unless ctrl/alt are also held on a letter
 * ("ctrl+shift+k"); named keys always spell it ("shift+tab", "shift+space").
 */
export function keyString(ev: KeyLike): string | null {
  if (ev.isComposing || IGNORED.has(ev.key) || ev.metaKey) return null;
  const ctrl = !!ev.ctrlKey;
  const alt = !!ev.altKey;
  const shift = !!ev.shiftKey;

  const named = NAMED[ev.key] ?? (/^F\d{1,2}$/.test(ev.key) ? ev.key.toLowerCase() : undefined);
  if (named) return mods(ctrl, alt, shift) + named;

  // one printable character (a grapheme may be two UTF-16 units)
  if ([...ev.key].length !== 1) return null;
  let ch = ev.key;
  if (ctrl || alt) {
    const lower = ch.toLowerCase();
    if (lower !== ch.toUpperCase()) {
      // a letter: ctrl+K is reported as ctrl+shift+k
      const sh = shift || ch !== lower;
      return mods(ctrl, alt, sh) + lower;
    }
    return mods(ctrl, alt, false) + ch;
  }
  return ch;
}

function mods(ctrl: boolean, alt: boolean, shift: boolean): string {
  return (ctrl ? 'ctrl+' : '') + (alt ? 'alt+' : '') + (shift ? 'shift+' : '');
}

/** ctrl+] — leaves INSERT (the TUI's escape from the embedded pane). */
export function isLeaveInsert(ev: KeyLike): boolean {
  return !!ev.ctrlKey && !ev.altKey && !ev.metaKey && (ev.key === ']' || ev.code === 'BracketRight');
}
