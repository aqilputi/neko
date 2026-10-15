---
type: Guide
title: Instalação e primeiro uso
description: Pré-requisitos, build com Go e como rodar o neko pela primeira vez.
resource: https://github.com/aqilputi/neko
tags: [instalação, build, go]
timestamp: 2026-10-15T17:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-15T17:00:00Z }
status: stable
---

# Instalação e primeiro uso

## Pré-requisitos

- **Linux** (PTY, `notify-send` opcional para notificações). Em macOS deve funcionar, mas nunca foi testado.
- **Go 1.27.2+** (o `go.mod` pede essa versão).
- **Claude Code** instalado e no `PATH`, numa versão com `claude agents --json`, `claude --bg` e
  `claude attach`. O neko embrulha esse binário e não traz cópia própria. Para usar outro binário, defina
  `NEKO_CLAUDE=/caminho/do/claude`.
- Opcional: a skill `todo` (`~/.claude/skills/todo/scripts/todo.py`) para a aba Todos, e o conector
  Atlassian no claude.ai para a [integração Jira](jira.md).

## Build

```sh
go install github.com/aqilputi/neko/cmd/neko@latest
# ou, a partir do clone:
git clone https://github.com/aqilputi/neko && cd neko
go build -o ~/.local/bin/neko ./cmd/neko
go test ./...
```

## Primeiro uso

```sh
neko
```

Sem configuração, o neko usa os padrões: contexto inferido pelo `cwd`, histórico de 2 dias e tema que segue
o terminal. Para ajustar, copie o [`config.example.toml`](../config.example.toml) para `~/.config/neko/config.toml`
(veja [configuracao.md](configuracao.md)).

Para receber eventos em menos de 1 s em vez de esperar o polling de 2 s, instale os [hooks](hooks.md).

O neko lê `CLAUDE_CONFIG_DIR` quando definido; sem ele, usa `~/.claude`.

## Ferramenta de diagnóstico

```sh
go run ./cmd/vtprobe -out /tmp/probe -- claude --model haiku
```

Roda um programa dentro do painel embutido de verdade e, a cada passo, despeja tela, cursor, título e
clipboard. Detalhes em [painel-embutido.md](painel-embutido.md).
