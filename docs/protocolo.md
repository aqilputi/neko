---
type: Reference
title: Protocolo v1 (RPC)
description: HTTP+JSON, SSE e WebSocket servidos por internal/rpc sobre o core — endpoints, erros, eventos e painéis remotos, como implementado hoje.
resource: https://github.com/aqilputi/nekuroddo/tree/main/internal/rpc
tags: [rpc, protocolo, http, sse, websocket, api]
timestamp: 2026-10-16T20:00:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T20:00:00Z }
status: draft
---

# Protocolo v1 (RPC)

`internal/rpc` serve um `*app.App` pelo protocolo v1 do [plano](plano-arquitetura.md#3-protocolo-do-core-v1):
HTTP+JSON para comandos e consultas, SSE para o barramento de eventos e um WebSocket por painel para os bytes
crus do PTY. É um shell como a TUI — traduz pedidos em chamadas de `app` e nunca fala direto com
claude/store/groups/jira/todo — e continua opcional: um `nk` comum (sem `serve` nem `--listen`) nunca sobe um
`rpc.Server`.

**Estado no código:** o pacote (`server.go`, `events.go`, `panes.go`, `client.go`, `proto/proto.go`) tem testes
próprios (meta do WP-3.2 é ≥ 90% de cobertura) e já está ligado de verdade em `cmd/nekuroddo` (WP-3.4):

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

`capabilities` sempre inclui `"panes"` (`proto.CapPanes`); `"lua"` (`proto.CapLua`) e `"demo"` (`proto.CapDemo`)
só aparecem se o host passar `Options.Capabilities` — hoje nada os liga (a fase 2 wire `lua`, e `--demo` é o
WP-5.2). `version` é `Options.Version` (a versão do binário, em texto livre); `pid` é o do processo do
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
| 400 | `bad_request` | JSON do corpo malformado; `?since=`/`Last-Event-ID` não numérico; `?todo_id=` não numérico; `session_id` faltando em `POST /v1/panes`; tamanho de painel inválido |
| 401 | `unauthorized` | TCP sem `Authorization: Bearer`/`?token=` válido |
| 404 | `not_found` | sessão ou painel desconhecido (`POST /v1/panes`, `.../resize`, `.../ws`, `DELETE`) |
| 404 | `unknown_command` | `POST /v1/commands/{name}` com nome não registrado |
| 404 | `unknown_token` | `POST /v1/confirm/{token}` com token desconhecido ou já consumido |
| 400 | `client_command` | o nome existe mas é `scope: client` (sem `Runner`; roda só no cliente) |
| 409 | `unavailable` | comando existe mas `available(ctx)` nega para essa seleção (ex.: sem sessão sob o cursor) |
| 400 | `missing_arg` | falta um argumento `required` do comando |
| 410 | `expired` | confirmação expirada (60 s) |
| 501 | `not_implemented` | rota da fase 2 (abaixo), ou `POST /v1/panes` sem `PTYSpawner` quando precisaria abrir um painel |
| 422 | `failed` | qualquer outro erro do `app` (fallback) |
| 500 | `failed` | a própria resposta não serializou em JSON (nunca deveria acontecer) |
| 503 | `failed` | `GET /v1/events` enquanto o servidor está fechando |

`proto.CodeConflict` está declarado mas **nenhum handler o emite hoje** — `ErrUnavailable` vira `409` com
`code: "unavailable"`, não `"conflict"` (ver "inconsistências" no relatório final do WP-3.5).

## O que ainda não existe (fase 2)

Declaradas só para dar uma resposta clara em vez de `404` genérico (`m.HandleFunc(p, notImplemented)`):

- `POST /v1/view/tree` — árvore já filtrada/ordenada (`core.BuildTree`), para o GUI não duplicar a lógica.
- `GET /v1/theme` — tokens de cor do tema ativo, para o GUI pintar igual à TUI.
- `POST /v1/lua/eval` — o `:lua <código>` da paleta, por HTTP.

Todas devolvem `501 {"error":"<rota>: fase 2 (ainda não existe)","code":"not_implemented"}`.

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
