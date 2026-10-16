---
type: Reference
title: Protocolo v1 (RPC)
description: HTTP+JSON, SSE e WebSocket servidos por internal/rpc sobre o core — endpoints, erros, eventos e painéis remotos, como implementado hoje.
resource: https://github.com/aqilputi/nekuroddo/tree/main/internal/rpc
tags: [rpc, protocolo, http, sse, websocket, api]
timestamp: 2026-10-16T22:00:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T20:00:00Z }
status: draft
---

# Protocolo v1 (RPC)

`internal/rpc` serve um `*app.App` pelo protocolo v1 do [plano](plano-arquitetura.md#3-protocolo-do-core-v1):
HTTP+JSON para comandos e consultas, SSE para o barramento de eventos e um WebSocket por painel para os bytes
crus do PTY. É um shell como a TUI — traduz pedidos em chamadas de `app` e nunca fala direto com
claude/store/groups/jira/todo — e continua opcional: um `nk` comum (sem `serve` nem `--listen`) nunca sobe um
`rpc.Server`.

**Estado no código:** o pacote (`server.go`, `events.go`, `panes.go`, `view.go`, `client.go`, `proto/proto.go`,
`proto/view.go`) tem testes próprios (≥ 90% de cobertura) e cobre todas as rotas do §3 do plano — as da fase 2
(`/v1/view/tree`, `/v1/theme`, `/v1/lua/eval`) incluídas (WP-5.2). Já está ligado de verdade em `cmd/nekuroddo`
(WP-3.4):

- `nk serve [--listen <endereço>]` sobe o core sem TUI, servido por RPC; sem `--listen`, escuta em
  `unix://$XDG_RUNTIME_DIR/nekuroddo-<uid>.sock` (`rpc.DefaultSocketPath()`).
- `nk [--listen <endereço>]` é a TUI de sempre; com `--listen`, o **mesmo processo e o mesmo core** também
  respondem pelo protocolo (TUI e um cliente remoto enxergam as mesmas sessões, painéis e confirmações).
- `nk run [<comando> [arg=valor…]] [--session <id>] [--yes] [--addr <endereço>] [--token <token>]` é o cliente
  de linha de comando do protocolo: sem comando, lista `GET /v1/commands`; com um, faz `POST
  /v1/commands/{nome}`, responde um `pending` com um `[y/N]` no stdin (`--yes` pula a pergunta) e imprime o
  `Result` (`cmd/nekuroddo/remote.go`).

Em ambos os casos o host (`cmd/nekuroddo/host.go`) usa `wire.Default(cfg)`, então `Ports.Panes` é um spawner de
verdade — `POST /v1/panes` de um `nk serve` comum abre painéis de PTY de verdade, não `501`. Um listener TCP
(`tcp://127.0.0.1:porta`) ganha um token novo a cada vez, gravado em `rpc.DefaultTokenPath()` (o que `nk run`
lê sozinho quando `--addr` é `tcp://` e falta `--token`).

## Transporte e autenticação

| transporte | endereço | autenticação |
|---|---|---|
| Unix socket | `unix:///caminho` — padrão `rpc.DefaultSocketPath()` = `$XDG_RUNTIME_DIR/nekuroddo-<uid>.sock` | nenhuma: o arquivo nasce com permissão `0600` (só o dono lê/escreve) |
| TCP loopback | `tcp://127.0.0.1:porta` (ou `localhost`; qualquer outro host é recusado) | obrigatória: `Authorization: Bearer <token>`, ou `?token=<token>` para SSE/WebSocket, que não controlam cabeçalhos |

`rpc.Listen(spec)` entende as duas formas; `rpc.ListenUnix(path)` troca um socket morto (ninguém responde) e
recusa um arquivo que não seja socket ou um socket vivo (`ErrInUse`). `Server.Serve(l)` exige
`Options.Token` para qualquer listener que não seja unix (`ErrNoToken` se faltar); a checagem usa
`crypto/subtle.ConstantTimeCompare`. O token é gerado com `rpc.NewToken()` (`crypto/rand.Text()`) e persistido
com `rpc.WriteTokenFile`/`ReadTokenFile` em modo `0600`, por padrão em `rpc.DefaultTokenPath()` =
`$XDG_RUNTIME_DIR/nekuroddo-<uid>.token`. Um host embutido que já confia no chamador (um teste, um proxy que já
autenticou) usa `Server.Handler()` — o `http.Handler` puro, sem a camada de auth.

## Versionamento

`proto.Version = 1` é o `hello.protocol` e o prefixo `/v1` de toda rota. Dentro de v1 as mudanças são só
aditivas: campos novos chegam opcionais e um cliente ignora o que não conhece; uma mudança incompatível vira
`/v2` convivendo com `/v1` (comentário de pacote em `proto/proto.go`).

## `GET /v1/hello`

```
$ curl --unix-socket $XDG_RUNTIME_DIR/nekuroddo-$(id -u).sock http://nk/v1/hello
{"app":"nekuroddo","version":"dev","protocol":1,"capabilities":["panes"],"pid":48213}
```

`capabilities` sempre inclui `"panes"` (`proto.CapPanes`); `"lua"` (`proto.CapLua`) aparece sozinho quando o
host passa um runtime em `Options.Lua` (ver [`POST /v1/lua/eval`](#post-v1luaeval)), e `"demo"`
(`proto.CapDemo`) quando o host passa `Options.Capabilities` com ele (`nk serve --demo`, ver [Modo
demo](#modo-demo-internaldemo)). `version` é `Options.Version` (a versão do binário, em texto livre); `pid` é o do processo do
servidor.

## `GET /v1/snapshot`

Tudo que um cliente precisa para desenhar a tela, num só pedido:

```json
{
  "sessions": [
    {"session_id": "abcd1234", "short_id": "abcd1234", "label": "trabalho · api", "cwd": "~/work/api",
     "kind": "background", "state": "busy", "attention": 0, "cost": 0.42, "model": "sonnet-5", "subs": []}
  ],
  "inbox": ["ef567890"],
  "groups": {"deploy": [{"id": "abcd1234", "name": "api"}]},
  "mail": {"abcd1234": {"at": "2026-10-16T19:58:00Z", "sent": false}},
  "usage": {"limits": [{"name": "5h", "pct": 40}], "at": "2026-10-16T19:58:00Z"},
  "todos": {"groups": []},
  "jira": {"issues": []},
  "pending": [],
  "at": "2026-10-16T19:58:01Z",
  "today_cost": 3.27
}
```

`sessions[]` é `store.Session` convertido por `proto.FromSession` (nenhum campo tem `omitempty` em
`core.Command`, mas em `proto.Session` quase todos têm — campos vazios somem do JSON; a lista completa está em
`internal/rpc/proto/proto.go`). `inbox` é a mesma ordem de `core.SortInbox` usada pela TUI. `groups` nunca é
`null` (um mapa vazio se não há grupos); o mesmo vale para `pending`. `err` (omitido aqui) carrega o erro do
último refresh, se houve.

## Comandos

### `GET /v1/commands`

Lista **todo** comando registrado, na ordem de registro, com `available` calculado para a seleção do
`?session_id=&picked=a,b&tab=&todo_id=&issue_key=` do caller (`picked` aceita vírgulas ou `picked=a&picked=b`
repetido). `CommandInfo` embute `core.Command` inteiro — como nenhum campo ali tem `omitempty`, toda entrada
mostra todos os campos:

```
$ curl --unix-socket $SOCK "http://nk/v1/commands?session_id=abcd1234"
[
  {"name":"session.archive","title":"arquiva/restaura (no contexto: todas)","hint":"","keys":["X"],
   "mode":"normal","args":[{"name":"context","type":"string","prompt":"contexto","required":false}],
   "scope":"core","confirm":true,"source":"builtin","available":true},
  {"name":"nav.down","title":"desce","hint":"","keys":["j","down"],"mode":"normal","args":null,
   "scope":"client","confirm":false,"source":"builtin","available":true}
]
```

Comandos Lua aparecem na mesma lista com `"source":"lua:<arquivo>"` (ver [lua.md](lua.md)); a lista completa de
nomes, títulos, teclas e argumentos builtin está em [comandos.md](comandos.md).

### `POST /v1/commands/{name}`

```
$ curl -XPOST --unix-socket $SOCK http://nk/v1/commands/session.rename \
    -d '{"args":{"name":"novo nome"},"context":{"session_id":"abcd1234"}}'
{"toast":"seção renomeada","data":{"refresh":true}}
```

O corpo é `proto.RunRequest{args, context}` (`context` nos mesmos campos de `?session_id=…`, todos opcionais);
a resposta é `proto.Result{toast, err, pending, exec, data}` — o mesmo `app.Result` que a TUI recebe. `data` é
específico de cada comando (ex.: `app.SessionEffect{close_panes, refresh}` nos comandos de sessão); um cliente
remoto que não entende um `data` específico pode ignorá-lo. Um comando `Confirm: true` não necessariamente
responde com `pending` — isso é decisão do próprio caso de uso (`a.Ask`), não do metadado (ver
[comandos.md](comandos.md#confirmação-metadado-vs-comportamento)):

```
$ curl -XPOST --unix-socket $SOCK http://nk/v1/commands/session.remove \
    -d '{"context":{"session_id":"abcd1234"}}'
{"pending":{"token":"7f3e…","question":"remover \"api\" (e o worktree, se seguro)?",
            "expires_at":"2026-10-16T20:01:01Z"},
 "data":{"close_panes":["abcd1234"]}}
```

Quando o servidor é ele mesmo quem abriu o painel da sessão (via `POST /v1/panes`), `Server.effects` lê
`data` e fecha esses painéis sozinho quando a ação finalmente roda (nunca enquanto só há `pending`) e dispara
um `Refresh()` em segundo plano se `refresh` veio `true` — um cliente HTTP puro não precisa reimplementar essa
lógica para o seu próprio painel, mas pode olhar `data` para refletir o mesmo (ex.: fechar a janela do GUI).

## `POST /v1/confirm/{token}`

Responde a um `pending` de qualquer comando (ou de `nk.ui.confirm` do Lua). Com `{"yes":false}` cancela
(`app.Cancel`) e devolve `200 {}`; com `{"yes":true}` roda a ação que ficou estacionada (`app.Confirm`) e
devolve o `Result` dela:

```
$ curl -XPOST --unix-socket $SOCK http://nk/v1/confirm/7f3e… -d '{"yes":true}'
{"data":{"close_panes":["abcd1234"],"refresh":true}}
```

Token desconhecido ou já usado → `404 {"error":"confirmação desconhecida ou já respondida","code":"unknown_token"}`;
expirado (`ConfirmTTL` = 60 s) → `410 {"error":"confirmação expirou","code":"expired"}`. Toda resolução —
`yes`, `no` ou `expired` — publica `confirm.resolved {token, outcome}` para **todos** os clientes conectados
(não só quem respondeu), assim qualquer um fecha o diálogo; `confirm.requested` sai assim que o `pending`
nasce, pelo mesmo motivo. `GET /v1/snapshot` traz `pending: []core.PendingConfirmation` com o que ainda está
de pé, para um cliente que conectou depois de o diálogo abrir.

## `GET /v1/keymap`

```json
[{"command":"session.archive","keys":["X"],"mode":"normal","help":{"key":"X","desc":"arquiva/restaura (no contexto: todas)"}}]
```

É `core.BuildKeymap(Registry.All())` — a mesma tabela que a TUI resolve teclas com, então um `nk.keymap.set` do
Lua aparece aqui também (quando o host embutir Lua; ver [lua.md](lua.md)).

## Eventos — `GET /v1/events` (SSE)

Um pump assina o barramento do `app` assim que o `Server` é criado (`New`) e guarda, já codificados, os
últimos `Options.Backlog` eventos (padrão `DefaultBacklog = 1024`) num anel. Cada envelope SSE é:

```
id: 42
event: session.state
data: {"seq":42,"at":"2026-10-16T20:00:00Z","name":"session.state","data":{"id":"abcd1234","prev":"idle","state":"busy"}}

```

`data:` carrega `proto.Event{seq, at, name, data}` — o nome do evento aparece duas vezes (no `event:` da linha
SSE e dentro do JSON) de propósito, para quem só lê o corpo. A conexão abre com `retry: 1000` e um comentário
`: ok`; a cada `Options.Heartbeat` (padrão 15 s) um comentário `: ping` mantém o socket vivo sem virar evento.

**Retomada:** `Last-Event-ID` (automático no `EventSource` do browser) ou `?since=<seq>` (para `curl`/scripts).
`since=0` (ou `Last-Event-ID: 0`) pede "tudo que você tem, mais o que vier" sem contar como lacuna; sem nenhum
dos dois, o cliente só recebe eventos novos a partir de agora. Se o `since` pedido já saiu do anel (ou é maior
que o último conhecido — outro processo do servidor), o cliente recebe primeiro um evento **sem `id:`**:

```
event: gap
data: {"since":42,"oldest":950}
```

e deve tratar isso como "relê o snapshot, perdi alguma coisa entre esses números". O mesmo `gap` (com `oldest`
zerado) é mandado a **todos** os clientes conectados se o pump interno atrasar e perder eventos do próprio
barramento do `app` (ele se reassina sozinho e avisa). Um cliente SSE que não drena rápido o bastante (mais de
`listenerBuffer = 1024` eventos pendentes) é desconectado e deve reconectar com o último `Last-Event-ID` que
recebeu — o anel de 1024 normalmente ainda cobre o intervalo.

```
$ curl -N --unix-socket $SOCK -H 'Accept: text/event-stream' "http://nk/v1/events?since=0"
```

## Painéis — PTYs remotos

O **core** decide *como* mostrar uma sessão (reusar painel aberto, anexar a uma rodando em background, ou
retomar um histórico primeiro) com a mesma regra da TUI (`app.PaneManager.Open/Resume/Attach`); o **rpc** é só
mais um cliente dessa decisão — é ele quem efetivamente abre o PTY (pela porta `app.PTYSpawner`, por padrão
`a.Ports().Panes`) e relaia os bytes crus por um WebSocket.

| rota | o quê |
|---|---|
| `GET /v1/panes` | painéis que **este servidor** abriu: `[{id, session_id, title, cols, rows}]` |
| `POST /v1/panes` | `{session_id, cols?, rows?, reply?}` (padrão 80×24) → `{pane_id, reused?}`; `501 not_implemented` se o servidor não tem `PTYSpawner` e precisaria abrir um painel novo (reusar um já aberto ainda funciona) |
| `WS /v1/panes/{id}/ws` | binário cliente→servidor = teclado/paste cru para o PTY; binário servidor→cliente = saída crua; texto = frame de controle |
| `POST /v1/panes/{id}/resize` | `{cols, rows}` → `204`; última escrita vence, publica `pane.resized {id, cols, rows}` |
| `DELETE /v1/panes/{id}` | fecha (SIGHUP ao programa) → `204` |

Frames de controle (JSON em frame **texto**, nunca binário):

```
cliente → servidor:  {"type":"resize","cols":120,"rows":40}
servidor → cliente:  {"type":"title","text":"claude · api"}
servidor → cliente:  {"type":"exit"}              // ou {"type":"exit","err":"..."}
```

`exit` é sempre o último frame antes do servidor fechar o socket com `1000` (normal). Vários visualizadores
podem abrir o mesmo `{id}`; um saindo não derruba o painel. Se o programa já tinha terminado e o visualizador
demora demais para reconectar e pegar o status de saída, o servidor fecha com **`1013` (try again later)**
depois de `exitWait` (2 s) — reconecte. Tamanho máximo de um lado é 2000 colunas/linhas; frames (binário ou
texto) são limitados a 1 MiB. Sem checagem de `Origin`: o unix socket só é alcançável pelo dono do processo, e
o TCP exige o token (uma página de outro site não o conhece).

Um host que já roda o PTY no mesmo processo (a própria TUI, um `nkg` embutido) pode publicá-lo no servidor com
`Server.AttachPane(pane, sessionID, cols, rows)` em vez de deixar o `POST /v1/panes` abrir outro — hoje só os
testes do pacote chamam isso; nenhum binário o usa ainda.

## Códigos de erro

Toda resposta não-2xx é `{"error": "<mensagem em pt-BR>", "code": "<código>"}` (`proto.Error`). O `code` é o
que um cliente deve testar (`rpc.IsCode(err, "expired")`); o status HTTP por si só não distingue tudo (dois
códigos diferentes podem compartilhar status, como `not_found` e `unknown_command`, ambos `404`):

| status | `code` | quando |
|---|---|---|
| 400 | `bad_request` | JSON do corpo malformado; `?since=`/`Last-Event-ID` não numérico; `?todo_id=` não numérico; `session_id` faltando em `POST /v1/panes`; tamanho de painel inválido; `tab`/`sort` desconhecidos em `POST /v1/view/tree`; `code` vazio em `POST /v1/lua/eval` |
| 401 | `unauthorized` | TCP sem `Authorization: Bearer`/`?token=` válido |
| 404 | `not_found` | sessão ou painel desconhecido (`POST /v1/panes`, `.../resize`, `.../ws`, `DELETE`); tema desconhecido em `GET /v1/theme?name=` |
| 404 | `unknown_command` | `POST /v1/commands/{name}` com nome não registrado |
| 404 | `unknown_token` | `POST /v1/confirm/{token}` com token desconhecido ou já consumido |
| 400 | `client_command` | o nome existe mas é `scope: client` (sem `Runner`; roda só no cliente) |
| 409 | `unavailable` | comando existe mas `available(ctx)` nega para essa seleção (ex.: sem sessão sob o cursor) |
| 400 | `missing_arg` | falta um argumento `required` do comando |
| 410 | `expired` | confirmação expirada (60 s) |
| 501 | `not_implemented` | `POST /v1/lua/eval` num servidor sem `Options.Lua`, ou `POST /v1/panes` sem `PTYSpawner` quando precisaria abrir um painel |
| 422 | `failed` | qualquer outro erro do `app` (fallback) |
| 500 | `failed` | a própria resposta não serializou em JSON (nunca deveria acontecer) |
| 503 | `failed` | `GET /v1/events` enquanto o servidor está fechando |

`proto.CodeConflict` está declarado mas **nenhum handler o emite hoje** — `ErrUnavailable` vira `409` com
`code: "unavailable"`, não `"conflict"` (ver "inconsistências" no relatório final do WP-3.5).

## `POST /v1/view/tree`

A árvore contexto → sessão → subagent já filtrada, ordenada e dobrada por `core.BuildTree` — a mesma função que
desenha a árvore da TUI — sobre o snapshot do core (`internal/rpc/view.go`). **O servidor não guarda estado de
visão:** o cliente manda o `state` que recebeu da última vez e a próxima construção continua dele (ordem
congelada, paleta dos contextos, dobras, cursor). O primeiro pedido pode ir vazio (`{}` ou corpo vazio).

```
$ curl -XPOST --unix-socket $SOCK http://nk/v1/view/tree -d '{"state":{"tab":"sessions"}}'
{                                   (trecho, sobre o mundo de `nk serve --demo`)
  "rows": [
    {"key":"ctx:visio","kind":"ctx","ctx":"visio","depth":0,"color":0,"ctx_cost":6.78,"ctx_n":3,"attention":0},
    {"key":"sess:d3e0a001-…","kind":"sess","ctx":"visio","depth":1,"color":0,"session_id":"d3e0a001-…",
     "short_id":"dmgatewy","label":"api-gateway","session_kind":"background","state":"blocked","attention":1,
     "cost":2.13,"options":3,"groups":[{"tag":"deploy-rc","color":<core.GroupColorIndex("deploy-rc")>}],
     "mail":{"at":"2026-10-16T20:00:20Z","sent":true},"mail_state":"blink"},
    {"key":"sub:agent-idx-a","kind":"sub","ctx":"oss","depth":2,"color":2,"session_id":"d3e0a006-…",
     "short_id":"dmindexr","label":"mapear os módulos","state":"busy","attention":0,"cost":0.2,
     "sub":{"id":"agent-idx-a","description":"mapear os módulos","type":"Explore","cost":0.2}}
  ],
  "state": {"tab":"sessions","sort":"atividade","order":{"ctx:visio":0,"sess:d3e0a001-…":1},
            "ctx_color":{"visio":0,"pessoal":1,"oss":2},"cursor":0,"cursor_key":"ctx:visio","inbox_cursor":0},
  "inbox": ["d3e0a001-…","d3e0a002-…"],
  "hidden": 1,
  "archived": 0
}
```

**Pedido** — `proto.TreeRequest{state}`; `state` é `proto.ViewState` (todos os campos opcionais):

| campo | o quê |
|---|---|
| `tab` | `sessions` (padrão) \| `inbox` \| `todos` \| `archive` (`archive` lista só o que foi arquivado à mão) |
| `filter` | filtro fuzzy (label, contexto, nome, cwd); com filtro, sessões velhas também aparecem |
| `show_all` | também sessões velhas/arquivadas e subagents terminados (o `H` da TUI) |
| `sort` | `atividade` \| `custo` \| `nome`; vazio = o `sort` da config. Ao trocar, mande `reset_order: true` uma vez |
| `order`, `new_rank` | a ordem congelada (chave de linha → rank); devolva como recebeu |
| `collapsed` | dobras por chave (`"ctx:<nome>"`, `"sess:<id>"`) → `true` |
| `ctx_color` | contexto → índice de paleta, na ordem em que apareceram; devolva como recebeu |
| `cursor`, `cursor_key` | linha selecionada; a próxima construção segue `cursor_key` se a linha mudou de lugar |
| `inbox_cursor` | seleção na inbox (só limitada ao tamanho) |
| `reset_order` | só no pedido: reordena pelo critério atual (o `S`/`R` da TUI) |

`tab` ou `sort` desconhecidos → `400 bad_request`.

**Resposta** — `proto.Tree{rows, state, inbox, hidden, archived}`. `inbox` são os ids que pedem atenção, na
ordem de `core.SortInbox` (bloqueadas, depois devolvidas, depois ociosas; desempate pela ordem da árvore);
`hidden` conta as velhas que o filtro escondeu; `archived` as arquivadas à mão. Cada linha
(`proto.TreeRow`):

| campo | em | o quê |
|---|---|---|
| `key`, `kind`, `ctx`, `depth` | todas | `core.RowKey`; `ctx`\|`sess`\|`sub`; o contexto; 0/1/2 |
| `color` | todas | índice da paleta do contexto (`state.ctx_color[ctx]`); a cor é `tokens.contexts[color % len]` de `/v1/theme` |
| `collapsed` | ctx, sess | a linha está dobrada |
| `ctx_cost`, `ctx_n` | ctx | custo e número das sessões listadas sob o contexto |
| `session_id`, `short_id` | sess, sub | a sessão (numa linha `sub`, a mãe) |
| `label`, `state`, `cost` | sess, sub | `Label()`, `store.Session.State`, custo; numa `sub`: a descrição (ou o tipo), `busy`\|`done`, o custo do subagent |
| `session_kind`, `attention` | sess | `background`\|`interactive`\|`history`; 0 nada, 1 bloqueada, 2 devolvida, 3 ociosa |
| `stale`, `archived`, `ended`, `finished` | sess | as mesmas flags do snapshot |
| `subs_running`, `options` | sess | subagents rodando; opções numeradas das perguntas pendentes (`core.OptionCount`) |
| `groups` | sess | `[{tag, color}]`, `color` = `core.GroupColorIndex(tag)` (a coleira) |
| `mail`, `mail_state` | sess | o ✉ enquanto aparece: `{at, sent}` e `blink` (primeiros 90 s) ou `dim` (até 15 min) |
| `sub` | sub | o subagent inteiro (`proto.Sub`) |

O resto de uma sessão (perguntas, links, resultado…) está em `GET /v1/snapshot`, pela `session_id`.

## `GET /v1/theme`

Os tokens de cor de um tema como `#rrggbb` (`theme.Theme.Tokens`/`ContextTokens`, em
`internal/theme/export.go`) e o nome de todos os temas, para o GUI pintar igual à TUI:

```
$ curl --unix-socket $SOCK "http://nk/v1/theme?name=dracula"
{"name":"dracula",
 "tokens":{"bg":"#282a36","surface":"#343746","overlay":"#44475a","border":"#6272a4","text":"#f8f8f2",
           "subtle":"#e2e2dc","muted":"#6272a4","accent":"#ff79c6","accent2":"#bd93f9","busy":"#8be9fd",
           "blocked":"#ff5555","done":"#50fa7b","idle":"#6272a4","error":"#ff5555","cost_low":"#50fa7b",
           "cost_high":"#ff5555","cat_body":"#191a21","cat_eyes":"#f1fa8c"},
 "contexts":["#ff79c6","#bd93f9","#50fa7b","#ffb86c","#8be9fd","#f1fa8c"],
 "themes":["catppuccin-mocha","tokyo-night","gruvbox","dracula","rose-pine-dawn"],
 "active":"catppuccin-mocha"}
```

Sem `?name=`, é o tema da config (`theme = "…"`); como um servidor não vê o fundo do terminal, `theme = ""`
("segue o terminal") vira o primeiro tema. `active` é sempre esse tema da config, mesmo quando `?name=` pede
outro (um seletor de temas sabe qual está marcado). `?name=` com um nome que não existe (inclusive vazio) →
`404 not_found`. `contexts` é a paleta dos contextos e das coleiras dos grupos: o índice `i` (`color` das linhas
de `/v1/view/tree`) usa `contexts[i % len(contexts)]`, como `Theme.ContextColor` na TUI.

## `POST /v1/lua/eval`

O `:lua <código>` da paleta, por HTTP, com o mesmo orçamento do runtime. Precisa de um runtime Lua no
servidor: o host passa `rpc.Options{Lua: rt}` — qualquer coisa com `Eval(code string) (string, error)`, como
o `*lua.Runtime` de `internal/lua` — e `hello` passa a anunciar `"lua"`. Sem runtime → `501 not_implemented`.

```
$ curl -XPOST --unix-socket $SOCK http://nk/v1/lua/eval -d '{"code":"return 6*7"}'
{"result":"42"}
$ curl -XPOST --unix-socket $SOCK http://nk/v1/lua/eval -d '{"code":"error(\"x\")"}'
{"error":"…: x"}
```

Um erro do Lua (sintaxe, `error()`, orçamento estourado) ainda é `200` — a chamada funcionou — com `error` no
lugar de `result` (`proto.LuaEval`). `code` vazio → `400 bad_request`.

## Modo demo (`internal/demo`)

`internal/demo` é um mundo de mentira inteiro atrás das portas do `app` (`demo.Ports(clock)` /
`demo.New(clock).Ports()` preenchem **todas** as portas), para `nk serve --demo` e para os testes Playwright do
GUI. Nada ali roda `claude`: os painéis rodam um `sh` inofensivo (`internal/demo/screen.sh`) que desenha uma
tela parecida com a do Claude Code e responde cada linha com um eco (`/exit` encerra). O roteiro é função pura
do tempo desde o início (lido do `Clock`), então com um relógio falso tudo é determinístico; por cima dele o
`Scenario` guarda o que o usuário fez (renomear, mover, arquivar, `session.new`, parar, remover…), e os
comandos do core têm efeito visível.

| sessão (`demo.ID…`) | contexto | o quê |
|---|---|---|
| `api-gateway` (`IDGateway`, `dmgatewy`) | visio | `busy` ⇄ `blocked` a cada 15 s; bloqueada, pergunta "Aplico a migração 0042 no staging?" com 3 opções e dispara uma notificação `permission_prompt` |
| `migração do banco` (`IDMigration`) | visio | sempre `blocked`, pergunta com 3 opções |
| `relatório Q3` (`IDReport`) | visio | `done`, devolvida (some da inbox ao ser vista) |
| `dotfiles` (`IDDotfiles`) | pessoal | interativa, `idle` no prompt |
| `blog antigo` (`IDBlog`) | pessoal | histórico velho (`stale`, escondida sem `show_all`) |
| `indexador` (`IDIndexer`) | oss | `busy` com 2 subagents rodando (e 1 terminado) |
| `site de docs` (`IDDocs`) | oss | `busy` → `done` (devolvida) → `idle`, a cada 15 s |

O grupo `#deploy-rc` tem `api-gateway`, `migração do banco`, `indexador` e `site de docs`; a cada 20 s um ✉
(`peer`) passa de um para o próximo, em roda. O uso da janela de 5 h fica entre 58% e 62%; há 5 todos, 4
issues `DEMO-…` (criar uma devolve `DEMO-201`, `202`…), e `session.new` cria uma sessão que fica `busy` por 30 s
e depois volta `done`. Notificações e URLs abertas são só guardadas (`Scenario.Notes()`/`URLs()`).

## Quick-start

Com `nk`, o jeito mais direto é `nk run` (ele já resolve socket/token padrão sozinho):

```sh
nk serve &                        # sobe em unix://$XDG_RUNTIME_DIR/nekuroddo-<uid>.sock

nk run                            # lista os comandos disponíveis (marca "-" = indisponível agora)
nk run sessions.refresh           # roda um comando sem argumento
nk run session.rename name="novo nome" --session abcd1234
nk run session.remove --session abcd1234          # pede confirmação: "remover \"api\"…? [y/N]"
nk run session.remove --session abcd1234 --yes     # pula a pergunta

nk --listen tcp://127.0.0.1:0     # a TUI normal, servindo também por TCP (token novo a cada vez)
nk run --addr tcp://127.0.0.1:PORTA session.open --session abcd1234   # --token lido sozinho do arquivo padrão
```

Sem `nk`, o protocolo cru com `curl`/`websocat`:

```sh
SOCK=$XDG_RUNTIME_DIR/nekuroddo-$(id -u).sock

# hello + comandos disponíveis para uma sessão
curl --unix-socket "$SOCK" http://nk/v1/hello
curl --unix-socket "$SOCK" "http://nk/v1/commands?session_id=abcd1234"

# roda um comando
curl -XPOST --unix-socket "$SOCK" http://nk/v1/commands/sessions.refresh -d '{}'

# acompanha os eventos (Ctrl-C para sair)
curl -N --unix-socket "$SOCK" -H 'Accept: text/event-stream' http://nk/v1/events

# anexa a um painel (precisa de um cliente WebSocket sobre unix socket; websocat
# não fala unix socket nativamente sem socat/nc como ponte — para TCP com token:
websocat "ws://127.0.0.1:PORTA/v1/panes/<id>/ws?token=$(cat $XDG_RUNTIME_DIR/nekuroddo-$(id -u).token)"
```

Para `internal/rpc.Client`, o tipo Go que os testes, `nk run` e um futuro `nk --attach` usam, veja `client.go`:
`Dial`, `Hello`, `Snapshot`, `Commands`, `Run`, `Confirm`, `Events` (devolve um `*EventStream` que já faz o
parse de SSE) e `AttachPane`/`PaneConn` (devolve `Frame{Data}` ou `Frame{Control}` em cada `Read`).
