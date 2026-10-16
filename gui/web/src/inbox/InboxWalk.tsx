// The calçada (the ~72 px strip at the bottom) and its inbox cats. The
// queue (queue.ts) says who walks where and when; this component turns it
// into DOM: each walker is moved by one CSS transition from where it is to
// its stop, its legs swap by a CSS animation only while it walks, and the
// next change is one setTimeout away (no rAF loop, nothing ticking when
// everyone has arrived). With prefers-reduced-motion the cats don't walk:
// a static badge in the calçada's corner lists the same items.

import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { Store } from '../state/store.ts';
import { inboxHub as defaultHub, type InboxHub } from './hub.ts';
import {
  dismissSession,
  emptyQueue,
  evenLayout,
  nextWake,
  notePeer,
  ordered,
  overflow,
  phaseOf,
  sourceOf,
  SPEED,
  START_X,
  step,
  summon,
  visible,
  xAt,
  type InboxQueue,
  type Walker,
} from './queue.ts';
import { CatSprite } from './sprite.tsx';
import './inbox.css';

export interface InboxWalkProps {
  store: Store;
  /** The app's Sync (only `run` is used, by installInboxKeys's caller). */
  sync?: unknown;
  onOpenSession(id: string): void;
  /** x (px, viewport) under a session's cat/window, from the scene; walkers
   * stop under it. Without it (or for unknown sessions) they spread evenly. */
  positionOf?(sessionId: string): number | undefined;
  /** Where bus events come from (tests); defaults to the app's hub. */
  hub?: InboxHub;
}

const REDUCED = '(prefers-reduced-motion: reduce)';

function useReducedMotion(): boolean {
  const mq = typeof matchMedia === 'function' ? matchMedia(REDUCED) : null;
  const [reduced, setReduced] = useState(mq?.matches ?? false);
  useEffect(() => {
    if (!mq) return;
    const on = () => setReduced(mq.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  }, []);
  return reduced;
}

function useWidth(): number {
  const [w, setW] = useState(() => (typeof innerWidth === 'number' ? innerWidth : 1024));
  useEffect(() => {
    let t: ReturnType<typeof setTimeout> | null = null;
    const on = () => {
      if (t !== null) clearTimeout(t);
      t = setTimeout(() => setW(innerWidth), 120);
    };
    addEventListener('resize', on);
    return () => {
      removeEventListener('resize', on);
      if (t !== null) clearTimeout(t);
    };
  }, []);
  return w;
}

const ICON: Record<Walker['kind'], string> = { blocked: '!', returned: '!', idle: '…', mail: '✉' };
const WHAT: Record<Walker['kind'], string> = {
  blocked: 'te esperando',
  returned: 'voltou',
  idle: 'parada',
  mail: 'mensagem',
};

function describe(w: Walker): string {
  return `${w.label} — ${WHAT[w.kind]}${w.text ? `: ${w.text}` : ''}`;
}

function WalkerView({ w, now, glow, onOpen }: { w: Walker; now: number; glow: boolean; onOpen(w: Walker): void }) {
  const ref = useRef<HTMLButtonElement>(null);
  const phase = phaseOf(w, now);
  // one transition from where it is now to its stop (delayed until its
  // turn in the single file); the browser does the frames
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const t = Date.now();
    const x = xAt(w, t);
    el.style.transition = 'none';
    el.style.transform = `translateX(${x}px)`;
    if (x === w.to || w.startAt === null) return;
    void el.getBoundingClientRect(); // commit the start before transitioning
    const delay = Math.max(0, w.startAt - t);
    const dur = Math.abs(w.to - x) / SPEED;
    el.style.transition = `transform ${Math.round(dur)}ms linear ${Math.round(delay)}ms`;
    el.style.transform = `translateX(${w.to}px)`;
  }, [w.from, w.to, w.startAt]);
  const cls = ['nk-walker', `nk-walker-${w.kind}`, `nk-walker-${phase}`, glow ? 'nk-walker-summoned' : ''].filter(Boolean).join(' ');
  return (
    <button
      ref={ref}
      type="button"
      class={cls}
      style={{ transform: `translateX(${START_X}px)` }}
      data-testid="inbox-walker"
      data-session={w.sessionId}
      data-kind={w.kind}
      data-phase={phase}
      aria-label={describe(w)}
      title={describe(w)}
      onClick={() => onOpen(w)}
    >
      <span class="nk-bubble" aria-hidden="true">
        {ICON[w.kind]}
      </span>
      <CatSprite walking={phase === 'walking'} left={w.to < w.from} />
      <span class="nk-walker-label">{w.label}</span>
    </button>
  );
}

