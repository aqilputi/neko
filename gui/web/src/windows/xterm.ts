// The DOM side of a terminal window: an xterm.js Terminal with the WebGL
// renderer (DOM renderer when WebGL is unavailable or its context is lost)
// and the fit addon (the view follows its element's size; the session
// sends the resize frame). Keys go through `intercept` first: true keeps
// one from the PTY (ctrl+], ctrl+k, a pending confirmation).

import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal, type ITheme } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import type { Theme } from '../client/types.ts';
import type { TermView } from './term.ts';

export interface XtermView extends TermView {
  readonly renderer: 'webgl' | 'dom';
  setTheme(theme: Theme): void;
  fit(): void;
  dispose(): void;
}

/** Terminal colours from the theme tokens (the ANSI 16 approximated). */
export function xtermTheme(t: Theme): ITheme {
  const k = t.tokens;
  return {
    background: k.surface ?? k.bg,
    foreground: k.text,
    cursor: k.accent,
    cursorAccent: k.bg,
    selectionBackground: k.overlay,
    black: k.overlay,
    red: k.blocked,
    green: k.done,
    yellow: k.cat_eyes,
    blue: k.accent2,
    magenta: k.accent,
    cyan: k.busy,
    white: k.subtle,
    brightBlack: k.muted,
    brightRed: k.error,
    brightGreen: k.cost_low,
    brightYellow: k.cat_eyes,
    brightBlue: k.accent2,
    brightMagenta: k.accent,
    brightCyan: k.busy,
    brightWhite: k.text,
  };
}

export function createXtermView(el: HTMLElement, theme: Theme, intercept: (ev: KeyboardEvent) => boolean): XtermView {
  const term = new Terminal({
    fontFamily: "'JetBrains Mono', 'Fira Code', ui-monospace, monospace",
    fontSize: 14,
    lineHeight: 1.1,
    cursorBlink: true,
    scrollback: 5000,
    allowProposedApi: true,
    theme: xtermTheme(theme),
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(el);

  let renderer: 'webgl' | 'dom' = 'dom';
  try {
    const gl = new WebglAddon();
    gl.onContextLoss(() => {
      gl.dispose(); // back to the DOM renderer
      renderer = 'dom';
    });
    term.loadAddon(gl);
    renderer = 'webgl';
  } catch {
    renderer = 'dom';
  }

  // a swallowed keydown must not come back as keypress/keyup
  let swallowed: string | null = null;
  term.attachCustomKeyEventHandler((ev) => {
    if (ev.type === 'keydown') {
      if (intercept(ev)) {
        swallowed = ev.key;
        ev.preventDefault();
        return false;
      }
      swallowed = null;
      return true;
    }
    if (swallowed !== null && ev.key === swallowed) {
      if (ev.type === 'keyup') swallowed = null;
      return false;
    }
    return true;
  });

  let frame = 0;
  const refit = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (el.clientWidth > 0 && el.clientHeight > 0) {
        try {
          fit.fit();
        } catch {
          // not laid out yet
        }
      }
    });
  };
  const ro = new ResizeObserver(refit);
  ro.observe(el);
  refit();

  return {
    get cols() {
      return term.cols;
    },
    get rows() {
      return term.rows;
    },
    get renderer() {
      return renderer;
    },
    write: (d) => term.write(d),
    onData: (cb) => term.onData(cb),
    onBinary: (cb) => term.onBinary(cb),
    onResize: (cb) => term.onResize(cb),
    onTitleChange: (cb) => term.onTitleChange(cb),
    focus: () => term.focus(),
    blur: () => term.blur(),
    text() {
      const b = term.buffer.active;
      const lines: string[] = [];
      for (let i = 0; i < b.length; i++) lines.push(b.getLine(i)?.translateToString(true) ?? '');
      return lines.join('\n').trimEnd();
    },
    setTheme: (t) => {
      term.options.theme = xtermTheme(t);
    },
    fit: refit,
    dispose() {
      ro.disconnect();
      if (frame) cancelAnimationFrame(frame);
      term.dispose();
    },
  };
}
