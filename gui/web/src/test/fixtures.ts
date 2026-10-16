// Trimmed answers of `nk serve --demo` (captured from the real server), for
// the state tests.

import type { Hello, Snapshot, Theme, Tree } from '../client/types.ts';
import { FALLBACK_THEME } from '../state/theme.ts';

export const GW = 'd3e0a001-0000-4000-8000-000000000001';
export const MIG = 'd3e0a002-0000-4000-8000-000000000002';
export const IDX = 'd3e0a006-0000-4000-8000-000000000006';

export const hello: Hello = { app: 'nekuroddo', version: 'dev', protocol: 1, capabilities: ['panes', 'lua'], pid: 1 };

export const theme: Theme = { ...FALLBACK_THEME, themes: ['catppuccin-mocha', 'dracula'] };

export function snapshot(): Snapshot {
  return {
    sessions: [
      {
        session_id: GW,
        short_id: 'dmgatewy',
        label: 'api-gateway',
        cwd: '/demo/visio/gateway',
        context: 'visio',
        kind: 'background',
        state: 'busy',
        attention: 0,
        cost: 2.1,
      },
      {
        session_id: MIG,
        short_id: 'dmmigrat',
        label: 'migração do banco',
        cwd: '/demo/visio/db',
        context: 'visio',
        kind: 'background',
        state: 'blocked',
        attention: 1,
        cost: 3.4,
        questions: [{ question: 'Como tratar a coluna legada user_ref?', options: [{ label: 'manter', description: '' }] }],
      },
    ],
    inbox: [MIG],
    groups: { 'deploy-rc': [{ session_id: GW, name: 'api-gateway' }] },
    mail: {},
    usage: { limits: [{ name: 'Current session', pct: 60 }] },
    todos: { groups: null },
    jira: { fetched_at: '2026-10-16T18:46:49Z', issues: [], cost_usd: 0 },
    pending: [],
    at: '2026-10-16T18:46:49Z',
    today_cost: 3.27,
  };
}

export function tree(): Tree {
  return {
    rows: [
      { key: 'ctx:visio', kind: 'ctx', ctx: 'visio', depth: 0, color: 0, ctx_cost: 5.5, ctx_n: 2, attention: 0 },
      {
        key: `sess:${GW}`,
        kind: 'sess',
        ctx: 'visio',
        depth: 1,
        color: 0,
        session_id: GW,
        short_id: 'dmgatewy',
        label: 'api-gateway',
        session_kind: 'background',
        state: 'busy',
        attention: 0,
        cost: 2.1,
        groups: [{ tag: 'deploy-rc', color: 18 }],
      },
      {
        key: `sess:${MIG}`,
        kind: 'sess',
        ctx: 'visio',
        depth: 1,
        color: 0,
        session_id: MIG,
        label: 'migração do banco',
        state: 'blocked',
        attention: 1,
        options: 3,
      },
      {
        key: 'sub:agent-idx-a',
        kind: 'sub',
        ctx: 'oss',
        depth: 2,
        color: 2,
        session_id: IDX,
        label: 'mapear os módulos',
        state: 'busy',
        attention: 0,
        sub: { id: 'agent-idx-a', description: 'mapear os módulos' },
      },
    ],
    state: { tab: 'sessions', sort: 'atividade', cursor: 0, cursor_key: 'ctx:visio', inbox_cursor: 0 },
    inbox: [MIG],
    hidden: 1,
    archived: 0,
  };
}
