// Minimal shell of the GUI (WP-6.1): "conectando…" until the first snapshot
// and tree arrive, then the tree as plain text. The chrome, scene, inbox
// and windows WPs replace the body; the store/sync wiring stays.

import type { TreeRow } from './client/types.ts';
import type { NkState, Store } from './state/index.ts';
import { stateToken } from './state/theme.ts';

const STATUS_TEXT: Record<NkState['status'], string> = {
  connecting: 'conectando…',
  open: 'conectado',
  reconnecting: 'reconectando…',
  closed: 'desconectado',
  error: 'erro de conexão',
};

function money(n: number | undefined): string {
  return n ? `$${n.toFixed(2)}` : '';
}

/** One tree row as a line of text (the TUI's tree, without the art). */
export function rowText(r: TreeRow): string {
  switch (r.kind) {
    case 'ctx':
      return `${r.collapsed ? '▸' : '▾'} ${r.ctx}  ${money(r.ctx_cost)} (${r.ctx_n ?? 0})`;
    case 'sess':
      return `  ${r.label ?? r.short_id ?? r.session_id}  ${r.state ?? ''}  ${money(r.cost)}`.trimEnd();
    default:
      return `    └ ${r.label ?? r.sub?.id ?? ''}  ${r.state ?? ''}`.trimEnd();
  }
}

function Tree({ rows }: { rows: readonly TreeRow[] }) {
  return (
    <ul class="tree" data-testid="tree">
      {rows.map((r) => (
        <li key={r.key} class={`row row-${r.kind}`} data-key={r.key} style={{ color: `var(--nk-${stateToken(r.state)})` }}>
          {rowText(r)}
        </li>
      ))}
    </ul>
  );
}

export function App({ store }: { store: Store }) {
  const s = store.state.value;
  if (!s.ready) {
    return (
      <main class="boot">
        <p data-testid="status">conectando…</p>
        {s.error && <p class="error">{s.error}</p>}
      </main>
    );
  }
  return (
    <main>
      <header class="top">
        <span class="brand">ᓚᘏᗢ nekuroddo</span>
        <span data-testid="status" class={`status status-${s.status}`}>
          {STATUS_TEXT[s.status]}
        </span>
      </header>
      <Tree rows={s.tree.rows} />
      {s.error && <p class="error">{s.error}</p>}
    </main>
  );
}
