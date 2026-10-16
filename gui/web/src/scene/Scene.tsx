// <Scene>: the background layer. Derives the SceneModel from the store and
// hands it to the WebGL SceneView (render on demand), or shows the flat
// text scene when WebGL is unavailable or the user asked for it (view.flat).

import { signal } from '@preact/signals';
import { WebGLRenderer } from 'three';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { FALLBACK_THEME, type Store } from '../state/index.ts';
import { FlatScene } from './Flat.tsx';
import { browserDeps } from './loop.ts';
import { buildScene, modelKey, posesById, type Pose, type SceneModel } from './model.ts';
import { SceneView, type OverlayItem } from './view.ts';
import './scene.css';

export interface SceneProps {
  store: Store;
  onOpenSession(id: string): void;
}

/**
 * The 2D/flat mode switch (command `view.flat`): the chrome/keymap flips
 * it; `?flat=1` in the URL forces it too. WebGL failure falls back to flat
 * on its own.
 */
export const sceneFlat = signal(false);

export function toggleSceneFlat(): void {
  sceneFlat.value = !sceneFlat.value;
}

function urlFlat(): boolean {
  try {
    const q = new URLSearchParams(location.search);
    return q.get('flat') === '1' || q.get('scene') === 'flat';
  } catch {
    return false;
  }
}

/** Read-only debug view of the scene (window.__nkScene; tests and devtools). */
export interface SceneDebug {
  readonly mode: 'webgl' | 'flat';
  readonly cats: number;
  readonly kittens: number;
  readonly poses: Record<string, Pose>;
  readonly frames: number;
  /** Frames with ambient motion (0 forever under prefers-reduced-motion). */
  readonly ambientFrames: number;
  readonly reducedMotion: boolean;
  screenPos(id: string): { x: number; y: number } | null;
}

function useReducedMotion(): boolean {
  const q = useMemo(() => (typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : null), []);
  const [r, setR] = useState(q?.matches ?? false);
  useEffect(() => {
    if (!q) return;
    const on = () => setR(q.matches);
    q.addEventListener('change', on);
    return () => q.removeEventListener('change', on);
  }, [q]);
  return r;
}

/** A WebGL renderer for the canvas, or null when the context can't be made. */
function makeRenderer(canvas: HTMLCanvasElement): WebGLRenderer | null {
  try {
    const r = new WebGLRenderer({ canvas, antialias: true, powerPreference: 'low-power' });
    return r.getContext() ? r : null;
  } catch {
    return null;
  }
}

export function Scene({ store, onOpenSession }: SceneProps) {
  const s = store.state.value;
  const selected = store.cursorRow.value?.session_id ?? null;
  const theme = s.theme ?? FALLBACK_THEME;
  const reduced = useReducedMotion();
  const [hovered, setHovered] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [tick, setTick] = useState(0);
  const flat = sceneFlat.value || failed || urlFlat();

  const model = useMemo(
    () => buildScene(s.snapshot, s.tree.rows, theme, Date.now(), { hovered, selected }),
    // tick: a ✉ stopped blinking (model.nextChangeAt)
    [s.snapshot, s.tree.rows, theme, hovered, selected, tick],
  );

  // rebuild when the model changes by itself (✉ blink window ends)
  useEffect(() => {
    if (model.nextChangeAt === null) return;
    const t = setTimeout(() => setTick((n) => n + 1), Math.max(50, model.nextChangeAt - Date.now() + 20));
    return () => clearTimeout(t);
  }, [model.nextChangeAt]);

  const modelRef = useRef<SceneModel>(model);
  modelRef.current = model;
  const viewRef = useRef<SceneView | null>(null);
  const reducedRef = useRef(reduced);
  reducedRef.current = reduced;

  // the debug hook, for whichever mode is on
  useEffect(() => {
    const debug: SceneDebug = {
      get mode() {
        return viewRef.current ? 'webgl' : 'flat';
      },
      get cats() {
        return viewRef.current?.catCount ?? modelRef.current.cats.length;
      },
      get kittens() {
        return viewRef.current?.kittenTotal ?? modelRef.current.cats.reduce((n, c) => n + c.kittens.length, 0);
      },
      get poses() {
        return viewRef.current?.poses() ?? posesById(modelRef.current);
      },
      get frames() {
        return viewRef.current?.frames ?? 0;
      },
      get ambientFrames() {
        return viewRef.current?.ambientFrames ?? 0;
      },
      get reducedMotion() {
        return reducedRef.current;
      },
      screenPos(id: string) {
        return viewRef.current?.catScreen(id) ?? null;
      },
    };
    Object.defineProperty(globalThis, '__nkScene', { value: debug, configurable: true });
    return () => {
      delete (globalThis as { __nkScene?: unknown }).__nkScene;
    };
  }, []);

  if (flat) return <FlatScene model={model} onOpenSession={onOpenSession} reason={failed ? 'webgl' : 'flat'} />;
  return (
    <WebGLScene
      model={model}
      reduced={reduced}
      hovered={hovered}
      setHovered={setHovered}
      onOpenSession={onOpenSession}
      onFail={() => setFailed(true)}
      viewRef={viewRef}
    />
  );
}

