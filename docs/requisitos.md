---
type: Requirements
title: Mapa de requisitos
description: Requisitos MUST/SHOULD/COULD levantados do uso real e o estado de cada um.
resource: https://github.com/aqilputi/nekuroddo
tags: [requisitos, roadmap]
timestamp: 2026-10-16T12:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T12:00:00Z }
status: stable
---

# Mapa de requisitos

Levantado a partir de 30 dias de uso real (até 15/10/2026): 82 sessões, 1–6 simultâneas e 156 subagentes
(19% do custo). A espera por você teve mediana de 2,8 min e p90 de 54 min. Os bloqueios eram quase sempre por
pergunta ou decisão, raramente por permissão, e todas as sessões foram em pt-BR. O objetivo é ter uma
**visão macro** e **responder rápido** quem está esperando.

| | requisito | estado |
|---|---|---|
| MUST | lista unificada (interativas + bg + histórico de N dias) por contexto | ✅ |
| MUST | inbox ordenado (bloqueado > voltou > no prompt) com `needs`, perguntas e opções, resposta sugerida | ✅ |
| MUST | resposta rápida para bg (attach no PTY + paste + enter) | ✅ |
| MUST | árvore sessão → subagents com modelo e custo | ✅ |
| MUST | barra de custo por sessão e por contexto, gasto de hoje, preços configuráveis | ✅ |
| MUST | contextos automáticos por cwd | ✅ |
| MUST | refresh incremental (tail por offset, linhas parciais seguras) | ✅ |
| MUST | vim, temas (5), gatinho, notificação (sino + `notify-send`) | ✅ |
| MUST | Claude Code embutido (painel PTY e tela cheia) — [painel-embutido.md](painel-embutido.md) | ✅ |
| MUST | sessões paradas (> `stale_days`) fora do inbox e ocultas por padrão; arquivar à mão; ordenar | ✅ |
| — | todos: ver, start/done, disparar agent no repo certo | ✅ |
| — | Jira: ver issues, issue → todo, todo → issue, agent por issue — [jira.md](jira.md) | ✅ (criar issue não testado de ponta a ponta) |
| SHOULD | eventos via hooks com latência < 1 s — [hooks.md](hooks.md) | ✅ |
| SHOULD | responder sessões **interativas** abertas em outro terminal | ❌ sem API pública (socket privado); alternativas: abrir pelo nekuroddo (`c`) ou kitty remote control |
| SHOULD | mensagens enfileiradas (`queue-operation`), barra de contexto (tokens/1M), `rate-limited` com countdown | ⏳ |
| SHOULD | busca full-text (`history.jsonl`), visão semana por contexto | ⏳ |
| SHOULD | barra de custo empilhada (cache read / write / output) | ⏳ |
| COULD | override manual de contexto, renomear sessão, timeline/replay, modo SDK host (stream-json) | ⏳ |
| COULD | índice persistido em `~/.cache/nekuroddo` para não reler centenas de MB na partida | ⏳ |
| COULD | temas do usuário (`~/.config/nekuroddo/themes/*.toml`) | ⏳ |

Histórico das decisões em [log.md](log.md).
