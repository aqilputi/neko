---
type: Architecture
title: Arquitetura
description: Pacotes Go do nekuroddo, o fluxo dos dados do ~/.claude até a tela, os grupos de agents e o laço de atualização.
resource: https://github.com/aqilputi/nekuroddo
tags: [arquitetura, go, bubbletea, charm, grupos]
timestamp: 2026-10-16T23:00:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T19:00:00Z }
status: stable
---

# Arquitetura

## Princípio

**Embrulhar o `claude` instalado, sem reimplementá-lo.** A lista de sessões vem de `claude agents --json`,
as sessões novas saem de `claude --bg`, e o painel da direita é um terminal virtual rodando `claude attach <id>`
de verdade. Assim, toda atualização do Claude Code chega ao nekuroddo sem mudança aqui. O nekuroddo não chama a
API da Anthropic e não exporta nada.

O mesmo princípio vale para os [grupos de agents](grupos.md): a conversa entre sessões usa o `SendMessage`
nativo do Claude Code; o nekuroddo só apresenta os pares e mostra as mensagens.

## Pacotes

| pacote | responsabilidade |
|---|---|
| `cmd/nekuroddo` | entrada e raiz de composição (monta `app` + `wire` + Lua): `nk` (TUI; com `--listen`, também o servidor RPC no mesmo core), `nk serve` (core sem TUI, servido por RPC), `nk run` (roda um comando num core servido), `nk hook` (registrador de eventos) e `nk hook-context` (grupos); ver [hooks.md](hooks.md) |
| `cmd/vtprobe` | diagnóstico do painel embutido |
| `internal/claude` | fala com o CLI (`agents`, `--bg`, `stop`, `rm`, `-p /usage`) e lê transcripts, estado e uso (ver [fontes-de-dados.md](fontes-de-dados.md)) |
| `internal/store` | junta CLI e transcripts num `Snapshot` imutável: sessões, subagents, atenção, custo, arquivadas, overrides |
| `internal/events` | grava e lê `~/.local/state/nekuroddo/events.jsonl`; reconhece `<cross-session-message>` (`PeerMessage`) |
| `internal/groups` | `groups.json`, a fila de notas do `:notify` e o corpo do `nk hook-context` (ver [grupos.md](grupos.md)) |
| `internal/term` | `Process` (PTY e fan-out dos bytes, sem Bubble Tea) + `Screen` (`charmbracelet/x/vt`, filtro de OSC, teclas); ver [painel-embutido.md](painel-embutido.md) |
| `internal/core` | domínio puro, sem I/O: árvore e inbox (`BuildTree`), helpers, comando/evento/confirmação como dados |
| `internal/app` | o core: casos de uso por fatia, registro de comandos, barramento de eventos, confirmação por token e o estado de domínio (snapshot, grupos, ✉, painéis, avisos, `/usage`, todos, Jira); fala com o mundo só por portas |
| `internal/wire` | raiz de composição: adaptadores finos que ligam `claude`, `store`, `term`, `groups`, `events`, `jira`, `todo`, `notify-send` e `xdg-open` às portas de `app` |
| `internal/tui` | a TUI Bubble Tea v2, um tradutor: teclas e mensagens viram chamadas a `app`; desenha árvore, inbox, todos, diálogos, paleta, painéis e o gato |
| `internal/archtest` | testes que falham se a regra de dependência (abaixo) for violada |
| `internal/theme` | 5 temas e a escolha automática claro/escuro |
| `internal/cat` | o gatinho `ᓚᘏᗢ` no canto inferior direito |
| `internal/config` | carga do TOML, padrões (ver [configuracao.md](configuracao.md)) e a migração dos diretórios do `neko` |
| `internal/paths` | nomes dos diretórios de config, estado e cache (XDG); sem imports internos, para o hook ficar barato |
| `internal/todo` | lê e escreve a lista **somente** via CLI da skill `todo` (que cuida de lock e escrita atômica) |
| `internal/jira` | `claude -p` headless com o conector Atlassian (ver [jira.md](jira.md)) |

