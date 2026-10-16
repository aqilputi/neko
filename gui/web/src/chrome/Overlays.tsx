// Modal layers of the chrome: the ":" palette, the ctrl+k switcher, the
// "/" filter, help (?), usage (u) and the confirmation dialog. Each owns
// its keys while open (the global key controller stands aside).

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { NkClient } from '../client/client.ts';
import type { CommandInfo } from '../client/types.ts';
import type { KeyController } from '../keymap/controller.ts';
import { keyString } from '../keymap/keys.ts';
import type { Ui } from '../keymap/ui.ts';
import type { Store } from '../state/store.ts';
import { findLimit, usageToken } from './format.ts';
import {
  filterPalette,
  fuzzyFilter,
  paletteItems,
  switcherItems,
  typedItem,
  type PaletteItem,
} from './palette.ts';

export interface OverlayProps {
  store: Store;
  ui: Ui;
  keys: KeyController;
  client?: Pick<NkClient, 'commands'>;
}

function close(ui: Ui) {
  ui.overlay.value = null;
}

/** Up/down/enter/esc over a list, shared by the palette and the switcher. */
function listKeys(e: KeyboardEvent, n: number, cur: number, setCur: (i: number) => void, onEnter: () => void, onEsc: () => void) {
  const k = keyString(e);
  switch (k) {
    case 'down':
    case 'ctrl+n':
    case 'ctrl+j':
    case 'tab':
      setCur(n ? (cur + 1) % n : 0);
      break;
    case 'up':
    case 'ctrl+p':
    case 'ctrl+k':
    case 'shift+tab':
      setCur(n ? (cur - 1 + n) % n : 0);
      break;
    case 'enter':
      onEnter();
      break;
    case 'esc':
      onEsc();
      break;
    default:
      return;
  }
  e.preventDefault();
  e.stopPropagation();
}

function ItemList({ items, cur, onPick }: { items: PaletteItem[]; cur: number; onPick(i: number): void }) {
  const ref = useRef<HTMLUListElement>(null);
  useEffect(() => {
    ref.current?.children[cur]?.scrollIntoView({ block: 'nearest' });
  }, [cur, items]);
  return (
    <ul class="nk-pal-list" role="listbox" ref={ref}>
      {items.slice(0, 200).map((it, i) => (
        <li
          key={it.id + i}
          role="option"
          aria-selected={i === cur}
          class={`nk-pal-item${i === cur ? ' is-cur' : ''}${it.action.kind === 'none' ? ' is-usage' : ''}`}
          onMouseDown={(e) => {
            e.preventDefault();
            onPick(i);
          }}
        >
          <span class="nk-pal-title">{it.title}</span>
          {it.hint && <kbd class="nk-pal-hint">{it.hint}</kbd>}
        </li>
      ))}
      {items.length === 0 && <li class="nk-pal-empty">nada encontrado</li>}
    </ul>
  );
}

