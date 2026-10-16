// The GUI's own interaction state (not the core's): NORMAL/INSERT, which
// overlay is open (palette, switcher, help, usage, filter), the sessions
// picked with space, and a sequence waiting for its second press. Signals,
// shared by the key controller, the chrome and the windows.

import { signal, type Signal } from '@preact/signals';
import { IDLE, type SeqState } from './resolve.ts';

export type Mode = 'normal' | 'insert';

export type Overlay =
  | { kind: 'palette'; query: string }
  | { kind: 'switcher' }
  | { kind: 'help' }
  | { kind: 'usage' }
  | { kind: 'filter'; value: string }
  | null;

export interface Ui {
  mode: Signal<Mode>;
  overlay: Signal<Overlay>;
  picked: Signal<ReadonlySet<string>>;
  seq: Signal<SeqState>;
  /** The left tree column is shown. */
  treeOpen: Signal<boolean>;
}

export function createUi(): Ui {
  return {
    mode: signal<Mode>('normal'),
    overlay: signal<Overlay>(null),
    picked: signal<ReadonlySet<string>>(new Set()),
    seq: signal<SeqState>(IDLE),
    treeOpen: signal(true),
  };
}