## Fluxo

```
claude agents --json --all ─┐
~/.claude/sessions/*.json  ─┤
~/.claude/jobs/*/state.json ─┼─► store.Refresh ─► app.RefreshNotices ─► tui.Model ─► uv.ScreenBuffer
~/.claude/projects/**.jsonl ─┤   (goroutine própria: Snapshot,  ▲         ▲
events.jsonl (nk hook) ──────┘    transições, nomes dos grupos)  │         │
groups.json ─────────────────────────────────────► app (grupos) ─┘         │
                                     term.Pane (PTY: claude attach) ────────┘ Emulator.Draw célula a célula
```

- **Refresh incremental:** os transcripts são lidos a partir do último offset, e uma linha ainda incompleta
  espera o próximo ciclo. Só entram arquivos modificados nos últimos `show_history_days`, com no máximo
  64 MB por arquivo.
- **Concorrência:** cada `Store` pertence a uma goroutine de cada vez (o comando de refresh). O refresh passa
  por `app.RefreshNotices`, que guarda o `Snapshot` como o do core, publica `snapshot.changed`, compara os
  estados (avisos de "voltou") e atualiza os nomes nos grupos; a UI desenha o `Snapshot` que ele devolve.
  As sessões são compartilhadas com o core, então arquivar ou renomear aparece na hora, antes do próximo
  refresh.
- **Eventos:** o `events.Tail` (porta `EventLog`, lida por `app.PollHookEvents`) traz o que foi acrescentado
  ao `events.jsonl` a cada 400 ms. `app.OnHookEvents` decide: um evento comum antecipa o refresh; um evento
  `peer` vai para `app.OnPeer` (✉ dos dois lados, `peer.message`); a UI só mia e toca o sino.
- **Relógio único de animação:** 150 ms quando algo anima (spinner, `◆` piscando, `✉` recente) e 1 s com o gato
  dormindo.
- **Atenção** (menor = mais urgente): bloqueada (pergunta/permissão) > voltou (terminou e você não viu) >
  ociosa no prompt.

## Grupos de agents

```
tui (:group / :ungroup) ─► app ─► groups.json ◄── app.SyncGroupNames (a cada refresh: renomeações do Claude)
tui (:notify) ─► app ────────► mail/<sid>.jsonl
                                │
nk hook-context ◄───────────────┘  (SessionStart · UserPromptSubmit · PostToolUse)
   └─► additionalContext na sessão: pares + combinado + notas
sessão A ──SendMessage──► sessão B ──UserPromptSubmit──► nk hook ──► events.jsonl (type=peer) ──► app.OnPeer ──► tui (miau)
```

- `groups.json` é escrito de forma atômica (arquivo temporário + `rename`), porque o hook pode lê-lo a qualquer
  momento, e é relido a cada refresh, então dois nekuroddo abertos enxergam as mudanças um do outro.
- O hook guarda um hash do contexto por sessão (`ctx/<sid>`) para só repetir o texto quando o grupo muda.
- As notas são reivindicadas com `rename` antes da leitura, para que uma nota escrita no meio não se perca.
- Os dois hooks nunca falham para o Claude Code: em qualquer erro, saem com 0 sem imprimir nada.

## Diretórios e migração

`internal/paths` concentra os nomes: `~/.config/nekuroddo`, `~/.local/state/nekuroddo` e
`~/.cache/nekuroddo` (ou os equivalentes XDG). `config.MigrateLegacy` roda no início de toda invocação,
hooks incluídos, e renomeia os diretórios antigos do `neko` quando o novo ainda não existe. No caso comum, custa
três `os.Lstat`.

## Influências

