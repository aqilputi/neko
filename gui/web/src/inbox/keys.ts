// shift+space anywhere in the GUI → inbox.summon (the core picks the most
// urgent session and publishes inbox.summoned). Not while typing: inside a
// terminal window (INSERT, keys go to the PTY) or an editable field.

import type { Result } from '../client/types.ts';
import { inboxHub, isSummoned, type InboxHub } from './hub.ts';

export interface KeyLike {
  key: string;
  code?: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  repeat?: boolean;
  target?: unknown;
  defaultPrevented?: boolean;
}

interface ElementLike {
  closest?(sel: string): unknown;
  isContentEditable?: boolean;
  tagName?: string;
}

/** Where shift+space must stay a space: a terminal (xterm keeps its input
 * in a textarea inside .xterm) or anything editable. */
export const TYPING_SELECTOR = '.xterm, input, textarea, select, [contenteditable=""], [contenteditable="true"]';

export function isTyping(target: unknown): boolean {
  const el = target as ElementLike | null | undefined;
  if (!el || typeof el !== 'object') return false;
  if (el.isContentEditable) return true;
  return typeof el.closest === 'function' && !!el.closest(TYPING_SELECTOR);
}

export function isSummonKey(e: KeyLike): boolean {
  const space = e.key === ' ' || e.key === 'Spacebar' || e.code === 'Space';
  return space && e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey && !e.repeat && !e.defaultPrevented && !isTyping(e.target);
}

/** The bits of Sync this needs. */
export interface SummonSync {
  run(name: string, args?: Record<string, string>): Promise<Result>;
}

/** Where keydown is listened to (a Window, or a fake in tests). */
export interface KeyTarget {
  addEventListener(type: string, fn: (e: Event) => void, capture?: boolean): void;
  removeEventListener(type: string, fn: (e: Event) => void, capture?: boolean): void;
}

/** Listens for shift+space on `target`; returns the uninstaller. The pick
 * in the command's Result goes to the hub (the event may not reach us). */
export function installInboxKeys(target: KeyTarget, sync: SummonSync, hub: InboxHub = inboxHub): () => void {
  const onKey = (ev: Event) => {
    const e = ev as KeyboardEvent;
    if (!isSummonKey(e)) return;
    e.preventDefault();
    sync.run('inbox.summon').then(
      (r) => {
        if (isSummoned(r.data)) hub.summoned(r.data);
      },
      () => {}, // no answer, no pick: nothing for the cat to do
    );
  };
  target.addEventListener('keydown', onKey, true);
  return () => target.removeEventListener('keydown', onKey, true);
}
