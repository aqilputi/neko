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
`RegisterGroupCommands`, `RegisterTodoJira`, `RegisterKeyCommands` e `RegisterWorldCommands` — uma função por
arquivo "dono" (`internal/app/sessions.go`, `dispatch.go`, `panes.go`, `groups.go`, `jira.go`, `commands.go`,
`world.go`). As tabelas
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

## Mundo — `world.go`

Os comandos do mundo (o compositor `nkw`, [plano-mundo.md](plano-mundo.md) §3.12 e §7; guia de uso em
[mundo.md](mundo.md)): todos `scope: client`, sem
`Runner`, nos modos do mundo (`core.WorldModes`). O núcleo só os lista — em `GET /v1/commands` para a paleta
do `nkw` e em `GET /v1/keymap` para as teclas dele; quem executa é o `nkw`. A TUI e o GUI nunca os mostram
(`core.IsWorldMode`: nem na paleta, nem na ajuda, nem na barra de status). Teclas e modos em
[teclas.md](teclas.md#mundo-nkw).

Um comando com teclas em dois modos (`world.hint` no NAV e no ANDAR, `world.back`, as `world.line_*` na linha e
na paleta) é **um** comando no registro (o primeiro modo, com as teclas dele) e uma ligação a mais por modo no
keymap (`App.Keymap()` junta as de `world.go` enquanto o comando estiver registrado). `Registry.SetKeys` muda
só a ligação principal; a do outro modo fica.

| modo | teclas | comando | título |
|---|---|---|---|
| `normal.world` | `h` `left` · `l` `right` | `world.left` · `world.right` | estação anterior · próxima estação |
| `normal.world` | `j` `down` · `k` `up` | `world.down` · `world.up` | tela de baixo · tela de cima |
| `normal.world` | `J` `world+shift+j` · `K` `world+shift+k` | `world.room_next` · `world.room_prev` | próxima sala · sala anterior |
| `normal.world` | `1`…`9` | `world.room_1` … `world.room_9` | sala N |
| `normal.world` | `gg` · `G` | `world.first` · `world.last` | primeira mesa · última mesa |
| `normal.world` | `enter` `i` | `world.focus` | FOCO na tela (ou no terminal do gato) |
| `normal.world` | `t` | `world.talk` (`cat:string`, opcional) | conversa com o gato |
| `normal.world` | `w` `world+w` | `world.walk` | ANDAR (e volta) |
| `normal.world` | `f` `world+f` · `F` | `world.hint` · `world.hint_focus` | SALTO · SALTO direto para o FOCO |
| `normal.world` | `o` `world+o` | `world.overview` | mapa |
| `normal.world` | `b` | `world.library` | biblioteca |
| `normal.world` | `H` · `L` | `world.move_left` · `world.move_right` | tela para a mesa à esquerda/direita |
| `normal.world` | `,` · `.` | `world.consume` · `world.expel` | junta na pilha à esquerda · tira para uma mesa só dela |
| `normal.world`, `world.walk` | `-` · `=` | `world.narrower` · `world.wider` | mesa mais estreita · mais larga (no ANDAR: o tamanho da tela na mão) |
| `normal.world` | `p` | `world.pin` | prende a tela no ar |
| `normal.world` | `M` | `world.send_room` (`room:room*`) | manda a tela para outra sala |
| `normal.world` | `T` `world+enter` | `world.terminal` | terminal novo |
| `normal.world` | `Q` `world+q` | `world.close` | fecha a janela |
| `normal.world` | `shift+space` `world+space` | `world.summon` | chama o gato mais urgente |
| `normal.world` | `:` `ctrl+p` `world+p` | `world.palette` | COMANDOS |
| `normal.world` | — | `world.fullscreen` · `world.maximize` | tela cheia (TV) · maximiza (largura 1) |
| `normal.world` | — | `world.room_new` (`name:string*`) · `world.goto_room` (`room:room*`) · `world.goto_object` (`name:string*`) | sala nova · ir para a sala · ir até o objeto |
| `normal.world` | — | `world.open` (`path:string*`) · `world.vault_lock` | abre um arquivo no mundo · tranca o cofre |
| `normal.world`, `world.walk` · `world` | `tab` · `world+n` | `world.phone` | telefone |
| `normal.world` | `?` · `z` | `world.help` · `world.hud` | ajuda · esconde/mostra o HUD |
| `normal.world` | — | `world.tour` | tour da recepcionista |
| `normal.world` | — | `world.walk_set` (`key:string*`, `value:string*`) | ajusta o ANDAR (não salva) |
| `normal.world` | — | `world.launch_isolated` (`id:string*`) · `world.xwayland` (`on:string*`) | abre o app com perfil isolado · liga/desliga o X11 (xwayland-satellite) |
| `normal.world` | — | `world.goto_cat` (`cat:string*`) · `world.focus_cat` (`cat:string*`) | ir até o gato · FOCO no terminal do gato |
| `world` | `world+h` `world+l` `world+k` `world+j` | `world.focus_left` / `_right` / `_up` / `_down` | FOCO na vizinha |
| `world` | `world+d` | `world.launcher` | apps |
| `world` | `world+shift+e` | `world.quit` (confirma) | sair do mundo (confirma) |
| `world` | `XF86AudioPlay` · `XF86AudioNext` · `XF86AudioPrev` | `media.play_pause` · `media.next` · `media.prev` | tocar/pausar · faixa |
| `world` | `XF86AudioRaiseVolume` · `XF86AudioLowerVolume` · `XF86AudioMute` | `volume.up` · `volume.down` · `volume.mute` | volume |
| `world` | — | `brightness.up` · `brightness.down` | brilho |
| `world.walk` | `w` `s` `a` `d` (segurar) | `world.forward` / `back` / `strafe_left` / `strafe_right` | andar |
| `world.walk` | `q` `left` · `e` `right` | `world.turn_left` · `world.turn_right` | girar |
| `world.walk` | `up` · `down` | `world.look_up` · `world.look_down` | olhar |
| `world.walk` | `c` | `world.recenter` | horizonte |
| `world.walk` | `enter` `space` | `world.interact` | usar o que está na mira |
| `world.walk` | `g` | `world.grab` | pegar/soltar a tela |
| `world.walk` | `[` · `]` | `world.nearer` · `world.farther` | tela na mão: mais perto / mais longe |
| `world.walk` | `j` · `k` | `world.adjust_down` · `world.adjust_up` | ajustar o objeto na mira |
| `world.walk` | `h` · `l` | `world.prev` · `world.next` | anterior/próximo no objeto na mira |
| `world.walk` | `f` | `world.hint` | SALTO |
| `world.walk` | `esc` | `world.walk_exit` | volta ao NAV |
| `world.focus` | `ctrl+]` | `world.back` | sai do FOCO (`keys.focus_escape`) |
| `world.talk` | `space` `enter` | `world.talk_next` | avança |
| `world.talk` | `i` `r` | `world.talk_reply` | responder (linha) |
| `world.talk` | `f` | `world.talk_sit` | FOCO no terminal do gato |
| `world.talk` | `esc` `q` | `world.talk_end` | encerra |
| `world.talk` | `j` · `k` · `1`…`9` | `world.choice_down` · `world.choice_up` · `world.choice_1` … `_9` | escolha |
| `world.hint` | `esc` · `backspace` | `world.hint_cancel` · `world.hint_backspace` | cancela · apaga |
| `world.palette` | `esc` · `enter` · `tab` | `world.palette_cancel` · `_run` · `_complete` | paleta |
| `world.palette` | `up` `ctrl+p` · `down` `ctrl+n` | `world.palette_prev` · `world.palette_next` | paleta |
| `world.line`, `world.palette` | `ctrl+a` · `ctrl+e` · `ctrl+w` · `ctrl+u` | `world.line_home` · `_end` · `_delete_word` · `_clear` | edição |
| `world.line` | `esc` · `enter` · `shift+enter` | `world.line_cancel` · `world.line_send` · `world.line_newline` | linha |
| `world.library` | `h` `left` · `l` `right` | `world.book_prev` · `world.book_next` | livro anterior · próximo livro |
| `world.library` | `j` `down` · `k` `up` | `world.shelf_down` · `world.shelf_up` | prateleira de baixo · prateleira de cima |
| `world.library` | `enter` · `backspace` | `world.book_open` · `world.shelf_back` | abre o livro ou entra na pasta · sobe um nível |
| `world.library` | `/` | `world.library_search` (`q:string*`) | busca na estante |
| `world.library` | `e` · `p` · `y` | `world.book_edit` · `world.book_preview` · `world.book_copy_path` | abre no editor · prévia no atril · copia o caminho |
| `world.library` | `esc` | `world.library_leave` | sai da estante |
| `world.phone` | `h` `left` · `l` `right` | `world.phone_left` · `world.phone_right` | item à esquerda · item à direita |
| `world.phone` | `j` `down` · `k` `up` | `world.phone_down` · `world.phone_up` | item de baixo · item de cima |
| `world.phone` | `enter` `space` · `backspace` | `world.phone_open` · `world.phone_back` | abre · volta |
| `world.phone` | `esc` `tab` · `/` | `world.phone_close` · `world.phone_search` | fecha · busca |
| `world.phone` | `1`…`9` | `world.phone_app_1` … `world.phone_app_9` | app N |

`world.phone` é o **telefone** ([mundo.md](mundo.md#o-telefone)): o `nkw` o abre por cima dos COMANDOS e
seleciona esse modo enquanto ele está aberto. Ele **não tem fallback** — `core.ParentMode("world.phone") == ""`,
a outra exceção à regra do ponto —; o `nkw` resolve `world` antes dele (os acordes continuam valendo) e trata
as teclas imprimíveis que o telefone não liga como texto (a linha de busca, ou a tecla de um item). `world.phone`
tem três ligações: `tab` no NAV e no ANDAR (um comando, duas ligações) e `world+n` em `world`, que vale em todo
modo, inclusive no FOCO. `world.help` abre o telefone no app Ajuda; `world.talk` sem argumento conversa com o
gato da estação e, com `cat`, com aquele gato onde ele estiver (o telefone usa assim). `world.walk_set` muda uma
opção do `[walk]` só até o mundo fechar (`turn`, `mouse_look`, `comfort_vignette`, `speed`, `run_mul`,
`snap_deg`, `eye_height`); `world.xwayland` recebe `on: "true"|"false"`.

`world.library` é a estante da biblioteca (você está diante de uma prateleira, navegando pelos livros): é um
sub-modo do **NAV**, não de `world` — `core.ParentMode("world.library") == "normal.world"`, a única exceção à
regra do ponto —, então o que a estante não liga cai no NAV (`t`, `o`, `b`…) e dele em `normal`.
`world.open` é o caminho da paleta e do Lua para abrir um arquivo pelo mundo (recusa o que fica no cofre);
`world.vault_lock` tranca o cofre na hora.

Alguns nomes que o `nkw` usa por dentro **não** são comandos do core e nunca aparecem em `GET /v1/commands`:
`world.place_next`, `world.copy`, `world.band`, `world.goto_screen:<id>`, `world.launch:<id>`, `world.tour_done`,
`phone.sub`, `phone.app` (as ações do telefone) e `world.palette_run` com o argumento `name` (abre a paleta já no
argumento de um comando; o `world.palette_run` sem argumento é o `enter` da paleta, registrado). Nem as teclas Lua
`lua.key:<modo>:<tecla>` (ver [lua.md](lua.md#teclado)), que rodam por `POST /v1/commands/{name}` mas não são
comandos registrados.

O tipo de argumento `room` (`core.ArgRoom`) é novo: o `nkw` completa com as salas dele. Nos comandos agrupados
(mesmo título), o `hint` diz qual é qual ("à esquerda", "próxima"…). `world.back` aparece duas vezes: no ANDAR
(`s`, andar para trás — o título do comando é "andar") e no FOCO (`ctrl+]`, a ligação extra com "sai do FOCO");
o `nkw` decide pelo modo. O `nkw` nunca liga `app.quit`; os comandos `core` que o NAV herda de `normal`
(`r x n m X dd c a A ctrl+k`) agem sobre o gato da estação. `?` e `z` são do mundo no NAV (`world.help`,
`world.hud`) e sombreiam o `help.open` e o `pane.zoom` só ali: na TUI continuam os mesmos.

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
só a TUI (e, mais tarde, o GUI) sabe executá-lo sobre o seu próprio estado de tela; os do mundo, só o `nkw`.
De um script, `nk.ui.scene(nome, args)` pede ao `nkw` conectado que rode um comando do mundo
([lua.md](lua.md)).