Do estudo do [crush](https://github.com/charmbracelet/crush) vieram: tela composta num `uv.ScreenBuffer`,
`key.Binding` como fonte única de teclas, ajuda e paleta, diálogos em overlay, paleta fuzzy, toasts com `seq`,
OSC 7501 e título da janela, notificação só quando a janela está sem foco, debounce do resize do PTY e
banner de saída. A seleção com `space` segue o [yazi](https://github.com/sxyazi/yazi).

## Arquitetura alvo (fase 1 feita; onda 3 a seguir)

O plano em [plano-arquitetura.md](plano-arquitetura.md) separa o domínio do resto: regras puras em
`internal/core`, sem I/O; casos de uso com efeitos em `internal/app`, que fala com o mundo só por **portas**
implementadas pelos pacotes acima. Um registro de comandos e um barramento de eventos em `app` são a face
comum da TUI, de um GUI futuro (three.js + xterm.js num shell Wails v3, com gatinhos 3D low-poly por cima da
calçada) e de scripts em Lua (`gopher-lua`). Um servidor local (`internal/rpc`, HTTP+SSE+WS sobre Unix socket)
fica **opcional**: o `nk` continua um processo só, sem socket.

### O que já existe (fase 1)

- **`internal/core`**: árvore e inbox (`BuildTree`, ordem congelada, `SortInbox`), helpers (`Truncate`,
  `ResolveOption`, `WindowPct`, `GroupColorIndex`, `ActivePane`…) e os tipos `Command`, `Event`,
  `PendingConfirmation`, `ExecRequest`.
- **`internal/app`**: `App` com as portas (`ports.go`), o snapshot do core (`Refresh`/`RefreshNotices`,
  `snapshot.changed`), o barramento (`Subscribe`/`Publish`), o registro de comandos (`Registry`, preenchido
  por `RegisterBuiltins`) e confirmações por token (`Ask`/`Confirm`/`Cancel`). Todo o estado de domínio é
  campo do `App`, uma fatia por arquivo:

  | fatia | `internal/app` | `internal/tui` (só traduz e desenha) | comandos | eventos |
  |---|---|---|---|---|
  | sessões | `sessions.go` | `sessions.go` | `session.archive/stop/remove/rename/move`, `sessions.archive_stale/remove_ended` | `snapshot.changed` |
  | disparo | `dispatch.go`, `confirm.go` | `input.go` | `session.new` | `dispatched`, `confirm.*` |
  | painéis | `panes.go` (`PaneManager`) | `pane.go` | `session.open/preview/fullscreen`, `pane.close/zoom` | `pane.*` |
  | grupos | `groups.go` | `groups.go` | `group.add/remove/notify/dissolve` | `group.changed`, `peer.message`, `mail.cue` |
  | todos e Jira | `todo.go`, `jira.go` | `todo_jira.go` | `todo.start/done`, `jira.toggle/fetch/open_url` | `todos.changed`, `jira.changed` |
  | avisos | `watch.go` | `notify.go` | — | `session.state`, `session.notification`, `notice`, `usage.updated` |

- **`internal/wire`**: `wire.Default(cfg)` liga os adaptadores reais às portas; quem o usa é só
  `cmd/nekuroddo`, que monta o `App` (e chama `RegisterBuiltins`, carrega o Lua) e o entrega à TUI
  (`tui.New(app, cfg)`) e ao servidor. `internal/app/apptest` tem fakes de todas as portas.
- **`internal/term`**: `Process` (PTY e fan-out de bytes, sem Bubble Tea) e `Screen` (emulador e teclas, da
  TUI).
- **`internal/tui`** é um tradutor: teclas e mensagens viram chamadas a `app`, e a tela lê o estado por
  acessores de `app` (`Groups`, `Mail`, `Messages`, `Usage`, `LastEvent`, `HooksSeen`, `Panes()`, `Todos`,
  `JiraCache`…). Fica nela só o que é do cliente: cursor, dobras, filtro, ordem congelada, seleção com
  `space`, diálogos, o painel desenhado e o gato.
- **`internal/archtest`**: `core` e `app` não alcançam Bubble Tea, PTY, `os/exec`, rede, `term` nem a UI;
  `internal/tui` só importa `app`, `core`, `theme`, `cat`, `term`, `config` e os **tipos** de `store`,
  `claude`, `events`, `groups`, `jira` e `todo` (funções só `jira.AgentPrompt` e `todo.Prompt`); nunca
  `wire`, `rpc` nem `lua`; `os/exec` só em `tui/pane.go`, para o `tea.ExecProcess` da tela cheia que
  `app.Fullscreen` descreve.

### Próximos passos (onda 3 em diante)

- **WP-3.1:** o registro de comandos substitui o `switch` de teclas: cada tecla e item da paleta resolve
  para um nome de comando (`keyMap` derivado de `core.Keymap`, `paletteItems()` lendo `Registry.List()`), e
  a TUI passa a seguir o barramento (um `tea.Cmd` sobre `app.Subscribe`).
- **WP-3.2:** `internal/rpc` (HTTP+SSE+WS sobre Unix socket) expõe o mesmo registro e o barramento.
- **WP-3.3:** `internal/lua` (`gopher-lua`, API `nk.*`) registra comandos e assina eventos.
- **WP-3.4 e WP-4.1 (feitos):** `nk serve`, `nk --listen` e `nk run`; `internal/ui` virou `internal/tui`
  (`tui.New(app, cfg)`, com `:lua` na paleta). Depois: o GUI (fases 2 a 4).

Árvore de pacotes (resumida; "futuro" ainda não existe):

```
cmd/nekuroddo/      nk: TUI, nk --listen, nk serve, nk run <cmd>, nk hook, nk hook-context
cmd/nekuroddo-gui/  futuro — nkg: shell Wails v3 com o frontend em gui/web/
internal/core/      domínio puro: tipos, árvore, inbox, comando/evento como dados; sem I/O
internal/app/       casos de uso + registro de comandos + pubsub + confirmações; fala só com portas
internal/wire/      raiz de composição: liga claude/store/term/... às portas de internal/app
internal/tui/       a TUI: teclas → chamadas a app, estado de app → tela
internal/rpc/       servidor opcional (nk serve, nk --listen): HTTP+SSE+WS sobre Unix socket
internal/lua/       runtime gopher-lua, API nk.* (comandos e eventos para a TUI e o GUI)
internal/{claude,store,term,groups,events,jira,todo,config,paths,theme,cat}   adaptadores por papel
internal/archtest/  testes que falham se a regra de dependência abaixo for violada
```

Regra de dependência: `internal/core` nunca importa Bubble Tea, PTY ou rede, só tipos de dados dos pacotes
acima; `internal/app` só fala com o mundo por portas; os "shells" (`tui`, `rpc`, `lua`, `guihost`) não
chamam `claude`/`store`/`groups`/`jira`/`todo` diretamente — só por `internal/app`.

```
     cmd/nekuroddo   cmd/nekuroddo-gui
           │                │
  internal/tui  internal/rpc  internal/lua  internal/guihost     ← shells (teclas/HTTP/Lua → comandos)
           │                │              │
           └───────┬────────┴──────────────┘
                    ▼
            internal/app ──► portas ◄── internal/wire ──► claude, store, term, groups, events, jira, todo
                    │
                    ▼
            internal/core   (só stdlib + tipos de dados; nunca chama funções dos pacotes acima)
```

Decisões completas, o protocolo do core, a cena do GUI, a API Lua e os pacotes de trabalho por fase (com dono
de arquivos para agents trabalharem em paralelo) estão em [plano-arquitetura.md](plano-arquitetura.md).

## Riscos

- Os formatos de `~/.claude` e o `<cross-session-message>` são internos e mudam entre versões. Por isso o
  parsing fica isolado em `internal/claude` e `internal/events`, com testes.
- `claude --bg --resume <sid>` numa sessão que ainda está rodando cria uma **cópia**. O nekuroddo só faz isso
  com sessões do histórico.
- Transcripts podem conter dados sensíveis. O nekuroddo só os lê, localmente.
