---
type: Architecture
title: Arquitetura
description: Pacotes Go do neko, o fluxo dos dados do ~/.claude até a tela e o laço de atualização.
resource: https://github.com/aqilputi/neko
tags: [arquitetura, go, bubbletea, charm]
timestamp: 2026-10-15T17:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-15T17:00:00Z }
status: stable
---

# Arquitetura

## Princípio

**Embrulhar o `claude` instalado, sem reimplementá-lo.** A lista de sessões vem de `claude agents --json`,
as sessões novas saem de `claude --bg`, e o painel da direita é um terminal virtual rodando `claude attach <id>`
de verdade. Assim, toda atualização do Claude Code chega ao neko sem mudança aqui. O neko não chama a API
da Anthropic e não exporta nada.

## Pacotes

| pacote | responsabilidade |
|---|---|
| `cmd/neko` | entrada: `neko` (TUI) e `neko hook` (registrador de eventos, ver [hooks.md](hooks.md)) |
| `cmd/vtprobe` | diagnóstico do painel embutido |
| `internal/claude` | fala com o CLI (`agents`, `--bg`, `stop`, `rm`) e lê transcripts, estado e uso (ver [fontes-de-dados.md](fontes-de-dados.md)) |
| `internal/store` | junta CLI e transcripts num `Snapshot` imutável: sessões, subagents, atenção, custo, arquivadas, overrides |
| `internal/events` | grava e lê `~/.local/state/neko/events.jsonl` |
| `internal/term` | PTY + `charmbracelet/x/vt`, filtro de OSC e codificação de teclas (ver [painel-embutido.md](painel-embutido.md)) |
| `internal/ui` | modelo Bubble Tea v2: árvore, inbox, todos, diálogos, paleta, mouse, render |
| `internal/theme` | 5 temas e a escolha automática claro/escuro |
| `internal/cat` | o gatinho `ᓚᘏᗢ` no canto inferior direito |
| `internal/config` | carga do TOML e padrões (ver [configuracao.md](configuracao.md)) |
| `internal/todo` | lê e escreve a lista **somente** via CLI da skill `todo` (que cuida de lock e escrita atômica) |
| `internal/jira` | `claude -p` headless com o conector Atlassian (ver [jira.md](jira.md)) |

## Fluxo

```
claude agents --json --all ─┐
~/.claude/sessions/*.json  ─┤
~/.claude/jobs/*/state.json ─┼─► store.Refresh ─► Snapshot ─► ui.Model ─► uv.ScreenBuffer
~/.claude/projects/**.jsonl ─┤   (goroutine própria)                  ▲
events.jsonl (hooks) ────────┘                                         │
                                         term.Pane (PTY: claude attach) ┘ Emulator.Draw célula a célula
```

- **Refresh incremental:** os transcripts são lidos a partir do último offset, e uma linha ainda incompleta
  espera o próximo ciclo. Só entram arquivos modificados nos últimos `show_history_days`, com no máximo
  64 MB por arquivo.
- **Concorrência:** cada `Store` pertence a uma goroutine de cada vez (o comando de refresh). A UI só enxerga
  o `Snapshot` que ele devolve.
- **Relógio único de animação:** 150 ms quando algo anima e 1 s com o gato dormindo.
- **Atenção** (menor = mais urgente): bloqueada (pergunta/permissão) > voltou (terminou e você não viu) >
  ociosa no prompt.

## Influências

Do estudo do [crush](https://github.com/charmbracelet/crush) vieram: tela composta num `uv.ScreenBuffer`,
`key.Binding` como fonte única de teclas, ajuda e paleta, diálogos em overlay, paleta fuzzy, toasts com `seq`,
OSC 7501 e título da janela, notificação só quando a janela está sem foco, debounce do resize do PTY e
banner de saída.

## Riscos

- Os formatos de `~/.claude` são internos e mudam entre versões. Por isso o parsing fica isolado em
  `internal/claude`, com testes.
- `claude --bg --resume <sid>` numa sessão que ainda está rodando cria uma **cópia**. O neko só faz isso com
  sessões do histórico.
- Transcripts podem conter dados sensíveis. O neko só os lê, localmente.
