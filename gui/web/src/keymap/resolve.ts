// Key → command resolution over GET /v1/keymap, mirroring core.Keymap:
// a mode's own bindings first, then its parent's ("normal.todos" →
// "normal" → "*"), and two-press sequences ("gg", "dd") that wait for the
// second press for a short time. Pure.

import type { Binding } from '../client/types.ts';

export const MODE_NORMAL = 'normal';
export const MODE_NORMAL_TODOS = 'normal.todos';
export const MODE_INSERT = 'insert';
export const MODE_ANY = '*';

/** How long the first press of a sequence waits for the second (ms). */
export const SEQUENCE_TIMEOUT_MS = 800;

/** core.ParentMode: "normal.todos" → "normal" → "*" → "". */
export function parentMode(mode: string): string {
  if (mode === MODE_ANY || mode === '') return '';
  const i = mode.lastIndexOf('.');
  return i >= 0 ? mode.slice(0, i) : MODE_ANY;
}

/** core.Keymap.Resolve: the command key runs in mode, or undefined. */
export function resolve(keymap: readonly Binding[], mode: string, key: string): string | undefined {
  for (let m = mode; m !== ''; m = parentMode(m)) {
    for (const b of keymap) {
      if (b.mode === m && b.keys?.includes(key)) return b.command;
    }
  }
  return undefined;
}

/** core.IsSequence: two presses of the same printable key ("gg"). */
export function isSequence(key: string): boolean {
  const cps = [...key];
  return cps.length === 2 && cps[0] === cps[1];
}

/** core.Keymap.StartsSequence: key is the first press of a bound sequence. */
export function startsSequence(keymap: readonly Binding[], mode: string, key: string): boolean {
  return isSequence(key + key) && resolve(keymap, mode, key + key) !== undefined;
}

/** The NORMAL mode for a tab (the todos tab has its own sub-mode). */
export function normalMode(tab: string | undefined): string {
  return tab === 'todos' ? MODE_NORMAL_TODOS : MODE_NORMAL;
}

/** A first press waiting for its second. */
export interface SeqState {
  pending: string | null;
  at: number;
}

export const IDLE: SeqState = { pending: null, at: 0 };

export interface Feed {
  state: SeqState;
  /** The key as resolved ("gg" for a completed sequence). */
  key: string;
  /** The command it runs; undefined while waiting or when unbound. */
  command?: string;
  /** The press only started a sequence. */
  waiting: boolean;
}

/**
 * Feeds one key press. A press that starts a sequence waits; the same key
 * within SEQUENCE_TIMEOUT_MS completes it; anything else drops the wait and
 * resolves on its own (so "g" then "j" moves down).
 */
export function feedKey(
  keymap: readonly Binding[],
  mode: string,
  st: SeqState,
  key: string,
  now: number,
  timeoutMs = SEQUENCE_TIMEOUT_MS,
): Feed {
  if (st.pending !== null && key === st.pending && now - st.at <= timeoutMs) {
    const seq = key + key;
    const command = resolve(keymap, mode, seq);
    return command ? { state: IDLE, key: seq, command, waiting: false } : { state: IDLE, key: seq, waiting: false };
  }
  if (startsSequence(keymap, mode, key)) {
    return { state: { pending: key, at: now }, key, waiting: true };
  }
  const command = resolve(keymap, mode, key);
  return command ? { state: IDLE, key, command, waiting: false } : { state: IDLE, key, waiting: false };
}

/** The keys bound to command (first binding), for hints. */
export function keysOf(keymap: readonly Binding[], command: string): string[] {
  return keymap.find((b) => b.command === command)?.keys ?? [];
}

/** The help label of command ("j/down"), or "". */
export function helpKey(keymap: readonly Binding[], command: string): string {
  const b = keymap.find((x) => x.command === command);
  return b?.help.key || (b?.keys ?? []).join('/');
}
