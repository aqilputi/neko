---
type: Reference
title: Lua (nk.*)
description: Runtime gopher-lua do nekuroddo — API nk.*, eventos, orçamentos, sandbox e reload, como implementado em internal/lua.
resource: https://github.com/aqilputi/nekuroddo/tree/main/internal/lua
tags: [lua, gopher-lua, scripting, nk, eventos]
timestamp: 2026-10-16T23:00:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T20:20:00Z }
status: draft
---

# Lua (`nk.*`)

`internal/lua` é uma VM `gopher-lua` por `*app.App`: um `init.lua` opcional serve a TUI, o RPC e o GUI ao
mesmo tempo, porque registra comandos no mesmo `Registry` e assina o mesmo barramento de eventos que eles
(plano §5). O pacote fala só com `internal/app` — nunca com Bubble Tea, `os/exec` ou o terminal direto.

**Como o host liga o runtime:** `cmd/nekuroddo/host.go` (o mesmo para `nk`, `nk --listen` e `nk serve`) cria
um `lua.Runtime` logo depois de `RegisterBuiltins`, com as opções do `[lua]` do `config.toml` (abaixo) e as
**capacidades de UI do cliente** (`nk` = TUI, `nk serve` = sem tela; veja [Capacidades](#capacidades-nkuihas)),
registra os comandos do próprio runtime (`lua.reload`, `lua.messages`, `lua.enable` — `Runtime.RegisterCommands`)
e chama `rt.Load()` na hora (um erro de carga vira só um aviso no stderr, nunca impede o programa de subir).
Com `auto_reload = true`, liga também o vigia de arquivos (`Runtime.AutoReload`). O servidor RPC do host
(`listenRPC`) recebe o runtime em `rpc.Options.Lua`, então `POST /v1/lua/eval` avalia código no mesmo runtime
(o `:lua` de um cliente remoto). A TUI (`internal/tui/lua.go`) usa o runtime para: `lua <código>` depois de
abrir a paleta (`:`) chama `Runtime.Eval` e mostra o resultado como toast; as teclas que um script religou com
`nk.keymap.set`/`del` (`Runtime.Keymap()`) são resolvidas **antes** do keymap do núcleo; e a ajuda (`?`) lista
essas teclas numa seção própria, "teclas do init.lua", abaixo das colunas do núcleo (a barra de status
continua só com o keymap do núcleo).

## Configuração (`[lua]` no `config.toml`)

```toml
[lua]
enabled = true               # false: não carrega nenhum arquivo (Load não faz nada; lua.reload/lua.enable falham)
command_timeout_ms = 5000    # comando Lua (paleta, tecla, RPC), carga de um arquivo, :lua / POST /v1/lua/eval
callback_timeout_ms = 50     # cada handler nk.on e cada tecla ligada a uma função
auto_reload = false          # true: recarrega sozinho quando init.lua ou plugins/ mudam
```

Tudo é opcional; a tabela inteira pode faltar. Um orçamento `≤ 0` volta ao padrão e o `config.Load` devolve
um erro dizendo qual chave (o `nk` mostra como aviso no stderr), **sem** descartar o resto do arquivo; um tipo
errado (`enabled = "sim"`) é erro de TOML e o arquivo todo cai nos padrões, como qualquer outra chave.
`config.Lua` → `lua.Options` em `luaOptions` (`host.go`): `CommandTimeout`, `CallbackTimeout`,
`Disabled = !enabled`.

**`auto_reload`** é um poller simples (sem `fsnotify`): a cada 1 s (`lua.AutoReloadEvery`) olha a lista de
arquivos que o `Reload` leria e o mtime/tamanho de cada um; se um arquivo apareceu, sumiu ou mudou, chama
`Reload()`. Um arquivo quebrado nessa hora tem o mesmo tratamento de um `lua.reload` manual (toast, anel de
mensagens, `lua.error`, último estado bom mantido) e o vigia segue. Para quando o runtime fecha.

## Arquivos e carga

```
~/.config/nekuroddo/init.lua         # opcional
~/.config/nekuroddo/plugins/*.lua    # ordem alfabética, depois de init.lua
```

(`Options.ConfigDir` sobrescreve o diretório; o padrão é `paths.ConfigDir()`.) `Load()`/`Reload()` relê os dois
e refaz comandos, teclas e handlers a partir do zero — à mão (`lua.reload`) ou sozinho com `auto_reload`. Sem
nenhum arquivo, nada muda: é o contrato "no-op" que a suíte cobre (`TestNoFilesChangesNothing`).

## Comandos do runtime

Registrados pelo host (`Runtime.RegisterCommands`) como builtins — rodam pela paleta, por RPC
(`POST /v1/commands/<nome>`) e por `nk run <nome>`, e **não** saem do registro quando um arquivo é descartado:

| comando | args | o que faz |
|---|---|---|
| `lua.reload` | — | `Reload()`; toast `lua recarregado (N arquivos)` ou o erro (arquivo:linha) |
| `lua.messages` | — | o anel de mensagens: `Result.Data` = lista `{at, file?, level, text}` (mais antiga primeiro); toast `lua: N mensagens · última: …` |
| `lua.enable` | `file` (obrigatório, ex.: `plugins/x.lua`) | religa um arquivo desativado após 3 falhas e recarrega; o erro lista os desativados |

Na TUI, a resposta de `lua.messages` aparece pelo tratamento genérico de resultado (o toast com a contagem e a
última mensagem); `tui/lua.go` também tem o diálogo "últimos erros lua" (`onLuaMessages`, uma linha
`hora nível arquivo: texto` por mensagem, as mais recentes que cabem), para quando o laço de resultados da TUI
o chamar para `lua.messages`.

## API (`nk.*`)

Toda função roda **na goroutine da VM**, dentro de uma chamada orçada, e só alcança o mundo por
`internal/app`.

### Comandos

```lua
nk.command(name, fn, { doc = "título", hint = "...", args = {...}, keys = {"ctrl+x"}, mode = "normal", confirm = false })
```
Registra `name` com `Scope = core`, `Source = "lua:<arquivo>"`. `fn(args, ctx)` recebe uma tabela de
argumentos (texto) e o contexto do chamador (`session_id, picked, tab, todo_id, issue_key`); o retorno vira o
`Result`: nada (`{}`), uma string (vira `toast`), ou uma tabela `{ toast=, err=, data=, pending= }` (uma tabela
de `pending` sozinha, como a que `nk.dispatch` devolve, também funciona — útil para repassar um `nk.dispatch`
direto). `args` aceita strings soltas (`{"dir", "prompt"}`, viram `ArgString`) ou tabelas
`{name=, type=, prompt=, required=}` com `type` em `string|session|dir|tag|int`. Dois arquivos não podem
registrar o mesmo nome (`RaiseError`); o mesmo arquivo pode redefinir o seu.

```lua
nk.run(name, args, ctx) -> result   -- roda QUALQUER comando do registro (builtin ou Lua); ctx opcional (default: o do chamador)
```
`nk.run` primeiro procura `name` nos arquivos do próprio runtime (o que está carregando agora, depois os já
"commitados", em ordem); achando, roda **inline** (sem round-trip pela VM), mesmo aninhado — um comando Lua
pode chamar `nk.run` de outro comando Lua sem problema. Só quando não acha aí é que cai para `app.Run`; nesse
caminho, se o `Registry` ainda assim disser que o comando é Lua (`Source` começa com `"lua:"`), `nk.run` recusa
com `"comando lua %s indisponível"` em vez de reentrar na goroutine da VM. Argumentos `required` faltando
levantam erro antes de rodar, nos dois caminhos.

### Teclado

```lua
nk.keymap.set(mode, key, cmd)   -- cmd: nome de comando registrado OU uma função (atalho anônimo, fora da paleta)
nk.keymap.del(mode, key)        -- tecla não faz mais nada (mesmo uma padrão)
```
As sobreposições de um arquivo valem por cima do `core.Keymap` vindo do registro; `GET /v1/keymap` e
`keymap.changed` (evento) refletem a mudança assim que ela é aplicada.

### Eventos

```lua
nk.on(event, fn)        -- fn(data, name); data é só dados (tabela), nunca uma referência viva
nk.off(event[, fn])     -- sem fn, tira todo handler desse evento do arquivo atual
```
`fn` roda sob o orçamento de **callback** (50 ms), por handler, por evento — um handler que falha não impede
os outros de rodar.

### Sessões, inbox

```lua
nk.sessions() -> { session, ... }   -- cópias de store.Session (nunca referências vivas)
nk.session(id) -> session | nil     -- por session_id OU short_id (background)
nk.inbox() -> { id, ... }           -- esperando você, mais urgente primeiro (core.SortInbox)
```
Uma `session` Lua tem `id, short_id, name, label, title, cwd, context, kind, state, model, cost, attention
("blocked"|"returned"|"idle"|""), stale, archived, ended, started, last_at (unix), waiting_for, needs, tags`.

### Disparo, parar, remover

```lua
nk.dispatch{ dir=, prompt=, name=, perm= } -> short_id | pending   -- perm: "", acceptEdits, auto, plan, bypassPermissions
nk.stop(id) -> result | pending
nk.remove(id) -> result | pending
```
`dir`/`prompt` são obrigatórios; `perm` fora da lista (`core.PermModes()`, a mesma de todo cliente) levanta erro; `extra` (do plano) **ainda não é
suportado** (`"dispatch: extra ainda não é suportado"`). Um `bypassPermissions` que não é o
`permission_mode` configurado devolve uma tabela `pending` (mesmo formato de `POST /v1/commands` —
`{token, question, expires_at}`) em vez de disparar; resolva com `app.Confirm`/`POST /v1/confirm` (não há um
`nk.confirm(token)` de resposta — só `nk.ui.confirm` para perguntas que o próprio script inicia). `id` aceita
`short_id`; se não achar na foto atual, um `Refresh()` é tentado antes de desistir (agent recém-disparado).

### Grupos

```lua
nk.group.add(tag, ids) / nk.group.remove(tag, ids)   -- ids: string ou lista
nk.group.notify(tag, text)  -- == nk.notify(tag, text)
nk.group.dissolve(tag)      -- sem confirmação (o script já é a vontade do usuário)
nk.group.tags() -> {tag, ...}          -- ordenado
nk.group.of(id) -> {tag, ...}
```

### UI (por capacidade do cliente — veja [Capacidades](#capacidades-nkuihas))

```lua
nk.ui.has(cap) -> bool                 -- "toast" | "confirm" | "cat" | "palette" | "scene" | "notify"
nk.ui.toast(text, is_err)
nk.ui.confirm(question, fn) -> token   -- qualquer cliente pode responder; "yes" roda fn no orçamento de comando
nk.ui.palette_open(prefill)
nk.ui.cat.meow(text)
nk.ui.notify(body, title)              -- notificação de desktop (fora da VM, goroutine própria); precisa do port Notify também
```
`"scene"` está na lista de capacidades mas nenhuma função de hoje a consulta — é reservada para o GUI (fase
3). `nk.ui.toast` (é `app.Toast`: o evento `toast` chega a todo cliente) e `nk.ui.confirm` (é `app.Ask`:
`confirm.requested`) sempre funcionam — não consultam capacidade; os hosts as declaram mesmo assim para que
`nk.ui.has("toast")`/`("confirm")` digam a verdade.

### Diversos

```lua
nk.version          -- 1 (número, não função)
nk.config_dir()      -- Options.ConfigDir
nk.state_dir()       -- paths.StateDir(), ~/.local/state/nekuroddo
nk.log([level,] msg) -- vai para o anel de mensagens (veja "erros" abaixo); level default "info"
```

## Eventos que dá para `nk.on`

Payload = os campos JSON do struct Go (mesmas chaves do protocolo RPC). Lua recebe a tabela convertida por
`jsonData`+`toLua`, que passa pelo `encoding/json` do Go — então um campo `omitempty` (marcado com `?` na
tabela abaixo) que está no zero-valor **não aparece** na tabela Lua (indexá-lo dá `nil`), exatamente como some
do JSON do protocolo.

| evento | de | payload |
|---|---|---|
| `snapshot.changed` | `app.go` | `{at, sessions, err?}` |
| `toast` | `app.go` | `{text, err?}` |
| `confirm.requested` | `confirm.go` | `{token, question, expires_at}` |
| `confirm.resolved` | `confirm.go` | `{token, outcome}` — `outcome`: `yes`\|`no`\|`expired` |
| `command.ran` | `app.go` | `{name, err?}` |
| `dispatched` | `dispatch.go` | `{id, dir, name?, task_id?}` |
| `session.state` | `watch.go` | `{id, prev, state}` |
| `session.notification` | `watch.go` | `{id, type?, message?}` |
| `notice` | `watch.go` | `{meow, bell?, toast?, toast_err?}` |
| `usage.updated` | `watch.go` | `{at?, err?}` |
| `pane.opened` / `pane.closed` / `pane.exited` | `panes.go` | `{id, session_id?, err?}` |
| `pane.resized` | `rpc/panes.go` (publicado pelo `app` a pedido do rpc) | `{id, cols, rows}` — só dispara para painéis abertos via `POST /v1/panes`, não pelos da TUI |
| `group.changed` | `groups.go` | `{tag}` (`""` quando vários grupos mudaram de uma vez) |
| `peer.message` | `groups.go` | `{to, from, from_id?, text}` |
| `mail.cue` | `groups.go` | `{id, sent}` |
| `todos.changed` | `todo.go` | `{groups, err?}` |
| `jira.changed` | `jira.go` | `{fetched_at, issues, cost_usd, err?}` |
| `lua.error` | `lua/runtime.go` | `{file?, error}` — publicado pelo próprio runtime (abaixo) |
| `keymap.changed` | `lua/loader.go` | `nil` — só avisa que `GET /v1/keymap` mudou |
| `ui.palette_open` | `lua/bridge.go` | `{prefill}` — só sai se o host tiver a capacidade `"palette"` |
| `ui.cat.meow` | `lua/bridge.go` | `{text}` — só sai com a capacidade `"cat"` |

`nk.on` só assina eventos que algum handler pediu (`r.wanted`), então o `pump` não acorda a VM à toa.

## Orçamentos

| chamada | orçamento padrão | opção |
|---|---|---|
| comando (`nk.command`, palette/tecla/RPC), `Eval`, carregar um arquivo | 5 s | `Options.CommandTimeout` (`[lua] command_timeout_ms`) |
| `nk.on`, tecla de função (`nk.keymap.set` com `fn`) | 50 ms | `Options.CallbackTimeout` (`[lua] callback_timeout_ms`) |

Toda entrada passa por `context.WithTimeout` + `LState.SetContext`; um estouro vira `lua: orçamento de tempo
estourado (<duração>)` (`ErrBudget`), nunca trava a VM nem o processo (`TestInfiniteLoopIsInterruptedWithinBudget`
é o primeiro teste do pacote: um `while true do end` é cortado dentro do orçamento + folga). Um pânico do VM é
recuperado e também vira erro Go. Uma chamada feita **de dentro** de outra (por exemplo `nk.run` chamando um
comando Lua a partir de um handler) **compartilha o orçamento da chamada de fora** — não reinicia o relógio.
Disparar um processo (`nk.dispatch`) só enfileira o pedido na porta `ClaudeCLI`; o `exec` em si roda fora do
orçamento da VM.

## Sandbox

A `LState` abre só o subconjunto seguro da stdlib: `base`, `table`, `string`, `math`, `coroutine`, `os`
(reduzido). De `os` saem `exit` (derrubaria o processo hospedeiro), `execute`, `remove`, `rename`, `setenv`,
`setlocale`, `tmpname`; `dofile` e `loadfile` são removidos do escopo global. Não há `io`, nem
`package`/`require` — nenhum jeito de ler/escrever arquivo ou carregar código externo a não ser pelos
arquivos que o próprio `discover()` lista.

## Reload

`Reload()` relê `init.lua` + `plugins/*.lua` (em ordem) e, **por arquivo**, roda o arquivo "staged" (nada
chega ao registro ainda); só quando um arquivo termina sem erro é que `commit` troca o estado dele no lugar
do anterior — comandos velhos saem do `Registry`, os novos entram. **Isso é atômico por arquivo, não um swap
único de tudo**: um arquivo que falha mantém seus últimos comandos/teclas/handlers bons (ou nenhum, na
primeira carga), e o erro aponta arquivo:linha; os outros arquivos carregam normalmente mesmo se um deles
quebrar (`TestBrokenFileDoesNotStopTheOthers`). Um arquivo que sumiu do disco é descartado; um arquivo
desativado (abaixo) é pulado até `Enable`.

## Erros, mensagens e desativação automática

Todo erro (de carga, de comando, de handler, de tecla) vira: um toast (`lua: <mensagem>`), uma entrada no anel
de até 100 mensagens (`Runtime.Messages()`, comando `lua.messages`; mais antiga descartada) e o evento `lua.error {file?, error}` —
exceto quando o próprio handler de `lua.error` é quem falhou (não alimenta a si mesmo:
`TestLuaErrorHandlerFailureDoesNotFeedItself`). **Três falhas seguidas** de callbacks (`nk.on`/tecla) do
**mesmo arquivo** desativam só aquele arquivo (`Runtime.Disabled()` lista; `Enable(file)` — o comando `lua.enable file=<arquivo>` — religa e recarrega);
um sucesso zera a contagem. Falhas de `Eval` (`:lua`) nunca contam para a desativação.

## Capacidades (`nk.ui.has`)

`Options.Caps` é a lista que o host declara ao criar o runtime (constantes `lua.CapToast`, `CapConfirm`,
`CapCat`, `CapPalette`, `CapScene`, `CapNotify`). `nk.ui.has(cap)` só olha essa lista; as funções que dependem
de capacidade (`nk.ui.palette_open`, `.cat.meow`, `.notify`) voltam `false` sem fazer nada quando ela falta,
em vez de erro — então um script pode checar `nk.ui.has("notify")` e cair para `nk.ui.toast` (é o que o
exemplo 2 abaixo faz). Uma VM só por processo: as capacidades são as do processo, não de cada cliente RPC
conectado nele.

| processo | capacidades | por quê |
|---|---|---|
| `nk` (TUI; também com `--listen`) | `toast`, `confirm`, `cat`, `palette`, `notify` | a TUI desenha tudo menos a cena do GUI |
| `nk serve` (e `--demo`) | `toast`, `confirm`, `notify` | sem tela: só o que chega a um cliente remoto pelo barramento (`toast`, `confirm.requested`) e a notificação de desktop, que o próprio processo manda |

`nk.ui.cat.meow` e `nk.ui.palette_open` **publicam eventos** (`ui.cat.meow {text}`, `ui.palette_open
{prefill}`) no barramento — o SSE os leva a clientes remotos também. A TUI os desenha pelo handler
`onLuaUIEvent` (`internal/tui/lua.go`): o miado vai para o gato da barra de status (cortado em 50 colunas) e
`palette_open` abre a paleta com o texto já digitado (uma paleta aberta só troca o texto). O handler é chamado
pelo ouvinte do barramento da TUI para os eventos em que `isLuaUIEvent(nome)` é verdadeiro.

## Ajuda (`?`)

As teclas de `nk.keymap.set`/`del` aparecem na ajuda numa seção "teclas do init.lua", abaixo das colunas do
núcleo (em até 3 colunas): uma tecla ligada a comando mostra o título do comando (ou o nome, sem título), uma
ligada a função mostra "função lua", uma apagada mostra "desligada", e uma fora do NORMAL leva o modo entre
colchetes (`ctrl+y  inbox [insert]`). Sem teclas Lua, a ajuda é a de sempre.

## Os três exemplos de `internal/lua/testdata/init.lua`

Carregado pelos testes (`TestExampleInit`) contra fakes do `app`, não é o `init.lua` real de ninguém — mas é a
melhor referência de como a API se encaixa:

1. **Auto-agrupar por diretório.** Em todo `snapshot.changed`, junta ao grupo `#svc` toda sessão cujo `cwd`
   bate com `/svc%-` (padrão Lua) e que ainda não está no grupo (`nk.group.of` + checagem manual, já que não
   há `nk.group.has`).
2. **Notificação de deploy.** Em `peer.message`, se o texto menciona `[#deploy`, manda uma notificação de
   desktop (`nk.ui.notify`) se o host tiver a capacidade, senão cai para um toast — o padrão de fallback que
   todo script que usa `nk.ui.*` deveria seguir (um host sem `"notify"` cai no `else`).
3. **`deploy-all`.** Um comando (`nk.command`, tecla `ctrl+shift+d`) que dispara quatro agents
   (`nk.dispatch`, um `perm = "acceptEdits"` por diretório), agrupa os IDs resultantes em `#deploy` e mostra
   um toast com a contagem — mostra `nk.dispatch` chamado em loop e `nk.group.add` com uma lista de IDs.
