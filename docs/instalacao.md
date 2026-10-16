---
type: Guide
title: Instalação e primeiro uso
description: Pré-requisitos, build com Go ou make, o atalho nk, a migração de quem usava o neko e o primeiro uso.
resource: https://github.com/aqilputi/nekuroddo
tags: [instalação, build, go, make, migração]
timestamp: 2026-10-16T12:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T12:00:00Z }
status: stable
---

# Instalação e primeiro uso

## Pré-requisitos

- **Linux** (PTY; `notify-send` é opcional, para notificações). Em macOS deve funcionar, mas nunca foi testado.
- **Go 1.27.2+** (o `go.mod` pede essa versão).
- **Claude Code 2.1+** instalado e no `PATH`, com `claude agents --json`, `claude --bg` e `claude attach`. O
  nekuroddo embrulha esse binário e não traz cópia própria. Para usar outro binário, defina
  `NEKO_CLAUDE=/caminho/do/claude` (o prefixo `NEKO_` das variáveis de ambiente não mudou com o novo nome).
- Opcional: a skill `todo` (`~/.claude/skills/todo/scripts/todo.py`) para a aba Todos, e o conector
  Atlassian no claude.ai para a [integração Jira](jira.md).

## Instalar

O binário se chama `nekuroddo`. O comando do dia a dia é `nk`, um link simbólico para ele.

**Com `go install`:**

```sh
go install github.com/aqilputi/nekuroddo/cmd/nekuroddo@latest
ln -sf "$(go env GOPATH)/bin/nekuroddo" ~/.local/bin/nk
```

**A partir do clone:**

```sh
git clone https://github.com/aqilputi/nekuroddo && cd nekuroddo
make install                    # ~/.local/bin/nekuroddo + ~/.local/bin/nk
make install PREFIX=/usr/local  # ou em outro prefixo
```

Confira se `~/.local/bin` está no `PATH`.

| comando | o quê |
|---|---|
| `nk` | a TUI |
| `nk hook` | hook assíncrono que registra os eventos do Claude Code ([hooks.md](hooks.md)) |
| `nk hook-context` | hook síncrono dos [grupos de agents](grupos.md) |

## Primeiro uso

```sh
nk
```

Sem configuração, o nekuroddo usa os padrões: contexto inferido pelo `cwd`, histórico de 2 dias e tema que
segue o terminal. Para ajustar, copie o [`config.example.toml`](../config.example.toml) para
`~/.config/nekuroddo/config.toml` (veja [configuracao.md](configuracao.md)).

Para receber eventos em menos de 1 s em vez de esperar o polling de 2 s, e para usar grupos de agents,
instale os [hooks](hooks.md).

## Onde ficam os dados

| caminho | o quê |
|---|---|
| `~/.config/nekuroddo/config.toml` | a sua configuração |
| `~/.local/state/nekuroddo/` | eventos dos hooks, arquivadas, overrides de nome e contexto, `groups.json`, notas do `:notify` |
| `~/.cache/nekuroddo/` | cache do [Jira](jira.md) |

Os caminhos seguem `XDG_CONFIG_HOME`, `XDG_STATE_HOME` e `XDG_CACHE_HOME` quando definidos. O nekuroddo lê
`CLAUDE_CONFIG_DIR` quando definido; sem ele, usa `~/.claude`.

## Vindo do `neko`

O projeto se chamava `neko` até a v0.1.0. Para atualizar:

1. Instale o `nekuroddo` e o atalho `nk` como acima. O binário antigo (`~/.local/bin/neko`) pode ser apagado.
2. **Os dados migram sozinhos.** Na primeira execução (da TUI ou de qualquer hook), `~/.config/neko`,
   `~/.local/state/neko` e `~/.cache/neko` são renomeados para os equivalentes `nekuroddo`. A migração só
   acontece quando o diretório novo ainda não existe, então nunca sobrescreve nada; se falhar, o nekuroddo
   segue com os padrões e você pode mover à mão.
3. Em `~/.claude/settings.json`, troque `neko hook` por `nk hook` e acrescente o `nk hook-context`
   ([hooks.md](hooks.md)).
4. Num clone antigo, aponte o remote para o repositório novo:
   `git remote set-url origin https://github.com/aqilputi/nekuroddo`.

## Ferramenta de diagnóstico

```sh
go run ./cmd/vtprobe -out /tmp/probe -- claude --model haiku
```

Roda um programa dentro do painel embutido de verdade e, a cada passo, despeja tela, cursor, título e
clipboard. Detalhes em [painel-embutido.md](painel-embutido.md).
