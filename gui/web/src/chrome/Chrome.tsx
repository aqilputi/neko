// The DOM chrome around the scene and the windows: top bar (counts and the
// 5h usage bar, as the TUI's header), the left tree column (rows of
// POST /v1/view/tree), the status bar (NORMAL/INSERT and key hints), the
// overlays (palette, switcher, filter, help, usage), the confirmation
// dialog and the toasts. Bold, few details, big hit targets.

import { useEffect, useRef } from 'preact/hooks';
import type { NkClient } from '../client/client.ts';
import type { TabName, TreeRow } from '../client/types.ts';
import type { KeyController } from '../keymap/controller.ts';
import { cursorIndex } from '../keymap/nav.ts';
import type { Ui } from '../keymap/ui.ts';
import type { NkState } from '../state/model.ts';
import { dismissToast } from '../state/reducers.ts';
import type { Store } from '../state/store.ts';
import type { Sync } from '../state/sync.ts';
import { FALLBACK_THEME, contextColor } from '../state/theme.ts';
import { getWindowWiring } from '../windows/WindowLayer.tsx';
import { counts, findLimit, hints, rowView, tabLabels, usageToken } from './format.ts';
import { ConfirmDialog, FilterBar, Help, Palette, Switcher, UsageSheet, type OverlayProps } from './Overlays.tsx';
import './chrome.css';

export interface ChromeProps {
  store: Store;
  client?: NkClient;
  sync?: Sync;
  keys?: KeyController;
  ui?: Ui;
}

export const STATUS_TEXT: Record<NkState['status'], string> = {
  connecting: 'conectando…',
  open: 'conectado',
  reconnecting: 'reconectando…',
  closed: 'desconectado',
  error: 'erro de conexão',
};

function TopBar({ store, ui }: { store: Store; ui: Ui }) {
  const s = store.state.value;
  const c = counts(s.snapshot?.sessions ?? []);
  const five = findLimit(s.snapshot?.usage, 'session');
  const week = findLimit(s.snapshot?.usage, 'all models');
  return (
    <header class="nk-top" data-testid="topbar">
      <button
        class="nk-icon-btn"
        title={ui.treeOpen.value ? 'esconde a árvore' : 'mostra a árvore'}
        aria-label="árvore"
        onClick={() => (ui.treeOpen.value = !ui.treeOpen.value)}
      >
        ☰
      </button>
      <span class="nk-brand">ᓚᘏᗢ nekuroddo</span>
      <span class="nk-pill nk-pill-busy" data-testid="count-busy">
        ● {c.busy} rodando
      </span>
      <span class="nk-pill nk-pill-blocked" data-testid="count-waiting">
        ◆ {c.waiting} te esperando
      </span>
      <span class="nk-pill nk-pill-done" data-testid="count-done">
        ✓ {c.done} voltaram
      </span>
      {five ? (
        <span class="nk-usage" title={five.name} data-testid="usage-5h">
          <span class="nk-muted">5h</span>
          <span class="nk-bar">
            <span class="nk-bar-fill" style={{ width: `${Math.min(five.pct, 100)}%`, background: `var(--nk-${usageToken(five.pct)})` }} />
          </span>
          <strong style={{ color: `var(--nk-${usageToken(five.pct)})` }}>{five.pct}%</strong>
          {five.resets && <span class="nk-muted">↻{five.resets}</span>}
          {week && (
            <>
              <span class="nk-muted">semana</span>
              <strong style={{ color: `var(--nk-${usageToken(week.pct)})` }}>{week.pct}%</strong>
            </>
          )}
        </span>
      ) : (
        <span class="nk-usage">
          <span class="nk-muted">hoje ≈</span>
          <strong>${(s.snapshot?.today_cost ?? 0).toFixed(2)}</strong>
        </span>
      )}
      <span class="nk-gap" />
      {s.error && <span class="nk-err error">⚠ {s.error}</span>}
      <span data-testid="status" class={`nk-conn status status-${s.status}`}>
        {STATUS_TEXT[s.status]}
      </span>
      <span class="nk-muted">{s.theme?.name}</span>
    </header>
  );
}

function Row({ row, cursor, picked, ctxColor, onClick, onOpen }: {
  row: TreeRow;
  cursor: boolean;
  picked: ReadonlySet<string>;
  ctxColor(i: number): string;
  onClick(): void;
  onOpen(): void;
}) {
  const v = rowView(row, picked);
  const cls = [
    'nk-row',
    `nk-row-${row.kind}`,
    cursor && 'is-cursor',
    v.picked && 'is-picked',
    row.stale && 'is-stale',
  ]
    .filter(Boolean)
    .join(' ');
  return (
    <li
      class={cls}
      data-key={row.key}
      data-testid={`row-${row.kind}`}
      aria-selected={cursor}
      role="treeitem"
      style={{ '--row-ctx': ctxColor(row.color), paddingLeft: `${10 + row.depth * 18}px` }}
      onClick={onClick}
      onDblClick={onOpen}
    >
      <span class="nk-row-glyph" style={{ color: `var(--nk-${v.token})` }}>
        {v.picked ? '✔' : v.glyph}
      </span>
      <span class="nk-row-text">{v.text}</span>
      {row.mail_state && (
        <span class={`nk-mail is-${row.mail_state}`} title={row.mail?.sent ? 'mandou ✉' : 'recebeu ✉'} data-testid="mail">
          ✉
        </span>
      )}
      {(row.groups ?? []).map((g) => (
        <span key={g.tag} class="nk-chip" style={{ '--chip': ctxColor(g.color) }}>
          #{g.tag}
        </span>
      ))}
      {v.badge && <span class="nk-row-badge">{v.badge}</span>}
      {v.cost && <span class="nk-row-cost">{v.cost}</span>}
    </li>
  );
}

