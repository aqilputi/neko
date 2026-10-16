---
okf_version: "0.2"
type: Plan
title: Plano — arquitetura alvo, cliente gráfico, Lua e pacotes de trabalho
description: Decisões e roteiro por fases para refatorar o nekuroddo em core headless + TUI + GUI (gatinhos low-poly) + Lua, com pacotes de trabalho paralelizáveis e TDD.
resource: https://github.com/aqilputi/nekuroddo
tags: [plano, arquitetura, gui, lua, tdd, pacotes-de-trabalho, roadmap]
timestamp: 2026-10-16T18:00:00Z
generated: { by: agent:claude-code/claude-fable-5-1, at: 2026-10-16T18:00:00Z }
status: draft
stale_after: 2027-01-16T00:00:00Z
sources:
  - id: pesquisa-arquitetura
    resource: "pesquisa interna de 2026-10-16 (architecture)"
    title: Pesquisa — arquitetura alvo (core funcional, portas e adaptadores, servidor local à la crush)
  - id: pesquisa-bubbletea-gui
    resource: "pesquisa interna de 2026-10-16 (bubbletea-gui)"
    title: Pesquisa — reaproveitar Bubble Tea/PTY num GUI (core headless + socket, modo controle do tmux)
  - id: pesquisa-servo-stacks
    resource: "pesquisa interna de 2026-10-16 (servo-and-stacks)"
    title: Pesquisa — Servo em 2026 e comparação de stacks (Wails v3, Tauri, Godot, Bevy)
  - id: pesquisa-ui
    resource: "pesquisa interna de 2026-10-16 (ui-references)"
    title: Pesquisa — referências de UX fofa/low-poly, oneko, toon/matcap, render sob demanda
  - id: pesquisa-lua
    resource: "pesquisa interna de 2026-10-16 (lua)"
    title: Pesquisa — gopher-lua, API nk.*, orçamentos, reload, TDD
  - id: crush
    resource: https://github.com/charmbracelet/crush
    title: charmbracelet/crush — internal/{app,server,client,proto,pubsub}; HTTP sobre Unix socket
  - id: go-layout
    resource: https://go.dev/doc/modules/layout
    title: Go — organização de módulos (estrutura só quando há necessidade concreta)
  - id: tmux-control-mode
    resource: https://github.com/tmux/tmux/wiki/Control-Mode
    title: tmux -CC — servidor dono do estado, clientes programáticos
  - id: wails
    resource: https://wails.io/docs/howdoesitwork/
    title: Wails — Go + webview nativo (v3 beta, GTK4 + WebKitGTK 6.0 no Linux)
  - id: servo-010
    resource: https://servo.org/blog/2026/04/13/servo-0.1.0-release/
    title: Servo 0.1.0 no crates.io (embedding ainda "work in progress")
  - id: servo-xterm
    resource: https://github.com/servo/servo/issues/30822
    title: Servo — suporte ao xterm.js (WebGL2 e foco ainda instáveis)
  - id: ghostty-web
    resource: https://github.com/coder/ghostty-web
    title: ghostty-web — parser VT do Ghostty em WASM, API compatível com xterm.js
  - id: xterm-webgl
    resource: https://github.com/xtermjs/xterm.js/pull/1790
    title: xterm.js — renderizador WebGL
  - id: r3f-demand
    resource: https://r3f.docs.pmnd.rs/advanced/scaling-performance
    title: Render sob demanda (frameloop="demand")
  - id: reduced-motion
    resource: https://web.dev/articles/prefers-reduced-motion
    title: prefers-reduced-motion
  - id: gopher-lua
    resource: https://github.com/yuin/gopher-lua
    title: yuin/gopher-lua — Lua 5.1 em Go puro, cancelamento por contexto a cada instrução
  - id: neovim-lua
    resource: https://neovim.io/doc/user/lua-guide.html
    title: Neovim — comandos, keymaps e autocmds em Lua
  - id: wezterm-on
    resource: https://wezterm.org/config/lua/wezterm/on.html
    title: WezTerm — eventos nomeados e reload com última configuração boa
  - id: mpv-lua
    resource: https://mpv.io/manual/master/#lua-scripting
    title: mpv — bindings por nome, estado Lua por script
  - id: oneko
    resource: https://en.wikipedia.org/wiki/Neko_(software)
    title: Neko/oneko — o gato de desktop original
  - id: bruno-simon
    resource: https://medium.com/@bruno_simon/bruno-simon-portfolio-case-study-960402cc259b
    title: Bruno Simon — low-poly leve, textura-paleta, DPR limitado
  - id: toon
    resource: https://sbcode.net/threejs/meshtoonmaterial/
    title: three.js MeshToonMaterial
---

# Plano — arquitetura alvo, cliente gráfico, Lua e pacotes de trabalho

Este plano transforma o nekuroddo (hoje um único programa Bubble Tea em `internal/ui`, 97,8 % de cobertura,
gate ≥ 80 %) em **um core headless com dois clientes** (a TUI atual e um GUI com gatinhos low-poly), ambos
extensíveis em Lua, sem perder nenhum atalho e sem que os testes fiquem vermelhos em nenhum passo. O trabalho é
dividido em pacotes (WP) com dono de arquivos explícito, para que 4–6 agents trabalhem em paralelo sem conflito.

Os requisitos do usuário, em ordem: (1) refatorar para a arquitetura certa; (2) GUI moderno, rápido e leve
que embuta os terminais reais do Claude Code e mantenha **todos** os atalhos; (3) cada agent/subagent é
carregado por um gatinho; um gato caminha pela base da tela trazendo o que chega na inbox (clique ou
`shift+space`); 3D low-poly, janelas arrastáveis, **leve**, interface marcada e com poucos detalhes;
(4) extensível em Lua, estilo Emacs, na TUI e no GUI; (5) tudo por TDD com cobertura alta; (6) execução por
um time pequeno de agents em paralelo.

## 1. Decisões

### 1.1 Arquitetura: core funcional + shell imperativo, exposto por portas, servido localmente

**Decisão.** Manter a Elm Architecture da TUI, mas tirar de `ui.Model` o papel de domínio: as regras puras
(árvore, ordem congelada, inbox, labels, cues de ✉, resolução de opção numerada) vão para `internal/core`;
os casos de uso com efeitos (arquivar, disparar, abrir painel, grupos, Jira, todos, transições → notificação)
vão para `internal/app`, que fala com o mundo só por **portas** (interfaces) implementadas pelos pacotes que já
existem (`claude`, `store`, `term`, `groups`, `events`, `jira`, `todo`). Um **registro único de comandos**
nomeados e um **barramento de eventos** em `app` são a face que a TUI, o GUI, o Lua e o servidor usam, sempre
a mesma. O servidor (`internal/rpc`, HTTP+SSE+WS sobre Unix socket) é **opcional**: `nk` sozinho continua um
processo só, sem socket, como hoje.

**Por quê.** É o que `charmbracelet/crush` — a referência que o `docs/arquitetura.md` já cita — fez no mesmo
ecossistema (`internal/{app,server,client,proto,pubsub}`), e o que tmux/Neovim fazem no nível de protocolo. O
código já está meio pronto: `store`, `groups`, `events`, `jira`, `todo`, `claude` não importam Bubble Tea; só
`ui.Model` (model.go:97–187) mistura tudo, e `keyNormal` (update.go:408–625) decide **e** executa.

**Rejeitado.** Reescrever a TUI (perde 4,7 mil linhas de testes que dirigem o `Model` por teclas). Mover os
adaptadores para `internal/adapter/*` só para "parecer hexagonal" (quebra fixtures `NEKO_CLAUDE` e os docs,
sem ganho; o guia de layout do Go desaconselha). Crux/FFI (não precisamos de serialização entre linguagens).

### 1.2 Stack do GUI: frontend web (three.js + xterm.js) num shell Wails v3, frontend agnóstico ao shell