function Badge({ q, glowId, onOpen }: { q: InboxQueue; glowId: string | null; onOpen(w: Walker): void }) {
  const all = ordered(q);
  if (all.length === 0) return null;
  return (
    <ul class="nk-inbox-badge" data-testid="inbox-badge" aria-label="inbox">
      {all.map((w) => (
        <li key={w.key}>
          <button
            type="button"
            class={`nk-badge-item nk-walker-${w.kind}${glowId === w.sessionId ? ' nk-walker-summoned' : ''}`}
            data-testid="inbox-walker"
            data-session={w.sessionId}
            data-kind={w.kind}
            data-phase="arrived"
            title={describe(w)}
            onClick={() => onOpen(w)}
          >
            <span class="nk-bubble">{ICON[w.kind]}</span> {w.label}
          </button>
        </li>
      ))}
    </ul>
  );
}

/** The inbox cats on the calçada. */
export function InboxWalk(props: InboxWalkProps) {
  const { store, onOpenSession, positionOf } = props;
  const hub = props.hub ?? defaultHub;
  const snapshot = store.state.value.snapshot; // subscribes the component
  const reduced = useReducedMotion();
  const width = useWidth();
  const [q, setQ] = useState<InboxQueue>(emptyQueue);
  const [wake, setWake] = useState(0);
  const [lastOpen, setLastOpen] = useState('');
  const openRef = useRef(onOpenSession);
  openRef.current = onOpenSession;
  const layout = evenLayout(width, positionOf);

  // the source, the clock or the width changed: step the queue
  useEffect(() => {
    setQ((cur) => step(cur, sourceOf(snapshot), Date.now(), layout));
  }, [snapshot, width, wake]);

  // sleep until the next thing that changes by itself
  useEffect(() => {
    const now = Date.now();
    const at = nextWake(q, now);
    if (at === null) return;
    const t = setTimeout(() => setWake((n) => n + 1), Math.max(0, at - now) + 16);
    return () => clearTimeout(t);
  }, [q]);

  const open = (id: string) => {
    setLastOpen(id);
    openRef.current(id);
  };

  // peer.message texts and inbox.summoned picks
  useEffect(
    () =>
      hub.subscribe((e) => {
        if (e.kind === 'peer') {
          setQ((cur) => notePeer(cur, e.to, e.from, e.text));
          return;
        }
        setQ((cur) => summon(cur, e.sessionId, Date.now()));
        open(e.sessionId);
      }),
    [hub],
  );

  const onWalker = (w: Walker) => {
    setQ((cur) => dismissSession(cur, w.sessionId));
    open(w.sessionId);
  };

  const now = Date.now();
  const glowId = q.highlight && now < q.highlight.until ? q.highlight.sessionId : null;
  const vis = visible(q);
  const more = overflow(q);
  return (
    <div
      class={`nk-calcada${reduced ? ' nk-calcada-still' : ''}`}
      data-testid="inbox-walk"
      data-last-open={lastOpen}
      data-summoned={q.highlight?.sessionId ?? ''}
      aria-label="calçada da inbox"
    >
      {reduced ? (
        <Badge q={q} glowId={glowId} onOpen={onWalker} />
      ) : (
        <>
          {vis.map((w) => (
            <WalkerView key={w.key} w={w} now={now} glow={glowId === w.sessionId} onOpen={onWalker} />
          ))}
          {more > 0 && (
            <span class="nk-inbox-more" data-testid="inbox-more" title={`mais ${more} na inbox`}>
              +{more}
            </span>
          )}
        </>
      )}
    </div>
  );
}
