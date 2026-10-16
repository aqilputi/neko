---
type: Guide
title: Hooks do Claude Code
description: Como instalar `nk hook` (eventos em tempo real, assíncrono) e `nk hook-context` (grupos de agents, síncrono) no Claude Code.
resource: https://github.com/aqilputi/nekuroddo/tree/main/internal/events
tags: [hooks, eventos, grupos, claude-code]
timestamp: 2026-10-16T12:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T12:00:00Z }
status: stable
sources:
  - id: claude-code-hooks
    resource: https://docs.claude.com/en/docs/claude-code/hooks
    title: Claude Code — Hooks reference
---

# Hooks do Claude Code

O nekuroddo usa dois hooks, ambos opcionais:

| comando | modo | eventos | para quê |
|---|---|---|---|
| `nk hook` | assíncrono | `Notification`, `Stop`, `SubagentStop`, `UserPromptSubmit`, `SessionStart`, `SessionEnd` | eventos em tempo real e detecção das mensagens entre sessões |
| `nk hook-context` | **síncrono** | `SessionStart`, `UserPromptSubmit`, `PostToolUse` | contar a uma sessão agrupada quem são os pares e entregar notas do `:notify` ([grupos.md](grupos.md)) |

Um subcomando desconhecido sai com código 1 (nunca 2, que bloquearia a sessão) e não abre a TUI.

## `nk hook` — eventos em tempo real

Sem hooks, o nekuroddo descobre as mudanças pelo polling (`refresh_seconds`, 2 s por padrão). Com o hook, cada
evento vira uma linha em `~/.local/state/nekuroddo/events.jsonl`, que o nekuroddo lê a cada 400 ms. O gato
mia com a mensagem do Claude ("Claude needs your permission…"), e o `↯` na barra de status mostra que os
eventos estão chegando.

`nk hook` lê o payload do stdin, grava uma linha compacta (`at`, `event`, `session_id`, `cwd`, `type`,
`message`, `agent_id`, `from`) e sai. Ele **não imprime nada e sempre sai com código 0**, então nunca bloqueia
nem altera uma sessão. Leva ~3 ms. Cada linha é um único `write` com `O_APPEND`, então sessões em paralelo não
embaralham as linhas; o arquivo roda para `events.jsonl.1` ao passar de 4 MB.

Contam como "precisa de você" as notificações `permission_prompt`, `idle_prompt`, `agent_needs_input`,
`elicitation_dialog` e `worker_permission_prompt`.

Quando o prompt de um `UserPromptSubmit` é uma `<cross-session-message>` (um `SendMessage` de outra sessão), o
evento sai com `type: "peer"`, o remetente em `from` e a primeira linha do corpo em `message`. É assim que o
nekuroddo mostra o `✉` dos [grupos](grupos.md).

## `nk hook-context` — grupos de agents

Roda **síncrono** porque o que ele imprime vira contexto da sessão: um JSON com
`hookSpecificOutput.additionalContext`. Ele só imprime algo quando a sessão está num grupo e o grupo mudou
desde a última vez (ou no `SessionStart`), quando a sessão acabou de sair de um grupo, ou quando há notas do
`:notify` na fila. Nos outros casos, não imprime nada. Erros nunca chegam à sessão: sem saída, código 0.
O funcionamento completo está em [grupos.md](grupos.md).

## Instalação

Em `~/.claude/settings.json` (ou via `/hooks`). O caminho supõe `make install`; ajuste se o `nk` estiver em
outro lugar.

```json
{
  "hooks": {
    "Notification": [ { "hooks": [ { "type": "command", "command": "~/.local/bin/nk hook", "timeout": 5, "async": true } ] } ],
    "Stop":         [ { "hooks": [ { "type": "command", "command": "~/.local/bin/nk hook", "timeout": 5, "async": true } ] } ],
    "SubagentStop": [ { "hooks": [ { "type": "command", "command": "~/.local/bin/nk hook", "timeout": 5, "async": true } ] } ],
    "SessionEnd":   [ { "hooks": [ { "type": "command", "command": "~/.local/bin/nk hook", "timeout": 5, "async": true } ] } ],
    "SessionStart": [ { "hooks": [
      { "type": "command", "command": "~/.local/bin/nk hook", "timeout": 5, "async": true },
      { "type": "command", "command": "~/.local/bin/nk hook-context", "timeout": 5 }
    ] } ],
    "UserPromptSubmit": [ { "hooks": [
      { "type": "command", "command": "~/.local/bin/nk hook", "timeout": 5, "async": true },
      { "type": "command", "command": "~/.local/bin/nk hook-context", "timeout": 5 }
    ] } ],
    "PostToolUse": [ { "hooks": [
      { "type": "command", "command": "~/.local/bin/nk hook-context", "timeout": 5 }
    ] } ]
  }
}
```

**Não** marque o `nk hook-context` como `async`: a saída de um hook assíncrono não entra no contexto da sessão,
e o grupo deixaria de funcionar.

Se você não usa grupos, basta o `nk hook`. Para desligar tudo, remova as entradas; o polling continua
funcionando como rede de segurança.

### Vindo do `neko`

Troque `neko hook` por `nk hook` em todas as entradas e acrescente as do `nk hook-context`. Os eventos antigos
em `~/.local/state/neko/` são movidos para `~/.local/state/nekuroddo/` na primeira execução (ver
[instalacao.md](instalacao.md#vindo-do-neko)).

Ver também: [arquitetura.md](arquitetura.md).