function TreeColumn({ store, ui, keys }: { store: Store; ui: Ui; keys: KeyController }) {
  const s = store.state.value;
  const tree = s.tree;
  const cur = cursorIndex(tree);
  const theme = s.theme ?? FALLBACK_THEME;
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => {
    const el = list.current?.querySelector('.is-cursor');
    el?.scrollIntoView({ block: 'nearest' });
  }, [cur, tree.rows]);
  const tab: TabName = tree.state.tab ?? 'sessions';
  return (
    <aside class="nk-tree-col" data-testid="tree-column">
      <nav class="nk-tabs" role="tablist">
        {tabLabels(tree.inbox.length, tree.archived).map((t) => (
          <button
            key={t.tab}
            role="tab"
            aria-selected={t.tab === tab}
            class={`nk-tab${t.tab === tab ? ' is-on' : ''}${t.tab === 'inbox' && tree.inbox.length ? ' is-hot' : ''}`}
            onClick={() => void keys.refresh({ tab: t.tab })}
          >
            {t.label}
          </button>
        ))}
      </nav>
      <div class="nk-tree-info">
        {tree.state.show_all ? 'tudo' : 'ativas'} · ordem: {tree.state.sort ?? 'atividade'}
        {tree.hidden > 0 && ` · ${tree.hidden} ocultas (H)`}
        {tree.state.filter && <span class="nk-accent2"> · /{tree.state.filter}</span>}
      </div>
      <ul class="nk-tree" data-testid="tree" role="tree" ref={list}>
        {tree.rows.map((r, i) => (
          <Row
            key={r.key}
            row={r}
            cursor={i === cur}
            picked={ui.picked.value}
            ctxColor={(c) => contextColor(theme, c)}
            onClick={() => keys.moveTo(i)}
            onOpen={() => {
              keys.moveTo(i);
              void keys.execute('session.open');
            }}
          />
        ))}
        {tree.rows.length === 0 && <li class="nk-empty">nada aqui {tab === 'inbox' ? '— ninguém te esperando ✨' : ''}</li>}
      </ul>
    </aside>
  );
}

function StatusBar({ store, ui }: { store: Store; ui: Ui }) {
  const mode = ui.mode.value;
  const s = store.state.value;
  const seq = ui.seq.value.pending;
  const picked = ui.picked.value.size;
  return (
    <footer class="nk-status" data-testid="statusbar">
      <span class={`nk-mode nk-mode-${mode}`} data-testid="mode">
        {mode === 'insert' ? 'INSERT' : 'NORMAL'}
      </span>
      {seq && <kbd class="nk-seq">{seq}…</kbd>}
      {picked > 0 && <span class="nk-pill nk-pill-accent">{picked} selecionadas</span>}
      <span class="nk-hints">
        {hints(s.keymap, mode).map((h) => (
          <span key={h.key + h.label} class="nk-hint">
            <kbd>{h.key}</kbd> {h.label}
          </span>
        ))}
      </span>
    </footer>
  );
}

const TOAST_MS = 4000;
const TOAST_ERR_MS = 7000;

function Toasts({ store }: { store: Store }) {
  const toasts = store.state.value.toasts;
  useEffect(() => {
    const timers = toasts.map((t) => setTimeout(() => store.update(dismissToast, t.id), t.err ? TOAST_ERR_MS : TOAST_MS));
    return () => timers.forEach(clearTimeout);
  }, [toasts]);
  return (
    <div class="nk-toasts" aria-live="polite" data-testid="toasts">
      {toasts.map((t) => (
        <div key={t.id} class={`nk-toast${t.err ? ' is-err' : ''}`} onClick={() => store.update(dismissToast, t.id)}>
          {t.text}
        </div>
      ))}
    </div>
  );
}

export function Chrome(props: ChromeProps) {
  const w = getWindowWiring();
  const keys = props.keys ?? w?.keys;
  const ui = props.ui ?? w?.ui;
  if (!keys || !ui) return null;
  const op: OverlayProps = { store: props.store, ui, keys, ...(props.client ? { client: props.client } : {}) };
  const ov = ui.overlay.value;
  return (
    <>
      <TopBar store={props.store} ui={ui} />
      {ui.treeOpen.value && <TreeColumn store={props.store} ui={ui} keys={keys} />}
      <StatusBar store={props.store} ui={ui} />
      {ov?.kind === 'palette' && <Palette {...op} />}
      {ov?.kind === 'switcher' && <Switcher {...op} />}
      {ov?.kind === 'filter' && <FilterBar {...op} />}
      {ov?.kind === 'help' && <Help {...op} />}
      {ov?.kind === 'usage' && <UsageSheet {...op} />}
      <ConfirmDialog {...op} />
      <Toasts store={props.store} />
    </>
  );
}
