---
type: Reference
title: Fontes de dados e cálculo de custo
description: Arquivos e comandos do Claude Code que o neko lê (somente leitura) e a fórmula de custo por request.
resource: https://github.com/aqilputi/neko/tree/main/internal/claude
tags: [dados, custo, claude-code, transcripts]
timestamp: 2026-10-15T17:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-15T17:00:00Z }
status: stable
stale_after: 2027-01-15
---

# Fontes de dados

Todas as fontes são **somente leitura**. Os formatos são internos ao Claude Code e podem mudar a qualquer
versão, daí o `stale_after`: depois dessa data, confira esta página de novo.

| fonte | o que dá |
|---|---|
| `claude agents --json --all` | sessões vivas (interativas e bg) e bg concluídas; id curto para `attach` |
| `~/.claude/sessions/<pid>.json` | status real da interativa (`busy·shell·idle·waiting`) e `waitingFor`. O `.key` ao lado **nunca** é lido |
| `~/.claude/jobs/<id>/state.json` | bg: `detail`, `needs`, `suggestedReply`, `block.questions[].options`, `children` (PRs/artifacts) |
| `~/.claude/projects/<cwd>/<sid>.jsonl` | custo (usage por `requestId`, com dedup), título (`ai-title`), últimas mensagens, tool pendente |
| `<sid>/subagents/agent-*.jsonl` + `.meta.json` | árvore de subagents: descrição, tipo, modelo, custo |
| `todo.py list --format json` | a lista de tarefas (a escrita é só pela CLI da skill) |
| `~/.local/state/neko/events.jsonl` | eventos dos [hooks](hooks.md) |

Estado próprio do neko: `~/.local/state/neko/` (eventos, arquivadas, overrides) e `~/.cache/neko/` (cache do
[Jira](jira.md)).

## Custo

Por request (dedup por `requestId`), somando os subagents:

```
custo = input·in + output·out + cache_5m·1,25·in + cache_1h·2·in + cache_read·cr
```

Os preços por modelo são configuráveis (ver [configuracao.md](configuracao.md)). O resultado fica ~5–15% abaixo
do `cost-state` oficial, porque algumas chamadas internas do Claude Code não vão para o transcript.