export function Palette({ store, ui, keys, client }: OverlayProps) {
  const ov = ui.overlay.value;
  const query = ov?.kind === 'palette' ? ov.query : '';
  const [cur, setCur] = useState(0);
  const [cmds, setCmds] = useState<CommandInfo[] | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const s = store.state.value;

  useEffect(() => {
    input.current?.focus();
    let live = true;
    // availability for the current selection (the bootstrap list has none)
    client
      ?.commands(keys.context())
      .then((c) => live && setCmds(c))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => setCur(0), [query]);

  const commands = cmds ?? s.commands;
  const items = useMemo(() => {
    const groups: Record<string, number> = {};
    for (const [tag, members] of Object.entries(s.snapshot?.groups ?? {})) groups[tag] = members.length;
    return paletteItems({
      commands,
      sessions: s.snapshot?.sessions ?? [],
      showAll: !!s.tree.state.show_all,
      themes: s.theme?.themes ?? [],
      activeTheme: s.theme?.name ?? '',
      pickedN: ui.picked.value.size,
      groups,
    });
  }, [commands, s.snapshot, s.theme, s.tree.state.show_all, ui.picked.value]);

  const cursorTags = (store.cursorRow.value?.groups ?? []).map((g) => g.tag);
  const typed = typedItem(query, {
    commands,
    pickedN: ui.picked.value.size,
    cursorTags,
    lua: !!s.hello?.capabilities.includes('lua'),
  });
  const list = filterPalette(items, query, typed);

  const run = (i: number) => {
    const it = list[i];
    if (it) void keys.runAction(it.action).then(() => input.current?.focus());
  };

  return (
    <div class="nk-overlay" onMouseDown={(e) => e.target === e.currentTarget && close(ui)}>
      <div class="nk-pal" role="dialog" aria-label="comandos" data-testid="palette">
        <div class="nk-pal-input">
          <span class="nk-pal-prompt">:</span>
          <input
            ref={input}
            value={query}
            spellcheck={false}
            autocomplete="off"
            placeholder="ação, sessão, tema · group <tag> · ungroup · notify #tag msg"
            onInput={(e) => {
              ui.overlay.value = { kind: 'palette', query: (e.target as HTMLInputElement).value };
            }}
            onKeyDown={(e) => listKeys(e, list.length, cur, setCur, () => run(cur), () => close(ui))}
          />
        </div>
        <ItemList items={list} cur={cur} onPick={run} />
      </div>
    </div>
  );
}

export function Switcher({ store, ui, keys }: OverlayProps) {
  const [q, setQ] = useState('');
  const [cur, setCur] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const all = switcherItems(store.state.value.snapshot?.sessions ?? []);
  const list = fuzzyFilter(all, q);
  const run = (i: number) => {
    const it = list[i];
    if (it) void keys.runAction(it.action);
  };
  return (
    <div class="nk-overlay" onMouseDown={(e) => e.target === e.currentTarget && close(ui)}>
      <div class="nk-pal" role="dialog" aria-label="chats" data-testid="switcher">
        <div class="nk-pal-input">
          <span class="nk-pal-prompt">⌕</span>
          <input
            ref={input}
            value={q}
            spellcheck={false}
            placeholder={`${all.length} chats rodando ou te esperando…`}
            onInput={(e) => {
              setQ((e.target as HTMLInputElement).value);
              setCur(0);
            }}
            onKeyDown={(e) => listKeys(e, list.length, cur, setCur, () => run(cur), () => close(ui))}
          />
        </div>
        <ItemList items={list} cur={cur} onPick={run} />
      </div>
    </div>
  );
}

export function FilterBar({ store, ui, keys }: OverlayProps) {
  const ov = ui.overlay.value;
  const value = ov?.kind === 'filter' ? ov.value : '';
  const input = useRef<HTMLInputElement>(null);
  const before = useRef(store.state.value.tree.state.filter ?? '');
  useEffect(() => input.current?.focus(), []);
  return (
    <div class="nk-filter" data-testid="filter">
      <span class="nk-pal-prompt">/</span>
      <input
        ref={input}
        value={value}
        spellcheck={false}
        placeholder="filtro fuzzy: label, contexto, cwd…"
        onInput={(e) => {
          const v = (e.target as HTMLInputElement).value;
          ui.overlay.value = { kind: 'filter', value: v };
          void keys.refresh({ filter: v });
        }}
        onKeyDown={(e) => {
          const k = keyString(e);
          if (k === 'enter') close(ui);
          else if (k === 'esc') {
            if (value !== before.current || value) void keys.refresh({ filter: '' });
            close(ui);
          } else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      />
    </div>
  );
}

function useCloseKeys(ui: Ui, closers: string[]) {
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      const k = keyString(e);
      if (k && closers.includes(k)) {
        e.preventDefault();
        e.stopPropagation();
        close(ui);
      }
    };
    window.addEventListener('keydown', on, true);
    return () => window.removeEventListener('keydown', on, true);
  }, []);
}

