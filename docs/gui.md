---
type: Guide
title: GUI (nkg)
description: Guia de uso do GUI 3D de gatinhos low-poly (nkg) — como rodar, a cena, o gato da inbox, janelas, chrome, teclado, Lua, acessibilidade, desempenho e solução de problemas.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/web
tags: [gui, nkg, three.js, xterm, wails, webkitgtk, low-poly, fase-3]
timestamp: 2026-10-16T22:00:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T22:00:00Z }
status: stable
---

# GUI (`nkg`)

O `nkg` é o companheiro gráfico da TUI: o mesmo [core](arquitetura.md) e o mesmo
[protocolo](protocolo.md), só que desenhado como uma cena 3D low-poly fofa, com um gatinho por sessão em vez
de uma linha de árvore. Nada de domínio mora no GUI — ele fala só HTTP+SSE+WS com o `internal/rpc` (ou com o
core embutido no mesmo processo), lê o mesmo registro de comandos e o mesmo `/v1/keymap` que a TUI, e qualquer
comando ou tecla que você adiciona em Lua aparece nos dois clientes sozinho.

**O GUI funciona de ponta a ponta hoje**: `make run-gui-browser` abre de verdade no navegador, com a cena, o
gato da inbox, as janelas de terminal e o chrome inteiros — não é mais só o esqueleto do frontend. A única
peça que este guia não pôde confirmar na prática é a janela nativa (Wails): o código está pronto e compila sob
a tag `wails`, mas esta máquina não tem o WebKitGTK 6.0 instalado, então **ainda não foi verificado em
hardware** (ver [Como rodar](#como-rodar)). O desenho completo está em [plano-arquitetura.md](plano-arquitetura.md),
§1.2 e §4 (a "Fase 3" lista os pacotes de trabalho que entregaram isto).

## Estado atual

| peça | onde | estado |
|---|---|---|
| Binário `nkg` (`--browser`, `--connect`, `--listen`, `--demo`, `--dev`, `--safe-gfx`) | `cmd/nekuroddo-gui` | **implementado** |
| Host HTTP: serve o frontend, proxy de `/v1`, login por link de uso único | `internal/guihost` | **implementado** |
| Janela nativa (Wails v3 + GTK4/WebKitGTK 6.0) | `cmd/nekuroddo-gui` (tag `wails`) | **implementado em código** — não verificado em hardware |
| Cena 3D e gatos | `gui/web/src/scene` | **implementado** |
| Gato da inbox (calçada) | `gui/web/src/inbox` | **implementado** |
| Janelas de terminal | `gui/web/src/windows` | **implementado** |
| Chrome (topo, árvore, paleta, status, overlays) | `gui/web/src/chrome` | **implementado** |
| Resolução de teclado | `gui/web/src/keymap` | **implementado** |
| Cliente do protocolo e store | `gui/web/src/{client,state}` | **implementado** |

## Como rodar

### No navegador (o jeito do dia a dia)

```sh
make run-gui-browser
```

Isso builda o frontend (`cd gui/web && npx vite build`) e sobe `go run ./cmd/nekuroddo-gui --browser --demo
--dev`: sem janela nativa, sem precisar de WebKitGTK em lugar nenhum. O `nkg` abre o navegador padrão do
sistema sozinho (`xdg-open` no Linux, `open` no macOS) numa URL local; `--no-open` só imprime o link (que vale
uma vez — veja [Acesso](#acesso-e-segurança) abaixo) em vez de abrir.

Sem `--demo`, o `nkg` embute o core de verdade (o mesmo `wire.Default` que `nk` usa) e mostra suas sessões
reais, não um mundo roteirizado.

Flags do binário (`cmd/nekuroddo-gui/main.go`):

| flag | o quê |
|---|---|
| `--browser` | sem janela: serve e abre o navegador padrão |
| `--no-open` | com `--browser`: só imprime o link de abertura |
| `--demo` | mundo de demonstração roteirizado (sem `claude` de verdade), combina com qualquer modo |
| `--connect unix://…`/`tcp://…` | não embute o core: conecta num `nk serve` (ou numa TUI com `nk --listen`) já rodando, pelo mesmo endereço que `nk run` usa; não combina com `--demo` nem `--listen` |
| `--token-file` | token do `--connect tcp://` (padrão: o mesmo arquivo que `nk run` lê) |
| `--listen unix://…`/`tcp://127.0.0.1:0` | também serve o core embutido por RPC nesse endereço — outro cliente (`nk run`, a TUI) enxerga as mesmas sessões e painéis que o GUI |
| `--addr` | endereço loopback da própria página (padrão `127.0.0.1:0`, porta aleatória) |
| `--assets` | frontend construído a usar, com `--dev` ou num binário sem a tag `wails` (padrão `gui/web/dist`) |
| `--dev` | lê o frontend do disco (`--assets`) em vez do embutido no binário |
| `--safe-gfx` | janela nativa com `WEBKIT_DISABLE_DMABUF_RENDERER=1` (NVIDIA/X11); ver [Solução de problemas](#solução-de-problemas) |

### Acesso e segurança

O `nkg` serve a página e o `/v1` da mesma origem loopback, então nada é cross-origin. A janela (ou o
navegador) abre com um link que carrega uma chave de uso único; na primeira carga essa chave é trocada por um
cookie de sessão `HttpOnly`, `SameSite=Strict` — recarregar a mesma URL depois já usa o cookie, e reusar o
link antigo dá `401`. Requisições cujo `Host` não é um nome loopback são recusadas (proteção contra DNS
rebinding). `internal/guihost` tira o cookie/token da página antes de repassar qualquer coisa ao core, seja
ele embutido (handler em processo) ou um `nk serve` remoto por proxy reverso (unix socket ou TCP loopback).

### Janela nativa

Sem `--browser`, `nkg` abre como janela nativa (Wails v3, `v3.0.0-beta.30`): GTK4 + WebKitGTK 6.0 no Linux,
1280×800, título "nekuroddo". Precisa da tag de build `wails` e das bibliotecas instaladas:

| distro | pacote |
|---|---|
| Arch / CachyOS | `sudo pacman -S webkitgtk-6.0` |
| Debian / Ubuntu | `sudo apt install libwebkitgtk-6.0-dev libgtk-4-dev` |
| Fedora | `sudo dnf install webkitgtk6.0-devel gtk4-devel` |

```sh
make build-gui   # vite build + copia gui/web/dist para cmd/nekuroddo-gui/dist (go:embed) + go build -tags wails
bin/nkg
```

**Isto ainda não foi verificado em hardware** — esta máquina não tem o WebKitGTK 6.0 instalado
(`pkg-config webkitgtk-6.0` falha), então o `bin/nkg` com a tag `wails` nunca chegou a ser compilado nem
aberto aqui. O caminho `--browser` (sem essa tag) é o que foi de fato exercitado pelos testes e pelo script de
screenshot (`gui/web/scripts/shot.mjs`).

## A cena

Cada sessão (não arquivada, que não seja `history` — a menos que a árvore a mostre, por exemplo com
`view.show_all`) vira um gatinho procedural: `MeshToonMaterial` com uma rampa de 3 bandas, cabeça icosaedro,
orelhas cone, corpo cápsula, zero assets baixados (`gui/web/src/scene/cat.ts`). A pose segue o vocabulário de
estado que a TUI já usa, nesta ordem de precedência:

| ordem | condição | pose |
|---|---|---|
| 1 | terminou com erro/falhou | `puff` (pelo arrepia, flash) |
| 2 | atenção pendente (`blocked`) | `alert` (balão `!`) |
| 3 | `busy` | `typing` (patinhas no teclado, brilho) |
| 4 | `done`/terminou/`attention=2` | `sleeping` (`zZ`) |
| — | nenhuma das anteriores | `sit` |

**Filhotes** são os subagents rodando, desenhados como um único `InstancedMesh` ao lado do pai. **Coleiras**
são a cor do primeiro grupo da sessão (a mesma paleta que a TUI usa na barra `▍`); as coleiras das sessões que
compartilham um grupo com a sessão sob o mouse (ou selecionada) acendem. Um ✉ entre sessões de um grupo vira
um arco tracejado entre remetente e destinatário, com um envelope que voa de um para o outro quando a
mensagem é nova.

A cena desenha sob demanda (`gui/web/src/scene/loop.ts`, com testes que provam zero quadros numa cena parada):
nada de `requestAnimationFrame` contínuo. Um "bout" de respiração/rabo roda 1,6 s a 10 fps a cada 9 s, e de
novo ao passar o mouse sobre um gato; transições de pose e o voo do envelope rodam a 60 fps só enquanto duram
(até 1,4 s). `devicePixelRatio` é limitado a 1,5. A cena para de desenhar quando a aba fica oculta
(`visibilitychange`) e volta com um quadro de atualização ao reaparecer. Com `prefers-reduced-motion`, não há
nenhum movimento ambiente — o contador de depuração `ambientFrames` fica em 0 para sempre.

**Modo 2D** (`gui/web/src/scene/Flat.tsx`): a mesma cena como lista de texto, um ᓚᘏᗢ por sessão com as pílulas
de estado da TUI. Liga com o comando `view.flat`, com `?flat=1` (ou `?scene=flat`) na URL, ou sozinho se o
WebGL não puder ser inicializado (`webglcontextlost` também cai para ele). Clicar num gato (3D ou 2D) roda
`session.open`; passar o mouse mostra o label.

## O gato da inbox

Uma faixa de 72 px na base da janela — a **calçada** — por onde andam gatos de pixel ao estilo oneko (preto,
olhos amarelos, desenhados em SVG, sem asset nenhum: `gui/web/src/inbox/sprite.tsx`), cada um carregando uma
bolha com o que traz: `!` para uma sessão `blocked`/que voltou (`returned`), ✉ para uma mensagem entre
sessões de um grupo. Até 5 gatos ficam visíveis de uma vez (`MAX_VISIBLE`); o resto vira um badge "+N". A
ordem é `blocked` > `returned` > ✉ > `idle`, depois a atividade mais recente — a mesma função `RANK` espelha o
`core.SortInbox` do lado do servidor.

Clicar num gato da calçada abre a sessão que ele carrega (`session.open`). **`shift+space`** (em qualquer
lugar fora de um terminal ou campo de texto) roda o comando de escopo `core` `inbox.summon` — ele já existia
antes do GUI, é só `nk run inbox.summon` na linha de comando — que escolhe a sessão mais urgente
(`core.SortInbox`) e publica `inbox.summoned {session_id, label}`; todo GUI conectado que receber esse evento
abre a mesma sessão e faz o gato dela brilhar por 1,5 s.

Fora da janela (ou se o compositor não deixar um atalho global chegar ao navegador/webview), a solução honesta
é ligar `nk run inbox.summon` a um atalho do próprio compositor — ele fala com o core pelo socket, sem
depender de foco:

**Hyprland** (`~/.config/hypr/hyprland.conf`):

```ini
bind = SUPER, space, exec, nk run inbox.summon
```

**KDE Plasma**: Configurações do Sistema → Atalhos → Atalhos Personalizados → Novo → Atalho Global →
Comando/URL; defina o atalho (ex.: `Meta+Space`) e o comando `nk run inbox.summon`.

Com `prefers-reduced-motion`, os gatos não andam: um badge estático no canto da calçada lista os mesmos itens.

## Janelas

Cada janela de terminal é um painel real (`claude attach` por trás, não uma reimplementação): `POST /v1/panes`
+ o WebSocket do painel alimentam um `xterm.js`, com o addon WebGL (cai para o renderer DOM sozinho se o
contexto WebGL falhar ou for perdido) e o addon de fit (`gui/web/src/windows/xterm.ts`).

Arrastar pela barra de título ergue a janela 2° e 3 px (`transform: rotateX(2deg) translateY(-3px)`); o canto
inferior direito redimensiona; `z` ou duplo clique na barra alterna zoom (a janela ocupa a cena inteira); `w`
ou o ✕ fecha a janela **e mata o painel** no core; `alt+setas` move a janela ativa em passos de 24 px (4× com
shift). Focar uma janela entra em **INSERT**: as teclas vão para o `claude` dentro do painel, exceto `ctrl+]`
(volta para NORMAL) e `ctrl+k` (abre o switcher) — o mesmo par que a TUI usa para o painel embutido. Um painel
que sai sozinho (o `claude` termina) fecha a janela e mostra um toast.

## Chrome

A moldura DOM em volta da cena e das janelas (`gui/web/src/chrome`):

- **Barra do topo**: contagem de sessões rodando/te esperando/que voltaram, e a barra de uso das 5 h (e da
  semana, quando o limite existe) — os mesmos números do cabeçalho da TUI.
- **Coluna da árvore**: as quatro abas (`1` sessões, `2` inbox, `3` todos, `4` arquivo), cursor, dobras,
  cores por contexto, chips `#grupo` e a pista ✉ (mandou/recebeu) em cada linha.
- **Barra de status**: modo atual (NORMAL/INSERT) e as dicas de tecla do `GET /v1/keymap`.
- **Paleta `:`** (`palette.open`): os comandos do core para a seleção atual, "ir: sessão", temas, e os
  comandos digitados com gramática própria — `group <tag>`, `ungroup [tag]`, `notify #tag <mensagem>`,
  `<nome> <args>…` para qualquer comando `core` com argumentos, e `lua <código>` quando a capacidade Lua está
  ligada (`gui/web/src/chrome/palette.ts`, a mesma gramática de `internal/tui/dialog.go`).
- **`ctrl+k`**: o switcher — sessões rodando ou te esperando, mais urgentes primeiro.
- **`/`**: filtro da árvore. **Help** e **uso** (`usage.show`) como folhas por cima de tudo.
- **Diálogo de confirmação**: `y`/`enter` confirma, `n`/`esc` cancela; enquanto um está aberto, nada mais
  processa teclas.
- **Toasts**: erros ficam 7 s, o resto 4 s; clicar dispensa na hora.

## Teclado

O GUI carrega `GET /v1/keymap` ao iniciar e resolve `KeyboardEvent → nome do comando` pela mesma tabela que a
TUI usa (os mesmos modos: `normal`, `normal.todos`, `insert`, `*`), com `POST /v1/commands/{nome}` para os
comandos de escopo `core`. Comandos de escopo `client` (`nav.*`, `tab.*`, `view.*`, `palette.open`,
`switcher.open`…) rodam no próprio frontend, sobre a árvore que ele já tem — sem chamada de rede por tecla.
Sequências de duas teclas (`gg`, `dd`) esperam até 800 ms pela segunda. **Todas** as teclas de
[teclas.md](teclas.md) valem no GUI; nada é só de mouse (arrastar janela também tem `alt+setas`, clicar no
gato é só um atalho para `session.open`). `app.quit` não fecha nada — mostra um toast lembrando que é só
fechar a janela (as sessões seguem rodando em background).

## Lua

O GUI não tem runtime Lua próprio: é o **mesmo** `init.lua`/`plugins/*.lua` carregado uma vez no core que vale
para os dois clientes — um `nk.command` ou `nk.keymap.set` aparece em `GET /v1/commands` e `GET /v1/keymap`
sozinho, sem nada específico de GUI. Veja [lua.md](lua.md) para a API completa.

A diferença é por **capacidade** (`nk.ui.has(cap)`). `cmd/nekuroddo-gui/host.go` declara, para o core que o
próprio `nkg` embute (sem `--connect`, `--demo` ou não), `guiCaps = [toast, confirm, cat, palette, scene,
notify]` — a única combinação no código inteiro que inclui `"scene"`. Com `--connect`, porém, quem responde ao
Lua é o processo do outro lado, não o `nkg`:

- `nk serve` usa `serveCaps = [toast, confirm, notify]` (`cmd/nekuroddo/host.go`) — sem `scene`, sem `cat`,
  sem `palette`.
- a TUI (`nk`, inclusive com `--listen`) usa `tuiCaps = [toast, confirm, cat, palette, notify]` — também sem
  `scene`.

Ou seja: `nk.ui.has("scene")` só é `true` quando o GUI embute o próprio core (`nkg`/`nkg --demo` sem
`--connect`); conectado a um `nk serve` ou a uma TUI com `--listen`, continua `false`, mesmo com a janela do
GUI aberta e a cena na tela. Um script que queira efeito visual na cena deve checar essa capacidade antes de
assumir que existe — o mesmo padrão de fallback que `nk.ui.has("notify")` já usa.

## Acessibilidade

- **`prefers-reduced-motion`**: a cena não anima nada por conta própria (nenhum "bout" de respiração/rabo —
  `ambientFrames` fica em 0) e o gato da inbox não anda: vira um badge estático no canto da calçada com a
  mesma informação.
- **Modo 2D** (`view.flat`, `?flat=1`/`?scene=flat` na URL): a mesma cena como lista de texto (ᓚᘏᗢ + as pílulas
  de estado da TUI), tudo clicável. É também o fallback automático se o WebGL não existir, falhar ao iniciar,
  ou for perdido depois (`webglcontextlost`).

## Orçamento de desempenho

Metas do plano §4.7:

- **Tamanho do bundle** (medido, `make gui-build`): **291 KB** gzip de JS + **4,7 KB** gzip de CSS — bem
  abaixo da meta de 1 MB (three.js + xterm.js + Preact).
- **CPU ociosa ≈ 0**: implementado e testado (`gui/web/src/scene/loop.test.ts` prova zero quadros com a cena
  parada) — render sob demanda, respiração/rabo a no máximo 10 fps, 60 fps só durante tweens/voo do ✉,
  `visibilitychange: hidden` para tudo, `devicePixelRatio` limitado a 1,5.
- **Memória** (`nkg` com WebKitGTK ≤ 250 MB, a parte Go ≤ 40 MB) e **tamanho do binário** (`bin/nkg` ≤ 20 MB):
  metas do plano, ainda não medidas nesta rodada — dependem da janela nativa, que não pôde rodar nesta máquina
  (ver [Como rodar](#janela-nativa)).
- **Partida** (primeiro quadro ≤ 1,5 s; snapshot inicial ≤ 300 ms): o snapshot já é o mesmo `store.Refresh` da
  TUI, servido por `GET /v1/snapshot` — rápido hoje; o primeiro-quadro de ponta a ponta não foi cronometrado
  nesta rodada.
- `scripts/gui-idle-cpu.sh` (o script de verificação citado no plano) ainda não existe no repositório.

## Desenvolvimento

Para iterar no frontend com hot reload, sem precisar rebuildar e relançar o `nkg` a cada troca:

```sh
cd gui/web
npm ci              # make gui-install faz o mesmo
npm run demo        # sobe `nk serve --demo` + o servidor de dev do Vite, proxy de /v1
```

Isso imprime a URL para abrir (já com `?token=`); é o jeito mais rápido de ver uma mudança de código refletida
na hora. `npm run dev` sozinho também funciona contra um `nk serve` já rodando em outro endereço — exporte
`NK_ENDPOINT=http://127.0.0.1:PORTA` antes (veja `gui/web/scripts/proxy.ts`). Para ver o GUI como um usuário
de verdade o abre, use `make run-gui-browser` (ou `bin/nkg` depois de `make build-gui`), não `npm run demo`.

## Solução de problemas

- **Tela em branco / cena não aparece no `nkg` nativo, mas funciona no navegador:** é o caso clássico de WebGL
  quebrado dentro do WebKitGTK (comum com NVIDIA proprietário em X11, ou renderização por DMABuf). Relance com
  `nkg --safe-gfx`, ou manualmente:

  ```sh
  WEBKIT_DISABLE_DMABUF_RENDERER=1 nkg
  ```

  `--safe-gfx` também liga sozinho, sem precisar pedir, na próxima partida depois que a janela crashar ou não
  fechar direito: `nkg` deixa um marcador em `$XDG_STATE_HOME/nekuroddo/nkg-window.running` enquanto a janela
  está aberta e o apaga ao fechar normalmente; achar esse marcador no início é sinal de que a vez anterior não
  terminou bem, e a nova tentativa já sobe com o renderer seguro (nunca por cima de um valor que você mesmo já
  tenha posto na variável de ambiente).
- **NVIDIA / X11:** os bugs documentados de WebGL no WebKitGTK 6.0 concentram-se nessa combinação; em
  Wayland (o alvo principal, CachyOS) a renderização costuma ser direta. Se `nkg` nem abrir a janela, confira
  se o WebKitGTK 6.0 e o GTK4 estão instalados (ver [Janela nativa](#janela-nativa) acima e
  [instalacao.md](instalacao.md#gui-nkg)).
- **Nada de WebGL funciona de jeito nenhum:** `nkg --browser` abre no navegador padrão do sistema, que
  costuma ter um caminho de WebGL mais maduro que o WebKitGTK 6.0 ainda em beta; ou use o modo 2D
  (`view.flat`, [Acessibilidade](#acessibilidade)), que também é o fallback automático se o WebGL falhar de
  vez.
- **`frontend não encontrado`** ao rodar `go run ./cmd/nekuroddo-gui` direto: falta `gui/web/dist` — rode
  `make gui-build` (ou `cd gui/web && npx vite build`) antes, ou passe `--assets`.

## Ver também

- [plano-arquitetura.md](plano-arquitetura.md) — §1.2 (stack), §4 (cena e UX completas), §4.7 (orçamento),
  §4.9 (como o GUI se liga ao core) e "Fase 3" (pacotes de trabalho WP-6.1 a WP-6.6).
- [protocolo.md](protocolo.md) — o protocolo v1 que o GUI fala.
- [lua.md](lua.md) — a API `nk.*`, eventos e capacidades de UI.
- [teclas.md](teclas.md) — todas as teclas e o comando por trás de cada uma.
- [instalacao.md](instalacao.md#gui-nkg) — dependências e pacotes por distro.
