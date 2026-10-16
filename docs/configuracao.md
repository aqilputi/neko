---
type: Reference
title: Configuração
description: Campos do ~/.config/nekuroddo/config.toml — contextos por regex, preços, orçamento, todos e Jira.
resource: https://github.com/aqilputi/nekuroddo/blob/main/config.example.toml
tags: [config, toml]
timestamp: 2026-10-16T12:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T12:00:00Z }
status: stable
---

# Configuração — `~/.config/nekuroddo/config.toml`

Todos os campos são opcionais, e o que faltar usa o padrão de `internal/config/config.go`. O exemplo comentado
é o [`config.example.toml`](../config.example.toml).

## Geral

| campo | padrão | o quê |
|---|---|---|
| `stale_days` | 3 | sessões paradas há mais tempo que isso saem do inbox e ficam ocultas (`H` mostra) |
| `sort` | `atividade` | `atividade` \| `custo` \| `nome` |
| `permission_mode` | `""` | modo inicial ao disparar: `""` (o do Claude), `acceptEdits`, `auto`, `plan`, `bypassPermissions` |
| `theme` | `""` | vazio segue o terminal (escuro → catppuccin-mocha, claro → rose-pine-dawn); também `tokyo-night`, `gruvbox`, `dracula` |
| `bar_max_usd` | 20 | custo que enche a barrinha de uma sessão |
| `daily_budget_usd` | 0 | mostra o medidor "hoje" no topo (0 esconde) |
| `refresh_seconds` | 2 | intervalo do polling (rede de segurança dos [hooks](hooks.md)) |
| `show_history_days` | 2 | também lista sessões fechadas dos últimos N dias |

## Contextos — `[[context]]`

Cada regra tem `match` (regex sobre o `cwd`, com `$HOME` trocado por `~`) e `name`, que aceita `$1`, `$2`…
Vence a primeira regra que casar. **Se você definir alguma regra, as padrão deixam de valer**, então copie as
que quiser manter.

```toml
[[context]]
match = '^~/work/([^/]+)'
name = "trabalho · $1"
```

## Preços — `[[price]]`

Valores em USD por milhão de tokens. As suas linhas entram antes das padrão, e a primeira que casar com o nome
do modelo vence. A fórmula de custo está em [fontes-de-dados.md](fontes-de-dados.md).

```toml
[[price]]
match = "opus-5-5"
input = 4
output = 20
cache_write = 5      # TTL de 5 min; o de 1 h custa 2× input
cache_read = 0.2
```

## Todos — `[todo_repos]` e `repo_roots`

Mapeia o `@contexto` de uma tarefa da skill `todo` para o diretório onde o agent da tarefa vai começar. Um
`@contexto` sem mapeamento é procurado por nome dentro de `repo_roots`.

```toml
repo_roots = ["~/work", "~/dev", "~"]
[todo_repos]
"@meu-repo" = "~/dev/meu-repo"
```

## Jira — `[jira]`

| campo | o quê |
|---|---|
| `enabled` | liga a integração |
| `site` | `acme.atlassian.net` |
| `cloud_id` | UUID do site (sem ele, usa o `site`) |
| `account_id` | assignee das issues criadas |
| `projects`, `default_project`, `default_type` | projetos visíveis e os padrões ao criar issue |
| `jql` | padrão: `assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC` |
| `model`, `max_budget_usd` | modelo e teto de custo do `claude -p` (padrão `haiku`, $0,30) |
| `cache_minutes` | validade do cache em `~/.cache/nekuroddo/jira.json` (padrão 30) |
| `agent_dir` | onde começam os agents disparados para uma issue |
| `approve_hint` | comando mostrado quando um hook bloqueia a escrita |

Detalhes em [jira.md](jira.md).
