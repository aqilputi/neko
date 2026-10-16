---
type: Reference
title: Registro de comandos
description: Todo comando registrado em internal/app — nome, título, escopo, teclas e argumentos — e como rodar cada um pela paleta, por Lua, por RPC ou por nk run.
resource: https://github.com/aqilputi/nekuroddo/tree/main/internal/app
tags: [comandos, registro, paleta, rpc, lua, teclas]
timestamp: 2026-10-16T20:40:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T20:40:00Z }
status: stable
---

# Registro de comandos

Um **comando** (`core.Command`) é a unidade que o keymap, a paleta, o Lua, o RPC e (mais tarde) o GUI
compartilham: `Name`, `Title` (texto pt-BR da ajuda/paleta), `Hint`, `Keys` (teclas padrão; vazio = só paleta),
`Mode`, `Args`, `Scope` (`core` roda no núcleo, `client` é feito por cada cliente — navegação, abas, zoom), e
`Confirm`. `app.Registry` (`internal/app/registry.go`) é a única tabela: `Register` cadastra, `Run` executa
por nome (reconferindo o registro a cada chamada — um comando trocado em tempo real, como um `nk.command` do
Lua, vale na próxima chamada), `List(ctx)` devolve os disponíveis para uma seleção e `All()` devolve todos
(disponíveis ou não — é o que alimenta o keymap, que vale "não importa onde está o cursor agora"). Um comando
`Scope: client` não tem `Runner` (`run: nil`): existe só para a ajuda, a paleta e `GET /v1/commands`
mostrarem a tecla; rodá-lo no núcleo (RPC, `nk run`, `nk.run`) dá `ErrClientCommand`.

`App.RegisterBuiltins()` chama, nesta ordem, `RegisterSessionCommands`, `RegisterDispatch`, `RegisterPanes`,
`RegisterGroupCommands`, `RegisterTodoJira` e `RegisterKeyCommands` — uma função por arquivo "dono"
(`internal/app/sessions.go`, `dispatch.go`, `panes.go`, `groups.go`, `jira.go`, `commands.go`). As tabelas
abaixo seguem essa divisão; a coluna **args** lista `nome:tipo` (`required` com `*`), e **teclas** é o
`Keys` do comando no modo indicado (`normal.todos` é um sub-modo de `normal`: suas teclas têm prioridade na
aba Todos, e o resto cai em `normal` — ver [teclas.md](teclas.md)). Essa não é a lista **inteira** de um
processo real: `cmd/nekuroddo` registra à mão mais um, `lua.reload` (ver [lua.md](lua.md)), e um `init.lua`
pode registrar os seus próprios com `nk.command` (`Source: "lua:<arquivo>"`) — `GET /v1/commands`/`nk run`
sem argumento mostram o registro completo de um processo em particular.

## Sessões — `sessions.go`

| comando | título | escopo | modo | teclas | args | confirma |
|---|---|---|---|---|---|---|
| `session.archive` | arquiva/restaura (no contexto: todas) | core | normal | `X` | `context:string` | sim |
| `sessions.archive_stale` | arquivar todas as sessões paradas | core | normal | — | — | sim |
| `sessions.remove_ended` | limpar chats encerrados (`claude rm`) | core | normal | — | — | sim |
| `session.stop` | parar a sessão em background | core | normal | `x` | — | sim |
| `session.remove` | remover a sessão em background | core | normal | `dd` | — | sim |
| `session.rename` | renomear a sessão | core | normal | `n` | `name:string` | não |
| `session.move` | mover a sessão para outro contexto | core | normal | `m` | `context:string` | não |

## Disparo de agent — `dispatch.go`

| comando | título | escopo | modo | teclas | args | confirma |
|---|---|---|---|---|---|---|
| `session.new` | novo agent em background | core | normal | `c` | `dir:dir*`, `prompt:string*`, `perm:string`, `name:string` | sim\* |

