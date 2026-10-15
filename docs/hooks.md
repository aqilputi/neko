---
type: Guide
title: Eventos em tempo real (hooks)
description: Como instalar o `neko hook` no Claude Code para o neko reagir em menos de 1 s.
resource: https://github.com/aqilputi/neko/tree/main/internal/events
tags: [hooks, eventos, claude-code]
timestamp: 2026-10-15T17:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-15T17:00:00Z }
status: stable
sources:
  - id: claude-code-hooks
    resource: https://docs.claude.com/en/docs/claude-code/hooks
    title: Claude Code — Hooks reference
---

# Eventos em tempo real

Sem hooks, o neko descobre as mudanças pelo polling (`refresh_seconds`, 2 s por padrão). Com hooks, cada
evento do Claude Code vira uma linha em `~/.local/state/neko/events.jsonl`, que o neko lê a cada 400 ms.
O gato mia com a mensagem do Claude ("Claude needs your permission…"), e o `↯` na barra de status mostra
que os eventos estão chegando.

`neko hook` lê o payload do stdin, grava uma linha compacta (`at`, `event`, `session_id`, `cwd`, `type`,
`message`, `agent_id`) e sai. Ele **não imprime nada e sempre sai com código 0**, então nunca bloqueia nem
altera uma sessão. Leva ~3 ms.

## Instalação

Em `~/.claude/settings.json` (ou via `/hooks`), adicione a mesma entrada em `Notification`, `Stop`,
`SubagentStop`, `UserPromptSubmit`, `SessionStart` e `SessionEnd`:

```json
{
  "hooks": {
    "Notification": [
      { "hooks": [ { "type": "command", "command": "~/.local/bin/neko hook", "timeout": 5, "async": true } ] }
    ]
  }
}
```

Para desligar, remova essas entradas. O polling continua funcionando como rede de segurança.

Contam como "precisa de você" as notificações `permission_prompt`, `idle_prompt`, `agent_needs_input`,
`elicitation_dialog` e `worker_permission_prompt`.

Ver também: [arquitetura.md](arquitetura.md).