**Decisão.** O GUI é um **frontend web em TypeScript** (Vite + Preact/signals para o chrome DOM, three.js
para a cena, xterm.js com addon WebGL para os painéis) que fala **só o protocolo do core** (§3). Ele roda
dentro de um binário Go separado, `cmd/nekuroddo-gui` (`nkg`), feito com **Wails v3** (GTK4 + WebKitGTK 6.0
no Linux), que embute o core no mesmo processo ou se conecta a um `nk serve` existente. O frontend usa
**uma única** chamada do shell (`Endpoint()` → URL + token do servidor local); todo o resto é HTTP/SSE/WS. Em
modo de desenvolvimento e nos testes Playwright, o mesmo frontend roda num Chromium comum contra
`nk serve --demo`.

**Por quê, contra os requisitos.** *Leve:* sem Chromium nem Node embutidos; binário Go + webview do sistema;
render sob demanda deixa a CPU ociosa em ~0 (§4.7). *Fofo e 3D:* three.js com `MeshToonMaterial` e gatos
procedurais cobre "low-poly marcado" sem engine de jogo. *Terminais reais:* xterm.js é o caminho de produção
(VS Code) e recebe os bytes crus do PTY pelo WS; todos os atalhos vêm do mesmo keymap do core. *Core em Go:*
Wails é Go de ponta a ponta — o GUI pode ser `app.New(...)` no mesmo processo, sem IPC. *Wayland/CachyOS:*
WebKitGTK renderiza WebGL bem em Wayland (os bugs documentados são em X11 e NVIDIA/DMABuf; há fallback,
§7). *Construível e testável por agents:* TypeScript + Vitest + Playwright é o que agents de código mais
dominam; cena e reducers são funções puras testáveis sem GPU.