export function Help({ store, ui }: OverlayProps) {
  useCloseKeys(ui, ['esc', 'q', '?', 'enter']);
  const km = store.state.value.keymap;
  const modes = [...new Set(km.map((b) => b.mode))];
  return (
    <div class="nk-overlay" onMouseDown={(e) => e.target === e.currentTarget && close(ui)}>
      <div class="nk-sheet" role="dialog" aria-label="atalhos" data-testid="help">
        <h2>atalhos</h2>
        {modes.map((m) => (
          <section key={m}>
            <h3>{m === '*' ? 'em qualquer modo' : m}</h3>
            <dl class="nk-help">
              {km
                .filter((b) => b.mode === m)
                .map((b) => (
                  <div key={b.command} class="nk-help-row">
                    <dt>
                      <kbd>{b.help.key || (b.keys ?? []).join('/')}</kbd>
                    </dt>
                    <dd>{b.help.desc}</dd>
                  </div>
                ))}
            </dl>
          </section>
        ))}
        <section>
          <h3>janelas</h3>
          <dl class="nk-help">
            <div class="nk-help-row">
              <dt>
                <kbd>ctrl+]</kbd>
              </dt>
              <dd>sai do terminal (INSERT → NORMAL)</dd>
            </div>
            <div class="nk-help-row">
              <dt>
                <kbd>alt+setas</kbd>
              </dt>
              <dd>move a janela ativa (shift: mais longe)</dd>
            </div>
          </dl>
        </section>
      </div>
    </div>
  );
}

export function UsageSheet({ store, ui }: OverlayProps) {
  useCloseKeys(ui, ['esc', 'q', 'u', 'enter']);
  const u = store.state.value.snapshot?.usage;
  return (
    <div class="nk-overlay" onMouseDown={(e) => e.target === e.currentTarget && close(ui)}>
      <div class="nk-sheet" role="dialog" aria-label="uso" data-testid="usage">
        <h2>uso</h2>
        {u?.err && <p class="nk-err">{u.err}</p>}
        {(u?.limits ?? []).map((l) => (
          <div class="nk-usage-row" key={l.name}>
            <span class="nk-usage-name">{l.name}</span>
            <span class="nk-bar">
              <span class="nk-bar-fill" style={{ width: `${Math.min(l.pct, 100)}%`, background: `var(--nk-${usageToken(l.pct)})` }} />
            </span>
            <strong style={{ color: `var(--nk-${usageToken(l.pct)})` }}>{l.pct}%</strong>
            {l.resets && <span class="nk-muted">↻ {l.resets}</span>}
          </div>
        ))}
        {!u?.limits?.length && <p class="nk-muted">sem dados de uso</p>}
        {findLimit(u, 'session') === undefined && (
          <p class="nk-muted">hoje ≈ ${(store.state.value.snapshot?.today_cost ?? 0).toFixed(2)}</p>
        )}
      </div>
    </div>
  );
}

export function ConfirmDialog({ store, ui, keys }: OverlayProps) {
  const p = store.state.value.pending[0];
  useEffect(() => {
    if (!p) return;
    // modal: whatever was open steps aside so y/n reach the key controller
    ui.overlay.value = null;
    const a = document.activeElement;
    if (a instanceof HTMLElement && !a.closest('.xterm')) a.blur();
  }, [p?.token]);
  if (!p) return null;
  return (
    <div class="nk-overlay nk-overlay-confirm">
      <div class="nk-confirm" role="alertdialog" aria-label="confirmação" data-testid="confirm">
        <p class="nk-confirm-q">{p.question}</p>
        <div class="nk-confirm-actions">
          <button class="nk-btn nk-btn-yes" onClick={() => void keys.confirm(p.token, true)}>
            <kbd>y</kbd> sim
          </button>
          <button class="nk-btn nk-btn-no" onClick={() => void keys.confirm(p.token, false)}>
            <kbd>n</kbd> não
          </button>
        </div>
      </div>
    </div>
  );
}
