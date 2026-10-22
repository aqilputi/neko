---
okf_version: "0.2"
type: Bundle Index
title: nekuroddo — documentação
description: Bundle OKF 0.2 do nekuroddo, orquestrador TUI das sessões do Claude Code.
resource: https://github.com/aqilputi/nekuroddo
tags: [nekuroddo, índice]
timestamp: 2026-10-16T23:50:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T23:50:00Z }
status: stable
---

# nekuroddo — documentação

Bundle no [Open Knowledge Format](https://okf.md/spec/) v0.2: cada arquivo é um conceito,
o frontmatter YAML pode ser consultado, e os links entre arquivos formam o grafo. Só `type` é obrigatório.
Também usamos os campos de confiança da 0.2 (`generated`, `status`, `stale_after`, `sources`).
O `index.md` e o `log.md` são arquivos reservados da spec.

## Conceitos

| Conceito | Tipo | O quê |
|---|---|---|
| [instalacao.md](instalacao.md) | Guide | Pré-requisitos, `go install`/`make install`, o atalho `nk`, migração do `neko` |
| [teclas.md](teclas.md) | Reference | Todas as teclas, a paleta e o mouse |
| [comandos.md](comandos.md) | Reference | O registro de comandos: nome, escopo, teclas, argumentos e como rodar cada um (paleta, Lua, RPC, `nk run`) |
| [configuracao.md](configuracao.md) | Reference | `~/.config/nekuroddo/config.toml` campo a campo |
| [grupos.md](grupos.md) | Guide | Grupos de agents: `:group`, `nk hook-context`, `SendMessage` e o ✉ |
| [hooks.md](hooks.md) | Guide | `nk hook` (eventos em tempo real) e `nk hook-context` (grupos) |
| [arquitetura.md](arquitetura.md) | Architecture | Pacotes, fluxo de dados, grupos, laço de atualização |
| [protocolo.md](protocolo.md) | Reference | O protocolo v1 do `internal/rpc`: rotas HTTP, SSE, WebSocket de painel, erros e `nk serve`/`--listen`/`run` |
| [lua.md](lua.md) | Reference | A API `nk.*` do `internal/lua`, eventos, orçamentos, sandbox e o que ainda falta (capacidades de UI) |
| [plano-arquitetura.md](plano-arquitetura.md) | Plan | Arquitetura alvo (core headless, GUI 3D de gatinhos low-poly, Lua) e os pacotes de trabalho por fase — planejado, fase 1 em andamento |
| [plano-mundo.md](plano-mundo.md) | Plan | O mundo (`nkw`): WM 3D aconchegante em primeira pessoa no estilo Animal Crossing, com os agentes como gatos — modos NAV/ANDAR/FOCO/CONVERSA, Smithay + GLES, fases e orçamentos |
| [gui.md](gui.md) | Guide | Guia do GUI nativo (`nkg`, Rust + Bevy): o escritório, o zoom no terminal, a inbox, teclado e desempenho |
| [gui-web.md](gui-web.md) | Guide | O GUI anterior, em navegador (legado) |
| [fontes-de-dados.md](fontes-de-dados.md) | Reference | O que o nekuroddo lê de `~/.claude` e como calcula custo |
| [painel-embutido.md](painel-embutido.md) | Design Note | PTY + `x/vt` rodando `claude attach`; bugs contornados |
| [jira.md](jira.md) | Guide | Integração Jira sem token, via `claude -p` + conector |
| [requisitos.md](requisitos.md) | Requirements | Mapa MUST/SHOULD/COULD e o estado de cada um |
| [releases/v0.1.0.md](releases/v0.1.0.md) | Release Notes | Notas da v0.1.0 (primeira versão marcada) |
| [log.md](log.md) | Log | Histórico (reservado OKF) |

Fora do bundle: o [README](../README.md), o [CHANGELOG](../CHANGELOG.md) e o
[guia de contribuição](../CONTRIBUTING.md).

## Grafo

[requisitos.md](requisitos.md) → implementados em [arquitetura.md](arquitetura.md), que lê
[fontes-de-dados.md](fontes-de-dados.md), embute o claude em [painel-embutido.md](painel-embutido.md)
e recebe eventos de [hooks.md](hooks.md). Os [grupos](grupos.md) usam os dois hooks para que sessões
parecidas se avisem. O usuário instala com [instalacao.md](instalacao.md), ajusta em
[configuracao.md](configuracao.md) e opera com [teclas.md](teclas.md), que por sua vez aponta para
[comandos.md](comandos.md) — o registro que [teclas.md](teclas.md), o [protocolo.md](protocolo.md) e o
[lua.md](lua.md) compartilham. [jira.md](jira.md) estende os todos.
[plano-arquitetura.md](plano-arquitetura.md) traça para onde [arquitetura.md](arquitetura.md) está migrando
(core headless, GUI e Lua), por fases e pacotes de trabalho ainda em andamento; o [protocolo.md](protocolo.md)
(`internal/rpc`, `nk serve`/`--listen`/`run`) e o [lua.md](lua.md) (`internal/lua`, `nk.*`) documentam duas
dessas peças já em código. O [gui.md](gui.md) documenta a Fase 3 (o GUI `nkg`), que fala o mesmo
[protocolo.md](protocolo.md) e o mesmo [lua.md](lua.md) da TUI. As mudanças ficam registradas em
[log.md](log.md), e cada versão tem notas em [releases/](releases/v0.1.0.md).