interface WebGLProps {
  model: SceneModel;
  reduced: boolean;
  hovered: string | null;
  setHovered(id: string | null): void;
  onOpenSession(id: string): void;
  onFail(): void;
  viewRef: { current: SceneView | null };
}

function WebGLScene({ model, reduced, hovered, setHovered, onOpenSession, onFail, viewRef }: WebGLProps) {
  const host = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [ready, setReady] = useState(false);
  const [overlay, setOverlay] = useState<OverlayItem[]>([]);
  const lastKey = useRef('');
  const down = useRef<{ x: number; y: number } | null>(null);

  // create the renderer and the view once
  useEffect(() => {
    let view: SceneView | null = null;
    let ro: ResizeObserver | null = null;
    const onVis = () => view?.setVisible(document.visibilityState !== 'hidden');
    const onLost = (e: Event) => {
      e.preventDefault();
      onFail();
    };
    const r = makeRenderer(canvas.current!);
    if (!r) {
      onFail();
    } else {
      view = new SceneView({ renderer: r, deps: browserDeps(), reducedMotion: reduced, dpr: devicePixelRatio });
      viewRef.current = view;
      const el = host.current!;
      const resize = () => {
        view!.setSize(el.clientWidth, el.clientHeight);
        setOverlay(view!.overlay());
      };
      resize();
      ro = new ResizeObserver(resize);
      ro.observe(el);
      document.addEventListener('visibilitychange', onVis);
      canvas.current!.addEventListener('webglcontextlost', onLost);
      onVis();
      setReady(true);
    }
    return () => {
      ro?.disconnect();
      document.removeEventListener('visibilitychange', onVis);
      canvas.current?.removeEventListener('webglcontextlost', onLost);
      view?.dispose();
      viewRef.current = null;
      lastKey.current = '';
    };
  }, []);

  useEffect(() => {
    viewRef.current?.setReducedMotion(reduced);
  }, [reduced, ready]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const key = modelKey(model);
    if (key === lastKey.current) return;
    lastKey.current = key;
    view.setModel(model);
    setOverlay(view.overlay());
  }, [model, ready]);

  const ndc = (e: PointerEvent | MouseEvent) => {
    const r = canvas.current!.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1] as const;
  };
  const onMove = (e: PointerEvent) => {
    const view = viewRef.current;
    if (!view) return;
    const id = view.pick(...ndc(e));
    if (id !== hovered) {
      setHovered(id);
      if (id) view.wiggle();
    }
  };
  const onDown = (e: PointerEvent) => {
    down.current = { x: e.clientX, y: e.clientY };
  };
  const onUp = (e: PointerEvent) => {
    const d = down.current;
    down.current = null;
    if (!d || Math.hypot(e.clientX - d.x, e.clientY - d.y) > 6) return;
    const id = viewRef.current?.pick(...ndc(e));
    if (id) onOpenSession(id);
  };

  const hoverCat = hovered ? model.cats.find((c) => c.id === hovered) : undefined;
  const tip = hoverCat ? viewRef.current?.catScreen(hoverCat.id) : null;

  return (
    <div class="nk-scene" ref={host} data-testid="scene" data-mode="webgl">
      <canvas
        ref={canvas}
        class={hovered ? 'nk-scene-canvas hot' : 'nk-scene-canvas'}
        onPointerMove={onMove}
        onPointerLeave={() => hovered && setHovered(null)}
        onPointerDown={onDown}
        onPointerUp={onUp}
        aria-label="cena: um gato por sessão"
      />
      <div class={reduced ? 'nk-scene-overlay still' : 'nk-scene-overlay'} aria-hidden="true">
        {overlay.map((o) => (
          <span
            key={o.key}
            class={`nk-ov nk-ov-${o.kind}${o.blink ? ' blink' : ''}${o.dim ? ' dim' : ''}`}
            style={{ left: `${o.x}px`, top: `${o.y}px`, '--ov': `var(--nk-${o.token})` }}
          >
            {o.text}
          </span>
        ))}
        {hoverCat && tip && (
          <span class="nk-tip" data-testid="scene-tip" style={{ left: `${tip.x}px`, top: `${tip.y}px` }}>
            {hoverCat.label}
            <small>{hoverCat.state}</small>
          </span>
        )}
      </div>
    </div>
  );
}