**Rejeitado e por quê.** *Servo/Verso:* a crate `servo` 0.1.0 saiu em abr/2026, mas o próprio Servo Book
chama o embedding de "work in progress"; xterm.js roda parcialmente (WebGL2 "quebrou de novo", foco/input com
problemas, issue #30822) — desqualificante para um terminal usado todo dia. Revisitar em 2027. *Godot +
GDExtension (Go via graphics.gd, Godotty):* o 3D mais bonito, mas a ponte Go↔Godot é a peça menos madura,
exige editor e `.tscn`, e a lógica de orquestração teria de atravessar uma API nova; agents produzem cenas
Godot mal e os testes headless são fracos. Fica como plano B se o WebGL do WebKitGTK falhar na prática.
*Bevy/egui + libghostty:* leve e controlado, mas joga o time em Rust + API instável do libghostty-vt só para o
shell. *Tauri v2:* mesma WebKitGTK, mas adiciona Rust só para a casca; como o frontend é agnóstico ao shell,
Tauri continua sendo a troca de um pacote se o Wails v3 beta decepcionar (§7). *Electron/CEF:* 90–250 MB e
120–300 MB de RAM em idle contrariam "leve".

### 1.3 Transporte: HTTP+JSON para comandos, SSE para eventos, WebSocket binário só para bytes de PTY

**Decisão.** `internal/rpc` serve um `http.Server` em `unix://$XDG_RUNTIME_DIR/nekuroddo-<uid>.sock`
(permissão 0600) e, para o webview (que não alcança Unix sockets), em `tcp://127.0.0.1:0` com **bearer token**
por lançamento. Comandos e consultas são `POST /v1/commands/{name}` e `GET /v1/...`; eventos são SSE com
`id:` sequencial (retomada por `Last-Event-ID`); cada painel aberto tem um WS com frames binários (entrada e
saída crus) e frames de texto JSON para controle (resize, título, saída). Versão no prefixo `/v1` e em
`GET /v1/hello`; dentro de v1 só mudanças aditivas.

**Por quê.** É curl-ável, testável com `httptest`, e é o que o crush faz. SSE basta para eventos (unidirecional,
reconecta sozinho). WS fica só onde há bytes binários bidirecionais de baixa latência. **A TUI, por padrão,
fala com `app` em processo** (sem serialização); o socket só liga com `nk --listen` ou `nk serve`.

**Rejeitado.** JSON-RPC num WS único (mais estado, menos observável), gRPC (codegen pesado para um dev),
msgpack-rpc (ecossistema só Neovim), modo controle do tmux textual (bom como precedente, ruim como wire).

### 1.4 Runtime Lua: yuin/gopher-lua, uma VM no core, orçamento por chamada

**Decisão.** `internal/lua` com **gopher-lua** (Lua 5.1 + goto, Go puro, sem cgo). Uma única `LState` no
core, serializada por goroutine, com `L.SetContext(ctx)` e `context.WithTimeout` em **toda** entrada Go→Lua
(verificado: gopher-lua checa `ctx.Done()` a cada instrução). Orçamentos: 50 ms para callbacks de evento e
keymap; 5 s (configurável) para comandos invocados pela paleta. Lua registra comandos **no mesmo registro**
que a TUI e o GUI usam, e assina **o mesmo barramento** de eventos; por isso um `init.lua` vale para os dois
clientes. `nk.ui.*` é capacidade (`nk.ui.has("cat")`), roteada por evento ao(s) cliente(s) conectado(s).

**Rejeitado.** Shopify/go-lua (sem corrotinas, sem cancelamento por contexto), aarzilli/golua (cgo;
`pcall` vira pânico quando chama Go), arnodel/golua (5.5 e quotas excelentes, mas "alpha" e API em
movimento — candidato a v2), Starlark (estado congelado), Yaegi (sem kill switch de CPU), Risor v2 (escopo
estreito). Lua nos hooks `nk hook`/`nk hook-context`: **nunca** (contrato de ms e exit 0).

## 2. Arquitetura alvo

### 2.1 Árvore de pacotes

```
cmd/nekuroddo/           nk: TUI (em processo), nk hook, nk hook-context, nk serve, nk run <cmd>, nk --listen
cmd/nekuroddo-gui/       nkg: shell Wails v3; embute o frontend (gui/web/dist) e o core, ou conecta num nk serve
cmd/vtprobe/             inalterado (diagnóstico do painel)
gui/web/                 frontend TypeScript: src/{client,keymap,windows,scene,inbox,chrome}, tests Vitest/Playwright
internal/core/           domínio puro: tipos, regras, árvore, inbox, comando/keymap/evento como dados. Sem I/O.
internal/app/            casos de uso + registro de comandos + pubsub + confirmações por token. Fala só com portas.
internal/app/apptest/    fakes em memória de todas as portas (usados pelos testes e por `nk serve --demo`)
internal/wire/           raiz de composição: adaptadores finos que ligam claude/store/term/... às portas de app
internal/rpc/            server.go (unix+tcp+token), events.go (SSE), panes.go (WS), client.go, proto/ (DTOs)
internal/lua/            runtime.go (LState, callLua, orçamentos), bridge.go (nk.*), loader.go (init.lua, plugins/, reload)
internal/tui/            a `internal/ui` de hoje, afinada: teclas → comandos, eventos → redraw, estado só de apresentação
internal/term/           Process (PTY + fan-out de bytes, sem Bubble Tea) + Screen (x/vt + Draw + teclas, usado pela TUI)
internal/guihost/        o pouco de Go do GUI: sobe app+rpc em loopback com token, ou faz proxy unix→loopback
internal/{claude,store,groups,events,jira,todo,config,paths,theme,cat}   inalterados: já são adaptadores por papel
internal/archtest/       teste que falha se a regra de dependência (abaixo) for violada
```

### 2.2 Regra de dependência

```
            cmd/nekuroddo  cmd/nekuroddo-gui          (compõem tudo via internal/wire)
                  │              │
      ┌───────────┼──────────────┼────────────┐
      ▼           ▼              ▼            ▼
 internal/tui  internal/rpc  internal/lua  internal/guihost      ← shells: traduzem teclas/HTTP/Lua em comandos
      │           │              │
      └─────┬─────┴──────────────┘
            ▼
      internal/app  ──► portas (interfaces) ◄── internal/wire ──► claude, store, term, groups, events, jira, todo
            │
            ▼
      internal/core  ──► só stdlib + tipos de dados de store/claude/groups/events/jira/todo (sem chamar funções deles)
```

Proibido (verificado por `internal/archtest`): `core` e `app` importarem `bubbletea`, `bubbles`, `lipgloss`,
`ultraviolet`, `x/vt`, `creack/pty`, `os/exec`, `net`, `net/http`, `internal/term`, `internal/tui`,
`internal/rpc`, `internal/lua`; `tui`, `rpc`, `lua` importarem `claude`, `store`(exceto tipos), `groups`,
`events`, `jira`, `todo` diretamente para chamar funções. `tui` pode importar `term` (a metade Screen é
renderização de terminal, naturalmente da TUI). `internal/paths` continua sem imports internos.

### 2.3 Portas (`internal/app/ports.go`)

```go
type Clock interface{ Now() time.Time }

type ClaudeCLI interface {
	ListAgents() ([]claude.Agent, error)
	Dispatch(dir, name, prompt string, extra ...string) (shortID string, err error)
	ResumeInBackground(dir, sessionID string) (string, error)
	Stop(id string) error
	Remove(id string) error
	AttachArgv(id string) []string
	FetchUsage() (claude.UsageReport, error)
}

type SessionRepository interface { // *store.Store already satisfies this
	Refresh() store.Snapshot
	Reapply(sessions []store.Session)
	Archive(ids ...string)
	Unarchive(ids ...string)
	Rename(sessionID, name string)
	SetContext(sessionID, ctx string)
	RenameContext(shown, name string)
	MarkSeen(sessionID string, at time.Time)
	SetWindowStart(t time.Time)
}

type GroupStore interface {
	Load() groups.File
	Save(groups.File) error
	Queue(sessionID string, n groups.Note) error
}

type EventLog interface{ Poll() []events.Event } // *events.Tail already satisfies this

type PTYSpawner interface {
	Start(id, title, dir string, cols, rows int, argv ...string) (Pane, error)
}

// Pane is the framework-free half of a terminal pane (term.Process).
type Pane interface {
	ID() string
	Write(p []byte) (int, error)             // keys/paste, already encoded
	Resize(cols, rows int)
	Output() <-chan struct{}                 // coalesced "new output" signal (local emulator)
	Subscribe() (bytes <-chan []byte, cancel func()) // raw PTY bytes, for remote clients
	Exited() <-chan error
	Done() bool
	Title() string
	Close()
}

type Notifier interface{ Notify(title, body string) error } // notify-send today; GUI toast tomorrow
type Opener interface{ OpenURL(u string) error }            // xdg-open

type JiraClient interface {
	Fetch(cfg jira.Config) (jira.Cache, error)
	Create(cfg jira.Config, n jira.NewIssue) (key string, cost float64, err error)
}

type TodoClient interface {
	Available() bool
	List() ([]todo.Group, error)
	Start(id int) error
	Done(id int) error
	LinkJira(id int, key string) error
	AddFromJira(key, summary string) error
	AddNote(id int, text string) error
}

type Ports struct {
	Clock Clock; Claude ClaudeCLI; Sessions SessionRepository; Groups GroupStore; Events EventLog
	Panes PTYSpawner; Notify Notifier; Open Opener; Jira JiraClient; Todo TodoClient
}
```

`Clock` entra só onde o tempo real atrapalha os testes (cues de ✉ em `ui/groups.go:25–27`, timers do gato,
TTL de confirmação); `store.go` continua com `time.Now()` até haver razão concreta.

### 2.4 Registro de comandos (um só para keymap, paleta, Lua, RPC e GUI)

```go
// internal/core/command.go
type Arg struct{ Name, Type, Prompt string; Required bool } // Type: "string"|"session"|"dir"|"tag"|"int"
type Command struct {
	Name    string   // "session.archive"
	Title   string   // palette/help text, pt-BR ("arquiva/restaura (no contexto: todas)")
	Hint    string
	Keys    []string // default binding(s); "" = palette only
	Mode    string   // "normal" | "insert" | "input" | "*"
	Args    []Arg
	Scope   string   // "core" (runs in app) | "client" (nav/fold/zoom: the client handles it, but it is still listed)
	Confirm bool     // may return a PendingConfirmation
	Source  string   // "builtin" | "lua:<file>"
}

// internal/app/registry.go
type Result struct {
	Toast   string; Err bool
	Pending *core.PendingConfirmation // {Token, Question, ExpiresAt}
	Exec    *core.ExecRequest         // {Argv, Dir}: the client takes over the terminal (fullscreen)
	Data    any                       // command-specific JSON-able payload
}
type Registry struct{ /* name → Command + runner; dynamic lookup on every call (Lua may wrap later) */ }
func (r *Registry) Register(c core.Command, run func(ctx Context, args Args) (Result, error)) error
func (r *Registry) Run(ctx Context, name string, args Args) (Result, error)
func (r *Registry) List(ctx Context) []core.Command   // Available() filtered; feeds palette/help/keymap/RPC
```

Nomes (v1): `session.open`, `session.preview`, `session.fullscreen`, `session.insert`, `session.reply`,
`session.new`, `session.stop`, `session.remove`, `session.archive`, `session.rename`, `session.move`,
`sessions.archive_stale`, `sessions.remove_ended`, `sessions.refresh`, `group.add`, `group.remove`,
`group.notify`, `group.dissolve`, `todo.dispatch`, `todo.start`, `todo.done`, `jira.toggle`, `jira.fetch`,
`jira.open_url`, `pane.close`, `pane.zoom`, `usage.show`, `inbox.summon`, `app.quit`; e os de escopo
`client`: `nav.up/down/top/bottom/half_up/half_down/page_up/page_down`, `nav.collapse/expand`, `nav.pick`,
`nav.next_attention/prev_attention`, `tab.sessions/inbox/todos/archive/next/prev`, `view.filter`,
`view.show_all`, `view.sort`, `view.theme`, `view.flat` (só o GUI: cena ↔ lista plana), `palette.open`, `switcher.open`, `help.open`. A `Context` que
cada comando recebe traz a **seleção do cliente** (`SessionID`, `Picked []string`, `Tab`, `TodoID`,
`IssueKey`) — o core nunca sabe onde está o cursor de ninguém.

### 2.5 Barramento de eventos (`internal/app/pubsub.go`)

`Broker{Publish(core.Event); Subscribe(ctx) <-chan core.Event}` com buffer 256 por assinante e corte do
assinante lento (nunca bloqueia o publicador). Cada `Event{Seq uint64, At, Name string, Data any}`. Nomes:
`snapshot.changed`, `session.state {id, prev, state}`, `session.attention {id, attention}`,
`session.notification {id, type, message}`, `peer.message {to, from, from_id, text}`, `mail.cue {id, sent}`,
`group.changed {tag}`, `toast {text, err}`, `confirm.requested {token, question}`, `confirm.resolved`,
`pane.opened|closed|exited|title`, `usage.updated`, `todos.changed`, `jira.changed`, `lua.error`,
`command.ran {name}`. A TUI consome com um `tea.Cmd` que espera no canal e devolve `appEventMsg` (o mesmo
padrão de `p.Listen()` hoje); o RPC reexpõe como SSE; o Lua em `nk.on`.

### 2.6 Confirmações entre clientes (token)

`confirmThen(q, func(*Model) tea.Cmd)` (dialog.go:80) é um closure sobre o `*Model` e não atravessa um socket.
Em `app`: um comando com consequências devolve `Result{Pending: &PendingConfirmation{Token, Question,
ExpiresAt}}` e guarda a ação em `app.pending` (token aleatório, uso único, TTL 60 s pelo `Clock`);
`app.Confirm(token) (Result, error)` executa, `app.Cancel(token)` descarta; `confirm.requested` é publicado
para os outros clientes mostrarem o mesmo diálogo. Na TUI, `confirmDialog` passa a guardar o token e `y`
chama `app.Confirm`. No RPC: `POST /v1/confirm/{token} {"yes": true}`. No Lua: `nk.ui.confirm(q, fn)` cria
um pendente cujo "sim" chama `fn`.

### 2.7 Keymap servido ao GUI

`keys.go` deixa de ser a fonte e passa a ser **derivado**: cada `core.Command` traz `Keys`/`Mode`, o core
monta `core.Keymap` (lista ordenada de `Binding{Command, Keys, Mode, Help{Key, Desc}}`), e a TUI constrói os
`key.Binding` a partir dela (mantendo `barLabels` e `shortHelp` como apresentação). `GET /v1/keymap` serve a
mesma lista; o GUI resolve `KeyboardEvent → nome do comando` com a mesma tabela e chama
`POST /v1/commands/{name}`. Lua pode sobrepor (`nk.keymap.set`) e o `/v1/keymap` reflete a sobreposição. A
paleta (`paletteItems()`, dialog.go:221–304) passa a enumerar `Registry.List()` em vez de uma lista à mão —
o que faz comandos Lua aparecerem sozinhos nos dois clientes.

## 3. Protocolo do core (v1)

| método e caminho | corpo / resposta |
|---|---|
| `GET /v1/hello` | `{app:"nekuroddo", version, protocol:1, capabilities:["panes","lua","demo"], pid}` |
| `GET /v1/snapshot` | `{sessions:[store.Session…], inbox:[ids], groups:{tag:[members]}, mail:{id:{at,sent}}, usage, todos, jira, at}` |
| `GET /v1/events?since=<seq>` | SSE: `id:<seq>` `event:<name>` `data:<json>`; retoma por `Last-Event-ID` (buffer de 1024 eventos) |
| `GET /v1/commands` | `[{name,title,hint,keys,mode,args,scope,confirm,available}]` |
| `POST /v1/commands/{name}` | `{args:{…}, context:{session_id, picked, tab, todo_id, issue_key}}` → `Result` (toast, pending, exec, data) |
| `POST /v1/confirm/{token}` | `{yes:true\|false}` → `Result` |
| `GET /v1/keymap` | `[{command, keys, mode, help:{key,desc}}]` |
| `POST /v1/view/tree` | `{state:{tab, filter, show_all, sort, order, collapsed}}` → `{rows, state}` — `core.BuildTree` puro, sem estado no servidor |
| `GET /v1/theme` | tokens da paleta ativa e lista de temas (o GUI pinta igual à TUI) |
| `GET /v1/panes` · `POST /v1/panes` | lista · `{session_id, cols, rows, reply?}` → `{pane_id}` (mesma decisão resume/attach de `openPane`) |
| `WS /v1/panes/{id}/ws` | binário cliente→servidor: bytes para o PTY; binário servidor→cliente: saída crua; texto: `{type:"resize",cols,rows}` `{type:"title",text}` `{type:"exit",err}` |
| `POST /v1/panes/{id}/resize` · `DELETE /v1/panes/{id}` | redimensiona (última escrita vence; `pane.resized` avisa) · fecha (SIGHUP, como hoje) |
| `POST /v1/lua/eval` | `{code}` → `{result\|error}`; mesmo orçamento do `:lua` |

Autenticação: Unix socket por permissão de arquivo; TCP loopback exige `Authorization: Bearer <token>`
(WS: `?token=`), token gravado 0600 em `$XDG_RUNTIME_DIR/nekuroddo-<uid>.token`. Versionamento: `/v1` +
`hello.protocol`; campos novos são opcionais; clientes ignoram o que não conhecem; quebra → `/v2` convivendo.

## 4. GUI — cena e UX

### 4.1 Layout

Uma janela, três camadas: (a) **cena WebGL** ao fundo, uma prateleira/escrivaninha rasa em low-poly, câmera
fixa levemente isométrica (Monument Valley, não mundo aberto); (b) **janelas DOM** dos terminais (xterm.js)
flutuando por cima, arrastáveis, com sombra e "levantada" (2° de `rotateX`) enquanto se arrasta; (c) **chrome**
em DOM: topo com contagens e barras de uso (iguais ao `header()` da TUI), coluna esquerda recolhível com a
mesma árvore contexto → sessão → subagents (linhas vindas de `/v1/view/tree`), barra de status embaixo com
`NORMAL`/`INSERT` e as dicas do keymap. Uma faixa de ~72 px na base é a **calçada**: janelas não entram
nela; é por onde o gato da inbox anda. Poucas cores (tokens do tema via `/v1/theme`), bordas grossas,
ícones grandes, nada de texto pequeno além dos terminais.

### 4.2 Gatos: estado → animação (mesmo vocabulário de `store.Session.State`/`Attention`)

| estado | gato | pista secundária |
|---|---|---|
| `busy` | sentado "digitando", patinhas batendo, orelhas em pé | brilho fino sob as patas na cor `Busy` |
| `blocked` / `Attention != None` | senta, rabo bate, balão `!` acima | balão na cor `Blocked`; dispara o gato da inbox |
| `done` / `Finished` | enrola e dorme (respiração lenta, animação mais barata) | `zZ` como na TUI |
| `idle`/`stale` | sentado quieto, pisca de vez em quando | coleira apagada |
| erro (`Err`, `claude rm` falhou) | pelo arrepia (pop de escala no rabo), volta ao estado anterior | flash vermelho único, sem loop |
| `peer.message` (✉) | remetente faz gesto de "jogar"; um envelope percorre um arco até o destinatário | mesmo tempo do `mailCueFor` (90 s piscando, 15 min apagado) |

**Subagents** são **filhotes** (`InstancedMesh`, mesma geometria em escala 0,5) ao lado do pai; terminados
somem (ou ficam com `H`/`view.show_all`). **Grupos** são **coleiras** na cor de `core.GroupColorIndex(tag)` (o
hash fnv de `ui/groups.go:45`, movido para o core para TUI e GUI concordarem); os pares da sessão focada ganham
a coleira acesa (equivalente ao `┃`). Clicar num gato = `session.open`; passar o mouse mostra o label.

### 4.3 O gato da inbox (oneko)

Quando entra um item na inbox (`session.attention` para `blocked`/`returned`, ou `peer.message`), um gato
entra pela esquerda da calçada carregando o ícone do que traz (`!` ou ✉) e caminha até parar sob a janela
ou gato da sessão; itens seguintes entram em fila indiana (um só clip de caminhada instanciado, com
defasagem). **Clique** nele → `session.open {id}` da sessão que ele carrega. **`shift+space`** → `inbox.summon`
(o core escolhe a mais urgente por `core.SortInbox`, desempate por atividade, e abre focada). Fora da janela,
`shift+space` global no Wayland depende de portal; a solução honesta é `nk run inbox.summon` ligado ao atalho
do compositor (Hyprland/KDE), que fala com o core pelo socket. Com `prefers-reduced-motion`, o gato não anda:
aparece um badge estático no canto da calçada com a mesma informação.

### 4.4 Janelas de terminal

Cada janela = um `POST /v1/panes` + WS. xterm.js com `@xterm/addon-webgl` (fallback DOM se o contexto WebGL
falhar), `ghostty-web` como troca futura (API compatível). Foco numa janela = INSERT: teclas vão ao PTY
exceto `ctrl+]` (volta) e `ctrl+k` (switcher), espelhando `key()` em update.go:343–371. Arrastar pela barra de
título (limitado à área acima da calçada), `z` alterna zoom (janela ocupa a cena), `w` fecha. Clipboard OSC 52
e título OSC 0/2 ficam a cargo do xterm.js/da janela (no WS os bytes vão crus; o filtro de `internal/term/osc.go`
continua só para o emulador local da TUI).

### 4.5 Teclado em primeiro lugar

O GUI carrega `/v1/keymap` ao iniciar; `KeyboardEvent` → nome do comando → `POST /v1/commands`, com a mesma
`Context` (seleção da árvore). Comandos `client` (`nav.*`, `tab.*`, `view.*`, `palette.open`) são executados
pelo frontend sobre a sua árvore. A paleta `:` do GUI lista `/v1/commands` + "ir: sessão" + temas, com o mesmo
fuzzy. Nada é só de mouse: arrastar janela tem `pane.move` por setas; clicar no gato é `session.open`.

### 4.6 Fallbacks

`prefers-reduced-motion`: poses estáticas, sem respiração, gato da inbox vira badge. **Modo 2D** (comando
`view.flat`): troca a cena por uma lista plana com o gato em texto (`ᓚᘏᗢ`) e as pílulas da TUI; tudo continua
funcionando. Se o WebGL do WebKitGTK falhar (NVIDIA/X11), o shell relança com
`WEBKIT_DISABLE_DMABUF_RENDERER=1` e, persistindo, abre em modo 2D com aviso.

### 4.7 Orçamento de desempenho

- **CPU ociosa ≈ 0:** render sob demanda — `requestAnimationFrame` só roda enquanto houver `invalidate()`
  pendente (evento, drag, caminhada, meow). Respiração/rabo capados a 10 fps; 60 fps só em drag/caminhada.
  `visibilitychange: hidden` para tudo. DPR ≤ 1,5. Uma luz direcional + hemisférica, sem sombras.
- **Memória:** total do `nkg` (Go + WebKitGTK web process) ≤ 250 MB com 3 painéis abertos; Go ≤ 40 MB.
- **Tamanho:** binário `nkg` ≤ 20 MB; bundle do frontend ≤ 1 MB gzip (three.js ~150 KB, xterm ~100 KB).
- **Partida:** primeiro quadro ≤ 1,5 s; snapshot inicial ≤ 300 ms (é o `store.Refresh` de hoje).
- Verificado por `scripts/gui-idle-cpu.sh` (amostra `ps`/`/proc` por 30 s com a janela em foco e parada).

### 4.8 Assets

Primeiro **gatos procedurais**: cabeça esfera, orelhas cones, corpo cápsula, pernas cilindros, rabo em curva;
`MeshToonMaterial` com rampa de 3 bandas; cor da coleira = uniform. Zero licenças, zero download, cor
exatamente a do tema. Caminho de upgrade: base glTF **CC0** (Quaternius, via filtro CC0 do Poly Pizza)
remodelada para gato preto de olhos amarelos, Draco, textura-paleta (Bruno Simon) para N coleiras com um
material. Som (miado curto, um disparo) opcional e desligado por padrão.

### 4.9 Como o GUI se liga ao core

`nkg` (padrão) = `app.New(wire.Default(cfg))` + `rpc.Serve(tcp://127.0.0.1:0, token)` no mesmo processo; a
webview recebe URL+token por `Endpoint()`. `nkg --connect unix://…` = cliente de um `nk serve` (ou de um
`nk --listen` com a TUI aberta ao lado): `internal/guihost` faz um `httputil.ReverseProxy` unix→loopback,
inclusive WS. Dois clientes no mesmo core enxergam as mesmas sessões, painéis e confirmações.

## 5. Lua

### 5.1 Arquivos e carga

`~/.config/nekuroddo/init.lua` (opcional) e `~/.config/nekuroddo/plugins/*.lua` (ordem alfabética), carregados
após o `config.toml`, numa `LState` só. Sem arquivos = comportamento idêntico ao de hoje (contrato "no-op"
coberto pela suíte inteira). Config:

```toml
[lua]
enabled = true
command_timeout_ms = 5000   # orçamento de comandos invocados pela paleta
auto_reload = true          # observa init.lua e plugins/
```

### 5.2 API v1 (`nk.*`)

```lua
nk.command(name, fn, { doc = "...", args = { "dir", "prompt" }, keys = { "ctrl+x" }, mode = "normal" })
nk.run(name, args)                 -- roda QUALQUER comando do registro, builtin ou Lua ("session.archive", ...)
nk.keymap.set(mode, key, cmd)      -- cmd: nome registrado ou função (comando anônimo, fora da paleta)
nk.keymap.del(mode, key)
nk.on(event, fn)  / nk.off(event, fn)   -- eventos do §2.5, payload = tabela só de dados
nk.sessions() / nk.session(id)     -- cópias (store.Session como tabela), nunca referências vivas
nk.inbox()                         -- ids por urgência
nk.dispatch{ dir=, prompt=, name=, perm=, extra={} } -> short_id   -- perm valida contra permModes; bypass só explícito
nk.stop(id) / nk.remove(id)
nk.group.add(tag, ids) / nk.group.remove(tag, ids) / nk.group.tags() / nk.group.of(id)
nk.notify(tag, text)
nk.ui.has(cap)                     -- "toast" | "confirm" | "cat" | "palette" | "scene"
nk.ui.toast(text, is_err) / nk.ui.confirm(question, fn) / nk.ui.palette_open(prefill) / nk.ui.cat.meow(text)
nk.config_dir() / nk.state_dir() / nk.log(level, msg)
```

Comandos Lua entram no registro com `Source = "lua:<arquivo>"` e aparecem na paleta da TUI e do GUI; teclas
Lua entram no keymap servido. `:lua <expr>` na paleta avalia e mostra o resultado (Hammerspoon/`:lua` do
Neovim). Exemplo (`plugins/deploy.lua`):

```lua
nk.command("deploy-all", function()
  local ids = {}
  for _, dir in ipairs({ "~/dev/svc-a", "~/dev/svc-b" }) do
    ids[#ids + 1] = nk.dispatch{ dir = dir, prompt = "Suba em staging e confirme o healthcheck.", perm = "acceptEdits" }
  end
  nk.group.add("deploy", ids)
  nk.ui.toast(("deploy-all: %d agents em #deploy"):format(#ids))
end, { doc = "dispara os deploys e agrupa #deploy", keys = { "ctrl+shift+d" } })

nk.on("peer.message", function(ev)
  if ev.text:find("[#deploy", 1, true) then nk.ui.toast("deploy: " .. ev.from .. ": " .. ev.text) end
end)
```

### 5.3 Orçamentos, isolamento e reload

- Uma função Go, `callLua(fn, args, budget)`, em **toda** entrada: `context.WithTimeout` → `L.SetContext`;
  erro Lua vira `error`; pânico do VM é recuperado ali e nunca chega ao `tea.Program`. 50 ms para `nk.on`/
  keymap; `command_timeout_ms` para comandos; chamadas que disparam processos (`nk.dispatch`) só enfileiram —
  o `exec` acontece fora do orçamento, na porta `ClaudeCLI`.
- Erro → toast `lua: …` + anel de 100 mensagens (`lua.messages`, diálogo "últimos erros lua"); três falhas
  seguidas de callbacks do mesmo arquivo desativam **só** aquele arquivo (`lua.enable <arquivo>` religa).
- **Reload atômico**: `lua.reload` (ou auto) monta registros novos em staging; só troca se todo arquivo
  carregou sem erro; senão mantém o último bom e aponta arquivo:linha (WezTerm).
- Lua nunca roda em `nk hook`/`nk hook-context`; `nk.on` assina o stream já lido pelo processo longo.

## 6. Fases e pacotes de trabalho

### 6.0 Convenções para o time de agents

- Um WP = uma branch `wp/<id>-<slug>` a partir de `main`, um PR, um dono. O dono só toca o **escopo** listado;
  `scripts/wp-scope.sh '<regex>'` (criado em WP-1.1) falha se `git diff --name-only main...` sair dele.
- Ordem **TDD**: primeiro os testes listados (vermelhos), depois o mínimo de código, depois refatoração;
  `make ci` verde antes do PR; cobertura por pacote novo ≥ 90 %; total nunca cai mais de 0,5 pt no PR.
- Um agent **integrador** faz merge na ordem das ondas e roda `make ci` após cada merge; conflitos são
  resolvidos pelo dono rebaseando. Dentro de uma onda, nenhum par de WPs compartilha arquivo.
- `model.go` ganha, em WP-2.0, um bloco `// ---- slice: <nome> ----` por fatia; na onda 2 cada WP só escreve
  no seu bloco (campos novos) e nunca fora dele.
- Comentários de código em inglês, textos de UI e docs em pt-BR, `CHANGELOG` em "Não lançado" por WP.

### Fase 1 — Refatoração (strangler fig; o que dá para executar agora com 4–6 agents)

Objetivo: `ui.Model` deixa de ser domínio; `app`/`core`/portas existem e são usadas; a suíte atual continua
verde o tempo todo; cobertura total ≥ 95 %.

**Onda 1 (paralela: 1.1 ‖ 1.2 ‖ 1.3 ‖ 1.4)**

**WP-1.1 — `internal/core` puro + guarda de arquitetura** · dono: `internal/core/{helpers,tree,inbox,group}.go`
e testes, `internal/archtest/**`, `scripts/wp-scope.sh`
- Testes primeiro: `core/tree_test.go` portando os casos de `ui/behavior_nav_test.go` (layout da árvore,
  ordem congelada, `S` por custo/nome, inbox bloqueada > voltou, cursor segue a linha); `helpers_test.go`
  (`Truncate`, `Clamp`, `PctText`, `OptionCount`, `ResolveOption`, `WindowPct`, `RowKey`,
  `GroupColorIndex`); `archtest/deps_test.go` (`go list -deps -json` e a lista proibida de §2.2).
- Entrega: cópias puras de `rebuild/sortInbox/assignRanks/resetOrder/rowKey` (model.go:326–541) como
  `core.BuildTree(sessions, ViewState) ([]Row, ViewState)`, `core.SortInbox`, e os helpers; `core.Command`,
  `Keymap`, `Event`, `PendingConfirmation`, `ExecRequest` como tipos. A `ui` **não** muda ainda.
- Aceite: `core` ≥ 95 %; archtest verde; `make ci` verde.

**WP-1.2 — `internal/app` esqueleto, portas, pubsub, registro, confirmações, fakes, `internal/wire`** · dono:
`internal/app/**` (exceto as fatias da onda 2), `internal/app/apptest/**`, `internal/wire/**`
- Testes primeiro: `pubsub_test.go` (fan-out, assinante lento cortado sem bloquear, `Seq` monótono);
  `registry_test.go` (registrar, nome duplicado erra, `List` filtra `Available`, `Run` desconhecido erra,
  lookup dinâmico); `confirm_test.go` (token uso único, TTL por `FakeClock`, `Cancel`); `wire_test.go`
  (adaptadores reais contra `NEKO_CLAUDE` fake e `NEKO_TODO_SCRIPT`, um teste por porta).
- Entrega: `ports.go`, `app.go` (`New(cfg, Ports)`, `Snapshot()`, `Refresh()`, `Subscribe`), `pubsub.go`,
  `registry.go`, `confirm.go`, `result.go`; `apptest` com `FakeClaude/FakeSessions/FakeGroups/FakeEvents/
  FakePanes/FakeClock/FakeNotifier/FakeJira/FakeTodo`; `wire.Default(cfg) Ports`.
- Aceite: `app` ≥ 90 %, sem `os/exec`/Bubble Tea (archtest); nada em `ui` chama isso ainda.

**WP-1.3 — `internal/term` sem Bubble Tea no Process** · dono: `internal/term/**`; em `ui`, **só**
`startPane` (model.go:639–658) e os `case term.OutputMsg/term.ExitMsg` (update.go:181–195)
- Testes primeiro: `process_test.go` (`Output()` coalesce ~33 ms, `Exited()` fecha, `Subscribe()` recebe os
  bytes crus de um `bash -c printf`, `Write` chega ao PTY, `Title()`); `stab_test.go`/`live_stab_test.go`
  continuam (são os testes de corrida).
- Entrega: `Pane.Listen() tea.Cmd` (term.go:117–127) removido; `Pane` = `Process` (PTY, fan-out) +
  `Screen` (x/vt, `Draw`, `SendKey`, filtro OSC); `OutputMsg/ExitMsg` viram `paneOutputMsg/paneExitMsg` na `ui`,
  com `listenPane(p) tea.Cmd` em `ui`.
- Aceite: `internal/term` só importa Bubble Tea em `screen.go`/`keys.go`; `-race` verde; `pane_test.go`,
  `follow_test.go` verdes.

**WP-1.4 — Documentação da fase** · dono: `docs/**`, `CONTRIBUTING.md`, `CHANGELOG.md`, `README.md`
- Entrega: este plano em `docs/plano-arquitetura.md` e linha no `docs/index.md`; seção "Arquitetura alvo" e
  regra de dependência em `docs/arquitetura.md`; convenções de WP em `CONTRIBUTING.md`; `docs/log.md`.
- Aceite: links válidos; front matter OKF; sem claims sobre código ainda não mergeado (usar "planejado").

**Onda 2.0 (serial, 1 agent, curta) — WP-2.0 — fatiar `internal/ui` por assunto, sem mudar comportamento** ·
dono: `internal/ui/**`
- Entrega: movimentos puros: `ui/sessions.go` (toggleArchive, archiveStale, removeEnded, removeSelected, Stop,
  ramos `inRename/inMove/inRenameCtx` extraídos como métodos), `ui/pane.go` (openPane…resizePanes,
  fullscreen, listenPane, onPaneOpen, onPaneOutput, onPaneExit), `ui/input.go` (ask, dispatching,
  refreshPrompt, dispatch, submitInput), `ui/todo_jira.go` (jiraAction, jiraDescription, dispatchTodo,
  openURL, onJira*, onTodos), `ui/notify.go` (notice, notifyDesktop, onEvents), `ui/nav.go` (move, collapse,
  expand, toggle, jumpAttention); `case` do `update()` só delega (`return m.onPaneOpen(msg)`); campo
  `app *app.App` em `Model` e `New(cfg)` = `wire.Default(cfg)` + `app.New`; blocos `// ---- slice: X ----`
  em `model.go`; helpers duplicados da `ui` trocados por `core.X` (apagando as cópias).
- Aceite: `go test ./...` idêntico antes/depois; `git diff --stat` mostra só movimentos; cobertura ±0,5.

**Onda 2 (paralela: 2.1 ‖ 2.2 ‖ 2.3 ‖ 2.4 ‖ 2.5 ‖ 2.6)** — cada WP é dono de **um** `ui/<fatia>.go`, dos
testes que lista, de **um** `app/<fatia>.go` + teste, e do seu bloco em `model.go`. Nenhum toca
`update.go`, `view.go`, `keys.go`, `dialog.go` (exceto onde dito).

**WP-2.1 — Grupos** · dono: `ui/groups.go`, `ui/groups_test.go`, `app/groups.go`, `core/group.go`
- Testes primeiro: `app/groups_test.go` com `FakeGroups`+`FakeClock`: `CleanTag`, `GroupTargets` sem alvo
  erra, `Notify` conta paradas, `OnPeer` resolve remetente com sufixo `…`, cue de ✉ 90 s/15 min por `Clock`,
  `SyncGroupNames` salva só quando muda, `Peers(id)`.
- Entrega: lógica de ui/groups.go:53–343 em `app` (`GroupTargets/UngroupTargets/NotifyGroup/OnPeer/
  SyncGroupNames/Peers/Mail()/Messages()`), eventos `group.changed`, `peer.message`, `mail.cue`;
  `groupColor` usa `core.GroupColorIndex`; `ui/groups.go` só traduz (`toastCmd`, `mailMark`, `groupsDetail`).
- Aceite: `ui/groups_test.go` verde só mudando pontos de chamada; `groups` (pacote) intocado.

**WP-2.2 — Sessões: arquivar, parar, remover, renomear, mover** · dono: `ui/sessions.go`, `ui/archive_test.go`,
`ui/rename_test.go`, `app/sessions.go`
- Testes primeiro: `app/sessions_test.go` com `FakeSessions`+`FakeClaude`: `ToggleArchive` (sessão vs contexto
  → `Pending`), `ArchiveStale` só paradas não ocupadas, `RemoveEnded` fecha painéis e tolera falhas parciais,
  `Stop/Remove` pedem confirmação, `Rename/Move/RenameContext` reaplicam e publicam `snapshot.changed`.
- Entrega: model.go:800–900 e update.go:582–646 em `app`, devolvendo `Result` (toast + `Pending`).
- Aceite: testes de ui listados verdes; sem `claude.*` chamado da `ui`.

**WP-2.3 — Disparo e confirmação por token** · dono: `ui/input.go`, `ui/perm_test.go`,
`ui/behavior_input_test.go`, `app/dispatch.go`, `app/confirm.go` (herdado de 1.2), `ui/dialog.go` **linhas 47–82**
- Testes primeiro: `app/dispatch_test.go`: bypass pede confirmação salvo se for o padrão configurado; o modo
  volta ao padrão após disparar; `Confirm(token)` dispara com `--permission-mode`; token expirado erra;
  `dispatched` publica toast e pede refresh; vínculo com todo (`Start`+`AddNote`).
- Entrega: `confirmThen(q, func)` continua existindo para closures locais, e ganha `confirmToken(q, token)`;
  `dispatch` (model.go:753–770) vira `app.Dispatch(DispatchReq)`.
- Aceite: `perm_test.go` verde (os argv do fake `claude` continuam iguais).

**WP-2.4 — Painéis** · dono: `ui/pane.go`, `ui/pane_test.go`, `ui/follow_test.go`, `ui/switcher_test.go`,
`app/panes.go`
- Testes primeiro: `app/panes_test.go` com `FakePanes`: abrir duas vezes reaproveita; sessão `history` resume
  via `ResumeInBackground`; `interactive` erra com a mensagem de hoje; resposta pendente é digitada após
  pronto (50 ms / 1,5 s por `Clock`); `Close` limpa `paneOf`; `core.ActivePane(sel, panes, mode, zoom)` puro.
- Entrega: `PaneManager` em `app` (`Open/Close/Resize/Active/Pending`), eventos `pane.*`; `fullscreen`
  devolve `Result{Exec}` e a `ui` faz o `tea.ExecProcess`; `ui/pane.go` só desenha e encaminha teclas.
- Aceite: testes listados verdes; `app` não importa `term` (archtest).

**WP-2.5 — Jira e todos** · dono: `ui/todo_jira.go`, `ui/render_todo_test.go`, `app/jira.go`, `app/todo.go`
- Testes primeiro: `app/jira_test.go`/`todo_test.go` com fakes: issue → todo; todo com issue já ligada erra;
  criar issue pede confirmação e liga com `LinkJira`; `DeniedError` gera dica `approve_hint`; `DispatchTodo`
  escolhe `RepoFor`; `Start/Done` recarregam.
- Entrega: update.go:794–875 em `app`; `openURL` via porta `Opener`.
- Aceite: testes listados verdes; `todo.*`/`jira.*` não são chamados da `ui`.

**WP-2.6 — Transições, notificação e eventos de hook** · dono: `ui/notify.go`, `ui/behavior_msgs_test.go`,
`app/watch.go`
- Testes primeiro: `app/watch_test.go`: `blocked`/`done`/`idle` após `busy` geram `session.state` e um aviso
  agregado ("N agents voltaram"); com foco não chama `Notifier`; `OnHookEvents` publica
  `session.notification` e `peer.message` e pede refresh (coalescido se já refrescando).
- Entrega: `notice/notifyDesktop/onEvents` (model.go:567–600, update.go:304–333) em `app`; `app.SetFocused`;
  a `ui` só mia (`cat.Meow`) e toca o sino ao receber os eventos.
- Aceite: testes listados verdes; nenhum `exec.Command` em `ui`.

**Onda 2.7 (serial) — WP-2.7 — limpeza** · dono: `internal/ui/model.go`, `internal/ui/update.go`
- Entrega: remove de `Model` os espelhos que agora vivem em `app` (`grp`, `mail`, `msgs`, `prevState`,
  `events`, `evTail`, `pending`), `snapMsg` passa a vir de `snapshot.changed`; `docs/arquitetura.md` tabela.
- Aceite: `make ci`; `ui` importa só `app`, `core`, `theme`, `cat`, `term`, `config` (archtest estendido).

**Onda 3 (paralela: 3.1 ‖ 3.2 ‖ 3.3 ‖ 3.5; 3.4 após 3.2)**

**WP-3.1 — Registro de comandos substitui o switch** · dono: `ui/update.go`, `ui/keys.go`, `ui/dialog.go`
(paleta), `ui/keys_test.go`, `ui/helpkey_test.go`, `app/commands.go`, `docs/teclas.md`
- Testes primeiro: `app/commands_test.go` (todo nome do §2.4 registrado, `Keys` sem colisão por modo, cada
  comando `core` roda com `Context` fake); `ui/keys_test.go` ganha "cada tecla de `keys.go` resolve para um
  nome" antes de qualquer migração.
- Entrega: **um `case` por commit**, começando por `session.archive` (já em `app`): registrar, apontar a
  tecla, apagar o `case`, rodar `keys_test/helpkey_test/switcher_test/behavior_nav_test`, commit; ao fim,
  `keyMap` derivado de `core.Keymap` e `paletteItems()` enumerando `Registry.List()`.
- Aceite: `docs/teclas.md` com coluna "comando"; help e barra idênticos (testes de render).

**WP-3.2 — `internal/rpc`** · dono: `internal/rpc/**`
- Testes primeiro: servidor real em socket curto (`os.MkdirTemp("/tmp", "nk")` — limite de 108 bytes);
  `hello`; `commands` lista o registro de um `app` com fakes; `POST` roda e devolve `Pending`; `confirm`;
  SSE entrega eventos e retoma por `Last-Event-ID`; WS de painel ecoa bytes de um `FakePane`; TCP sem token
  → 401; `client.go` cobre tudo.
- Aceite: `rpc` ≥ 90 %; nenhum teste existente tocado.

**WP-3.3 — `internal/lua`** · dono: `internal/lua/**`, `internal/lua/testdata/*.lua`
- Testes primeiro: **`while true do end` interrompido em ≤ 50 ms + folga** (o primeiro teste de todos);
  erro Lua vira `error` e toast; `nk.command` aparece em `Registry.List()` com `Source`; `nk.keymap.set`
  reflete no keymap; `nk.on` recebe payload de `pubsub`; reload com arquivo quebrado mantém o último bom;
  três falhas desativam o arquivo; sem `init.lua` nada muda.
- Entrega: `runtime.go`, `bridge.go` (`nk.*` do §5.2 menos `nk.ui.*`), `loader.go`.
- Aceite: `lua` ≥ 90 %; sem Bubble Tea.

**WP-3.4 — `cmd/nekuroddo`: `serve`, `--listen`, `run`** · dono: `cmd/nekuroddo/**` · depende: 3.2
- Testes primeiro: `main_test.go`: `nk serve --listen unix://…` sobe e responde `hello`; `nk run inbox.summon`
  fala com o socket; `hook`/`hook-context` continuam sem construir `app` (teste de tempo e de imports).
- Aceite: caminho dos hooks inalterado (`events.Record`, `groups.Hook` direto).

**WP-3.5 — Docs da onda** · dono: `docs/**` · entrega: `docs/protocolo.md`, `docs/lua.md`, `docs/comandos.md`.

**Onda 4 (serial) — WP-4.1 — `internal/ui` → `internal/tui`, `tui.New(app, cfg)`** · dono: `internal/tui/**`,
`cmd/nekuroddo/main.go`, `docs/**` (renomes)
- Entrega: rename mecânico; `New` recebe `*app.App`; helpers de teste (`bhModel`, `groupModel`) montam
  `app.New(cfg, apptest.Ports())` e deixam de precisar de `NEKO_CLAUDE=/bin/false`; wiring em `cmd`.
- Aceite: `make ci` ≥ 80 % (esperado ≈ 95 %); `golangci-lint` limpo; `docs/arquitetura.md` final da fase.

### Fase 2 — Core como serviço e Lua completo

Objetivo: `nk serve` headless funciona sozinho; o protocolo do §3 está completo e documentado; Lua v1 inteiro.
Paralelo: 5.1 ‖ 5.2 ‖ 5.3 ‖ 5.4; 5.5 depois.

- **WP-5.1 — agendador no `app`** (`app/scheduler.go`): refresh/eventos/usage como goroutines com `Clock`
  falso nos testes; a TUI deixa de agendar `tickMsg/eventsTickMsg/usageTickMsg`. Dono: `app/scheduler.go`,
  `tui/update.go` (só os `case` de tick). Aceite: `nk serve` atualiza sem TUI.
- **WP-5.2 — `/v1/view/tree`, `/v1/theme`, `inbox.summon`, `nk serve --demo`** (fakes `apptest` com cenário
  roteirizado: 3 contextos, uma bloqueada, uma ✉ a cada 20 s). Dono: `rpc/view.go`, `app/demo.go`,
  `theme/export.go`. Aceite: Playwright da fase 3 depende disto.
- **WP-5.3 — Lua `nk.ui.*` por capacidade** (eventos `toast`/`confirm.requested`; `nk.ui.has`), `:lua` na
  paleta, `lua.reload`, `lua.messages`. Dono: `lua/ui.go`, `tui/dialog.go` (item `:lua`), `config` (`[lua]`).
- **WP-5.4 — bytes de painel para clientes remotos**: `Process.Subscribe` com tee cru, política de resize
  (último vence + evento), WS binário fim a fim com um `bash` real. Dono: `term/process.go`, `rpc/panes.go`.
- **WP-5.5 — docs e release**: `docs/protocolo.md` final, `docs/lua.md`, `CHANGELOG`, `docs/releases/v0.2.0.md`.

### Fase 3 — GUI MVP

Objetivo: `nkg` abre, mostra a árvore, os gatos por estado, janelas arrastáveis com `claude attach` real, o
gato da inbox, `shift+space`, e todos os atalhos do keymap. Paralelo: 6.1 primeiro (um dia), depois
6.2 ‖ 6.3 ‖ 6.4 ‖ 6.5 ‖ 6.6.

- **WP-6.1 — scaffold e cliente do protocolo** · dono: `gui/web/**` (raiz, `src/client/**`) — Vite + TS +
  Preact; `client.ts` (hello, snapshot, SSE com retomada, commands, keymap, WS de painel) com Vitest contra
  `fetch`/`EventSource`/`WebSocket` falsos; `vitest --coverage` com limiar 80 % de linhas; CI job `frontend`.
- **WP-6.2 — janelas e teclado** · dono: `gui/web/src/{windows,keymap,chrome}/**` — gerenciador de janelas
  (drag limitado, z-order, zoom, fechar), xterm.js + WebGL + WS, INSERT/`ctrl+]`/`ctrl+k`, resolução de
  teclas pelo `/v1/keymap`, paleta `:`; Playwright contra `nk serve --demo` (abre painel, digita, `w` fecha).
- **WP-6.3 — cena e gatos** · dono: `gui/web/src/scene/**` — renderer sob demanda (`invalidate()`), gato
  procedural toon, `state.ts` puro (snapshot → modelo de cena: pose, coleira, filhotes) com testes de tabela,
  estações por contexto, `InstancedMesh` para filhotes, `prefers-reduced-motion` e `view.flat`.
- **WP-6.4 — gato da inbox e ✉** · dono: `gui/web/src/inbox/**` — reducer puro da fila (entra, anda, para,
  é clicado, some ao resolver `session.attention → none`), caminhada na calçada, `shift+space` →
  `inbox.summon`, arco do envelope em `peer.message`, badge estático em reduced-motion.
- **WP-6.5 — shell Wails e `guihost`** · dono: `cmd/nekuroddo-gui/**`, `internal/guihost/**`, `Makefile`
  (`build-gui`), `.github/workflows/ci.yml` (job `gui-build` com `libgtk-4-dev libwebkitgtk-6.0-dev`,
  build only) — `Endpoint()`, modo embutido e `--connect`, proxy unix→loopback (testado com `httptest`),
  relançamento com `WEBKIT_DISABLE_DMABUF_RENDERER=1`. `cmd/nekuroddo-gui` sai do perfil de cobertura como
  `cmd/vtprobe`; `guihost` entra.
- **WP-6.6 — docs** · dono: `docs/gui.md`, `docs/instalacao.md` (requisitos GTK4/WebKitGTK), README.

Aceite da fase: Playwright verde no CI; `scripts/gui-idle-cpu.sh` < 1 % por 30 s; RAM e tamanho dentro do
§4.7; os 40 atalhos de `docs/teclas.md` funcionam no GUI (teste Playwright de tabela por comando).

### Fase 4 — Polimento e desempenho

- Upgrade de asset glTF CC0 (Quaternius) atrás de flag; som opcional; coleira/ícones finais; a11y (foco
  visível, leitor de tela nas janelas).
- `nk serve` como unidade systemd de usuário; TUI remota (`nk --attach unix://…`) usando `term.Screen`
  alimentado pelo WS — fecha o ciclo "dois clientes, um core".
- Só se faltar: grade de células `uv.ScreenBuffer` via protocolo (opção C2 da pesquisa) para efeitos por
  célula.

### Fase 5 — Observar

Servo/Verso e `tauri-runtime-verso` em 2027 (issue #30822, LTS do Servo); `ghostty-web` como troca do
xterm.js; `arnodel/golua` para quotas de memória; cliente web/mobile é "mais um cliente de `/v1`".

## 7. Riscos e mitigação; o que NÃO fazer

| risco | mitigação |
|---|---|
| WebGL no WebKitGTK falhar (NVIDIA, X11, DMABuf) | testar cedo na máquina alvo (Wayland/CachyOS) no WP-6.5; fallback automático `WEBKIT_DISABLE_DMABUF_RENDERER=1`; modo 2D sempre disponível; plano B Godot documentado |
| Wails v3 em beta | frontend fala só `/v1` + `Endpoint()`; trocar o shell por Tauri é um pacote; Playwright roda em Chromium puro |
| refatorar `keyNormal` em bloco e perder um atalho | WP-3.1 é um `case` por commit com os testes de tecla rodando a cada passo |
| corrida em `internal/term` | WP-1.3 isolado e primeiro; `stab_test`/`live_stab_test` sob `-race` são o gate |
| Lua congelar a UI | `callLua` com `SetContext` a cada instrução, 50 ms; teste do `while true` é o primeiro da fatia |
| hooks ficarem lentos | `nk hook`/`hook-context` nunca constroem `app`; teste de imports e de tempo em `main_test.go` |
| cobertura Go cair com código de GUI | GUI é outro binário e outro gate (Vitest 80 %); `cmd/nekuroddo-gui` fora do perfil como `vtprobe` |
| dois `nk` resumindo a mesma sessão | `PaneManager` único por core; `nkg --connect` reutiliza o core da TUI quando ela está aberta |
| conflitos entre agents | dono por arquivo, ondas, blocos em `model.go`, `wp-scope.sh`, integrador único |

**Não fazer:** reescrever `update.go` de uma vez; mover `claude/store/jira/todo/groups/events` para
`internal/adapter/*`; introduzir `Clock` em todo lugar no dia 1; tornar o socket obrigatório para o `nk`
comum; colocar Wails/cgo/GTK no binário `nekuroddo` (contaminaria os hooks); usar bindings do Wails além de
`Endpoint()`; duplicar a lógica de árvore/ordem em TypeScript (é `/v1/view/tree`); `CSS3DRenderer` na v1
(janelas DOM com transform bastam e são mais nítidas); rodar Lua nos hooks; dar à Lua referências vivas do
`Model`; loop de 60 fps parado; Servo agora.

## 8. Glossário

- **core** — `internal/core`: regras puras, sem I/O; **app** — `internal/app`: casos de uso com efeitos via
  portas; **shell** — quem traduz um protocolo (teclas, HTTP, Lua) em comandos (`tui`, `rpc`, `lua`, `guihost`).
- **porta / adaptador** — interface definida por `app` / pacote que a implementa (`store.Store`, `term.Process`).
- **registro de comandos** — tabela única nome → ação, lida por keymap, paleta, Lua, RPC e GUI.
- **token de confirmação** — identificador de uso único que substitui o closure `confirmThen` entre clientes.
- **snapshot / evento** — estado completo (`store.Snapshot`) / mudança publicada com `Seq` no barramento.
- **Process / Screen** — as duas metades de um painel: PTY + bytes (sem Bubble Tea) / emulador + desenho.
- **gato da inbox, filhote, coleira, calçada** — o gato que anda pela base com a notificação; subagent;
  cor do grupo; faixa inferior reservada à caminhada.
- **render sob demanda** — desenhar só quando algo mudou (`invalidate()`), CPU ociosa ≈ 0.
- **strangler fig** — migrar fatia a fatia atrás dos testes atuais, sem reescrever.
- **WP / onda / integrador** — pacote de trabalho com dono de arquivos / conjunto de WPs sem arquivos em
  comum / agent que faz merge em ordem e roda `make ci`.

