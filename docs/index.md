---
okf_version: "0.2"
type: Bundle Index
title: neko — documentação
description: Bundle OKF 0.2 do neko, orquestrador TUI das sessões do Claude Code.
resource: https://github.com/aqilputi/neko
tags: [neko, índice]
timestamp: 2026-10-15T17:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-15T17:00:00Z }
status: stable
---

# neko — documentação

Bundle no [Open Knowledge Format](https://okf.md/spec/) v0.2: cada arquivo é um conceito,
o frontmatter YAML pode ser consultado, e os links entre arquivos formam o grafo. Só `type` é obrigatório.
Também usamos os campos de confiança da 0.2 (`generated`, `status`, `stale_after`, `sources`).
O `index.md` e o `log.md` são arquivos reservados da spec.

## Conceitos

| Conceito | Tipo | O quê |
|---|---|---|
| [instalacao.md](instalacao.md) | Guide | Pré-requisitos, build, primeiro uso |
| [teclas.md](teclas.md) | Reference | Todas as teclas e o mouse |
| [configuracao.md](configuracao.md) | Reference | `~/.config/neko/config.toml` campo a campo |
| [arquitetura.md](arquitetura.md) | Architecture | Pacotes, fluxo de dados, laço de atualização |
| [fontes-de-dados.md](fontes-de-dados.md) | Reference | O que o neko lê de `~/.claude` e como calcula custo |
| [painel-embutido.md](painel-embutido.md) | Design Note | PTY + `x/vt` rodando `claude attach`; bugs contornados |
| [hooks.md](hooks.md) | Guide | Eventos em tempo real via `neko hook` |
| [jira.md](jira.md) | Guide | Integração Jira sem token, via `claude -p` + conector |
| [requisitos.md](requisitos.md) | Requirements | Mapa MUST/SHOULD/COULD e o estado de cada um |
| [log.md](log.md) | Log | Histórico (reservado OKF) |

## Grafo

[requisitos.md](requisitos.md) → implementados em [arquitetura.md](arquitetura.md), que lê
[fontes-de-dados.md](fontes-de-dados.md), embute o claude em [painel-embutido.md](painel-embutido.md)
e recebe eventos de [hooks.md](hooks.md). O usuário instala com [instalacao.md](instalacao.md),
ajusta em [configuracao.md](configuracao.md) e opera com [teclas.md](teclas.md). [jira.md](jira.md)
estende os todos. As mudanças ficam registradas em [log.md](log.md).
