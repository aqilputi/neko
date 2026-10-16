// The terminal windows: DOM boxes over the scene, dragged by the title bar
// (with a 2° lift), raised on focus, zoomed with z / double-click, closed
// with w / ✕, resized from the corner grip. A focused terminal is INSERT.

import { useEffect, useRef, useState } from 'preact/hooks';
import type { NkClient } from '../client/client.ts';
import { chromeInsets } from '../chrome/metrics.ts';
import type { KeyController } from '../keymap/controller.ts';
import type { Ui } from '../keymap/ui.ts';
import { FALLBACK_THEME } from '../state/theme.ts';
import type { Store } from '../state/store.ts';
import type { Sync } from '../state/sync.ts';
import { stateToken } from '../state/theme.ts';
import { shownRect, windowArea, type Rect, type Win } from './layout.ts';
import type { WindowManager } from './manager.ts';
import { createXtermView, type XtermView } from './xterm.ts';
import './windows.css';

export interface WindowLayerProps {
  store: Store;
  client?: NkClient;
  sync?: Sync;
  manager?: WindowManager;
  keys?: KeyController;
  ui?: Ui;
}

/** What the layer needs, set once by main (or passed as props). */
export interface WindowWiring {
  manager: WindowManager;
  keys: KeyController;
  ui: Ui;
}

let wiring: WindowWiring | null = null;

export function setWindowWiring(w: WindowWiring | null): void {
  wiring = w;
}

export function getWindowWiring(): WindowWiring | null {
  return wiring;
}

function inXterm(el: EventTarget | null): boolean {
  return el instanceof Element && !!el.closest('.xterm');
}

interface TermWindowProps {
  win: Win;
  area: Rect;
  active: boolean;
  store: Store;
  manager: WindowManager;
  keys: KeyController;
  ui: Ui;
}

function TermWindow({ win, area, active, store, manager, keys, ui }: TermWindowProps) {
  const body = useRef<HTMLDivElement>(null);
  const view = useRef<XtermView | null>(null);
  const [drag, setDrag] = useState<null | { px: number; py: number; x: number; y: number }>(null);
  const [grip, setGrip] = useState<null | { px: number; py: number; w: number; h: number }>(null);
  const theme = store.state.value.theme ?? FALLBACK_THEME;
  const session = store.sessionsById.value.get(win.id);

  useEffect(() => {
    const el = body.current;
    if (!el) return;
    let v: XtermView;
    try {
      v = createXtermView(el, store.state.value.theme ?? FALLBACK_THEME, (ev) => keys.handleInsertKey(ev));
    } catch (e) {
      manager.fail(win.id, `terminal indisponível: ${e instanceof Error ? e.message : String(e)}`);
      return;
    }
    view.current = v;
    el.dataset.renderer = v.renderer;
    manager.mount(win.id, v);
    return () => {
      manager.unmount(win.id);
      v.dispose();
      view.current = null;
    };
  }, [win.id]);

  useEffect(() => {
    view.current?.setTheme(theme);
  }, [theme]);

  const r = shownRect(win, area);

  const onBarDown = (e: PointerEvent) => {
    if (e.button !== 0 || (e.target as Element).closest('button') || win.zoomed) return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    manager.raise(win.id);
    setDrag({ px: e.clientX, py: e.clientY, x: r.x, y: r.y });
    e.preventDefault();
  };
  const onBarMove = (e: PointerEvent) => {
    if (drag) manager.moveTo(win.id, drag.x + e.clientX - drag.px, drag.y + e.clientY - drag.py);
  };
  const endDrag = () => setDrag(null);

  const onGripDown = (e: PointerEvent) => {
    if (e.button !== 0 || win.zoomed) return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    setGrip({ px: e.clientX, py: e.clientY, w: r.w, h: r.h });
    e.preventDefault();
  };
  const onGripMove = (e: PointerEvent) => {
    if (grip) manager.resize(win.id, grip.w + e.clientX - grip.px, grip.h + e.clientY - grip.py);
  };

  const cls = ['nk-win', active && 'is-active', drag && 'is-dragging', win.zoomed && 'is-zoomed', `is-${win.status}`]
    .filter(Boolean)
    .join(' ');
  return (
    <section
      class={cls}
      data-testid="window"
      data-session={win.id}
      aria-label={win.title}
      style={{ left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px`, zIndex: 10 + win.z }}
      onPointerDown={() => manager.raise(win.id)}
      onFocusIn={(e) => {
        if (inXterm(e.target)) {
          manager.raise(win.id);
          ui.mode.value = 'insert';
        }
      }}
      onFocusOut={(e) => {
        if (!inXterm(e.relatedTarget)) ui.mode.value = 'normal';
      }}
    >
      <header
        class="nk-win-bar"
        onPointerDown={onBarDown}
        onPointerMove={onBarMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDblClick={() => manager.toggleZoom(win.id)}
      >
        <span class="nk-win-dot" style={{ background: `var(--nk-${stateToken(session?.state)})` }} />
        <strong class="nk-win-title">{win.title}</strong>
        {win.subtitle && <span class="nk-win-sub">{win.subtitle}</span>}
        <span class="nk-win-gap" />
        <button class="nk-win-btn" title="zoom (z)" aria-label="zoom" onClick={() => manager.toggleZoom(win.id)}>
          {win.zoomed ? '⤡' : '⤢'}
        </button>
        <button class="nk-win-btn nk-win-close" title="fechar (w)" aria-label="fechar" onClick={() => manager.close(win.id)}>
          ✕
        </button>
      </header>
      <div class="nk-win-body" ref={body} data-testid="terminal" />
      {win.status === 'opening' && <div class="nk-win-note">abrindo…</div>}
      {win.status === 'error' && (
        <div class="nk-win-note nk-win-error" role="alert">
          {win.error}
        </div>
      )}
      {!win.zoomed && (
        <div
          class="nk-win-grip"
          aria-hidden="true"
          onPointerDown={onGripDown}
          onPointerMove={onGripMove}
          onPointerUp={() => setGrip(null)}
          onPointerCancel={() => setGrip(null)}
        />
      )}
    </section>
  );
}

export function WindowLayer(props: WindowLayerProps) {
  const w = getWindowWiring();
  const manager = props.manager ?? w?.manager;
  const keys = props.keys ?? w?.keys;
  const ui = props.ui ?? w?.ui;
  const treeOpen = ui?.treeOpen.value ?? true;

  useEffect(() => {
    if (!manager) return;
    const measure = () => manager.setArea(windowArea(window.innerWidth, window.innerHeight, chromeInsets(treeOpen)));
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  }, [manager, treeOpen]);

  const snap = props.store.state.value.snapshot;
  useEffect(() => {
    manager?.syncLabels();
  }, [snap]);

  if (!manager || !keys || !ui) return null;
  const st = manager.state.value;
  const area = manager.area.value;
  return (
    <div class="nk-windows" data-testid="windows">
      {st.wins.map((win) => (
        <TermWindow key={win.id} win={win} area={area} active={st.active === win.id} store={props.store} manager={manager} keys={keys} ui={ui} />
      ))}
    </div>
  );
}