\* só pergunta quando `perm = bypassPermissions` e esse **não** é o `permission_mode` configurado
(`App.DispatchNeedsConfirm`) — ver [a seção de confirmação](#confirmação-metadado-vs-comportamento) abaixo.

## Painéis — `panes.go`

| comando | título | escopo | modo | teclas | args | confirma |
|---|---|---|---|---|---|---|
| `session.open` | abre e foca | core | normal | `enter` | — | não |
| `session.preview` | abre sem focar | core | normal | `o` | — | não |
| `session.fullscreen` | tela cheia | core | normal | `O` | — | não |
| `pane.close` | fecha painel | client | normal | `w` | — | não |
| `pane.zoom` | zoom do painel | client | normal | `z` | — | não |

## Grupos — `groups.go`

| comando | título | escopo | modo | teclas | args | confirma |
|---|---|---|---|---|---|---|
| `group.add` | agrupa as sessões escolhidas | core | normal | — | `tag:tag*` | não |
| `group.remove` | tira as sessões escolhidas do grupo | core | normal | — | `tag:tag` | não |
| `group.notify` | avisa o grupo | core | normal | — | `tag:tag`, `text:string*` | não |
| `group.dissolve` | desfaz o grupo | core | normal | — | `tag:tag*` | sim |

Sem `tag` em `group.notify`, usa o grupo da sessão sob o cursor. Detalhes de uso em [grupos.md](grupos.md).

## Todos e Jira — `jira.go`

| comando | título | escopo | modo | teclas | args | confirma |
|---|---|---|---|---|---|---|
| `todo.start` | começa a tarefa | core | `normal.todos` | `s` | — | não |
| `todo.done` | marca a tarefa como done | core | `normal.todos` | `D` | — | sim |
| `jira.toggle` | todo↔jira: issue vira tarefa, tarefa vira issue | core | `normal.todos` | `J` | `target:string` | sim\* |
| `jira.fetch` | busca as issues no Jira (via claude, ~$0,10) | core | `normal.todos` | `F` | — | não |
| `jira.open_url` | abre a issue no browser | core | `normal.todos` | `o` | — | não |

\* só no sentido tarefa → issue (`AskCreateIssue`); issue → tarefa (`IssueToTodo`) roda direto, sem perguntar,
apesar do mesmo `Confirm: true` nos metadados — é outro caso da nota abaixo.

## Cliente: navegação, abas e vista — `commands.go` (`clientCommands`)

Todos `scope: client`, `mode: normal` (exceto `switcher.open`, que também vale em INSERT — `mode: *`); nenhum
tem `Runner` nem `confirm`. São resolvidos por cada cliente (a TUI hoje; o GUI, na fase 3), não pelo núcleo.

| comando | título | teclas |
|---|---|---|
| `nav.down` / `nav.up` | desce / sobe | `j`/`down` · `k`/`up` |
| `nav.top` / `nav.bottom` | topo / fim | `gg` · `G` |
| `nav.half_down` / `nav.half_up` | meia página ↓/↑ | `ctrl+d` · `ctrl+u` |
| `nav.page_down` / `nav.page_up` | página ↓/↑ | `ctrl+f`/`pgdown` · `ctrl+b`/`pgup` |
| `nav.collapse` / `nav.expand` | fecha/abre nó | `h`/`left` · `l`/`right` |
| `nav.pick` | seleciona (p/ `:group`); no contexto: alterna | `space` |
| `nav.next_attention` / `nav.prev_attention` | próxima/anterior te esperando | `a` · `A` |
| `session.insert` | INSERT no painel | `i` |
| `tab.sessions`/`tab.inbox`/`tab.todos`/`tab.archive` | as quatro abas | `1` `2` `3` `4` |
| `tab.next` / `tab.prev` | próxima/anterior aba | `tab` · `shift+tab` |
| `view.filter` | filtro fuzzy | `/` |
| `view.show_all` | mostra paradas/arquivadas/subagents concluídos | `H` |
| `view.sort` | ordem: atividade/custo/nome | `S` |
| `view.theme` | tema | `T` |
| `switcher.open` | ir p/ chat rodando/esperando | `ctrl+k` |
| `palette.open` | comandos | `:` `ctrl+p` |
| `usage.show` | uso (`/usage`) | `u` |
| `help.open` | todos os comandos | `ctrl+/` `?` `ctrl+_` |
| `app.quit` | sai | `q` `ctrl+c` |
| `inbox.summon` | inbox: vai para quem te espera | — (só paleta/Lua/RPC) |

## Mais três comandos `core` — `commands.go` (`RegisterKeyCommands`)

| comando | título | escopo | modo | teclas | args | confirma |
|---|---|---|---|---|---|---|
| `todo.dispatch` | dispara um agent para a tarefa ou issue | core | `normal.todos` | `enter` | `dir:dir`, `perm:string` | sim\* |
| `session.reply` | resposta rápida | core | normal | `r` | `text:string*` | não |
| `sessions.refresh` | atualiza e reordena | core | normal | `R` | — | não |

\* mesma regra condicional de `session.new` (usa o mesmo `App.Dispatch`).

## Confirmação: metadado vs. comportamento

`Confirm: true` em `core.Command` é só um **aviso** ("este comando pode pedir uma confirmação") para quem
monta a paleta, a ajuda ou um cliente remoto — não é o que efetivamente dispara o fluxo de `pending`. Quem
decide pedir confirmação é o próprio runner, chamando `App.Ask(pergunta, func)` (ver
[protocolo.md](protocolo.md#post-v1confirmtoken)). Na prática:

- **Sempre pergunta:** `session.archive`, `sessions.archive_stale`, `sessions.remove_ended`, `session.stop`,
  `session.remove`, `group.dissolve`, `todo.done`, `jira.toggle` (só tarefa→issue).
- **Pergunta só às vezes:** `session.new` e `todo.dispatch` só pedem confirmação se o modo de permissão for
  `bypassPermissions` **e** isso não for o `permission_mode` já configurado; com qualquer outro `perm`, o
  agent dispara direto.
- **Metadado sem efeito hoje:** `jira.toggle` no sentido issue→tarefa roda sem perguntar nada, apesar de
  `Confirm: true` valer para o comando inteiro (o `core.Command` não distingue "ramo" de execução).

Um cliente que quer saber de antemão se *vai* perguntar não tem como, pela metadata sozinha, nesses dois
últimos casos — só rodando o comando e olhando se a resposta trouxe `pending`.

## Como rodar um comando

| onde | como |
|---|---|
| Paleta (`:`) | `:<nome> <arg>…` — as palavras preenchem os argumentos na ordem declarada, a última leva o resto; ver [teclas.md](teclas.md#paleta-) |
| Lua | `nk.run(nome, { arg = valor, ... }, ctx)` — roda qualquer comando do registro, builtin ou Lua; `ctx` opcional (default: o do chamador) — ver [lua.md](lua.md) |
| RPC | `POST /v1/commands/<nome>` com `{"args": {...}, "context": {...}}` — ver [protocolo.md](protocolo.md#comandos) |
| `nk run` | `nk run <nome> [arg=valor…] [--session <id>] [--yes] [--addr <endereço>]` — sem `<nome>`, lista `GET /v1/commands` (marca `-` = indisponível para a seleção); um `pending` vira um `[y/N]` no terminal, a menos que `--yes` |

Um comando `scope: client` (a tabela de navegação acima) não roda em nenhuma dessas quatro formas no núcleo —
só a TUI (e, mais tarde, o GUI) sabe executá-lo sobre o seu próprio estado de tela.
