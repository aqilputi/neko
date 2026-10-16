---
type: Guide
title: GUI (nkg)
description: Guia de uso do GUI 3D de gatinhos low-poly (nkg) — como rodar hoje e como está planejado, a cena, o gato da inbox, janelas, teclado, Lua, acessibilidade, desempenho e solução de problemas.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/web
tags: [gui, nkg, three.js, xterm, wails, webkitgtk, low-poly, fase-3]
timestamp: 2026-10-16T23:50:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T23:50:00Z }
status: draft
---

# GUI (`nkg`)

O `nkg` é o companheiro gráfico da TUI: o mesmo [core](arquitetura.md) e o mesmo
[protocolo](protocolo.md), só que desenhado como uma cena 3D low-poly fofa, com um gatinho por sessão em vez
de uma linha de árvore. Nada de domínio mora no GUI — ele fala só HTTP+SSE+WS com o `internal/rpc` (ou com o
core embutido no mesmo processo), lê o mesmo registro de comandos e o mesmo `/v1/keymap` que a TUI, e qualquer
comando ou tecla que você adiciona em Lua aparece nos dois clientes sozinho. O desenho completo está em
[plano-arquitetura.md](plano-arquitetura.md), §1.2 e §4 (a "Fase 3" lista os pacotes de trabalho).

**Este documento descreve o GUI planejado e em construção**, não só o que já roda. Cada seção diz claramente
se a peça existe em código hoje ou se é o alvo da Fase 3.

## Estado atual

| peça | onde | estado |
|---|---|---|
| Cliente do protocolo, store e sincronização (`client/`, `state/`) | `gui/web/src/{client,state}` | **pronto** — com testes, é o que todo o resto consome |
| Cena 3D e gatos (WP-6.3) | `gui/web/src/scene` | **em construção** — peças puras já com teste (modelo de cena, animação, laço de render); o módulo ainda não exporta o componente `Scene` do contrato |
| Janelas de terminal e chrome (WP-6.2) | `gui/web/src/{windows,chrome}` | **em construção** — só o esqueleto do módulo |
| Resolução de teclado (WP-6.2) | `gui/web/src/keymap` | **em construção** — resolução de tecla→comando e navegação já com teste; o módulo ainda não exporta o contrato que `chrome`/`windows` vão consumir |
| Gato da inbox (WP-6.4) | `gui/web/src/inbox` | **quase pronto** — já exporta `InboxWalk`/`installInboxKeys` pelo contrato; falta só o `App.tsx` (WP-6.2) montá-lo |
| Host HTTP do GUI: serve o frontend construído, faz proxy de `/v1` (embutido ou `--connect`), login por link de uso único (WP-6.5) | `internal/guihost` | **em construção, já funcional como pacote** — falta quem o chame |
| Binário `nkg` (flags, janela Wails) (WP-6.5) | `cmd/nekuroddo-gui` | **não existe ainda** — nenhum binário `nkg` para rodar hoje |
| O core que o GUI fala (`internal/rpc`, `nk serve --demo`) | `internal/rpc`, `internal/demo` | **pronto** — é o que faz o modo demo abaixo funcionar hoje |

Na prática: hoje dá para abrir o frontend web num navegador comum contra `nk serve --demo` (próxima seção) e
ver a árvore em texto simples (`App.tsx`, dono do WP-6.2, ainda não monta `scene`/`chrome`/`windows`/`inbox` —
eles substituem esse corpo quando prontos, módulo por módulo). Não existe janela nativa, nem `--connect`, nem
gatos na tela ainda.

## Como rodar

### Hoje: o frontend num navegador, contra `nk serve --demo`

Sem o shell `nkg`, o jeito de ver o GUI em construção é rodar o frontend Vite num navegador qualquer e deixar
que ele fale com um `nk serve --demo` de verdade:

```sh
cd gui/web
npm ci              # make gui-install faz o mesmo
npm run demo        # sobe `nk serve --demo` + o servidor de dev do Vite, proxy de /v1
```

O comando imprime a URL para abrir (já com `?token=`). `npm run dev` sozinho também funciona se você já tem um
`nk serve` rodando em outro endereço — exporte `NK_ENDPOINT=http://127.0.0.1:PORTA` antes (veja
`gui/web/scripts/proxy.ts`). O endpoint também pode vir pela própria URL da página
(`?endpoint=http://…&token=…`), o que os testes Playwright usam.

### Planejado: o binário `nkg`

Quando o WP-6.5 existir, o uso do dia a dia é um binário Go, `nkg`, com dois modos:

- **`nkg --browser [--demo]`** — sobe o core (embutido, ou o mundo de mentira com `--demo`) e abre o
  navegador padrão do sistema na URL local. **Não precisa de WebKitGTK** — é o caminho mais simples em
  qualquer distro, inclusive antes de instalar as dependências de janela nativa abaixo.
- **`nkg`** (sem `--browser`) — abre como janela nativa (Wails v3: GTK4 + WebKitGTK 6.0 no Linux). Precisa do
  WebKitGTK instalado:

  | distro | pacote |
  |---|---|
  | Arch / CachyOS | `sudo pacman -S webkitgtk-6.0` |
  | Debian / Ubuntu | `sudo apt install libwebkitgtk-6.0-dev` |
  | Fedora | `sudo dnf install webkitgtk6.0` |

- **`nkg --connect unix://$XDG_RUNTIME_DIR/nekuroddo-$(id -u).sock`** — não embute o core: conecta num
  `nk serve` (ou num `nk --listen` com a TUI aberta ao lado) já rodando, pelo mesmo socket Unix que
  `nk run` usa. `internal/guihost` faz o proxy unix→loopback para a webview chegar nele. Dois clientes no
  mesmo core enxergam as mesmas sessões, painéis e confirmações (ver [plano-arquitetura.md
  §4.9](plano-arquitetura.md#49-como-o-gui-se-liga-ao-core)).
- **`--demo`** combina com qualquer um dos dois: embute o mesmo `internal/demo` que `nk serve --demo` usa, em
  vez do core real — é o que os testes Playwright da Fase 3 rodam.

Nenhuma dessas flags existe em código ainda — `cmd/nekuroddo-gui` não existe —, mas a peça que vai servir
tudo isso já está em construção em `internal/guihost`: um host HTTP que serve o frontend construído
(`gui/web/dist`) e faz proxy de `/v1/**` (HTTP, SSE e os WebSockets de painel) para o core, seja um
`rpc.Server.Handler()` embutido no mesmo processo, seja um `--connect unix://…`/`tcp://…` de um `nk serve` já
rodando — com um link de abertura de uso único trocado por um cookie de sessão, para a janela nunca expor o
token na URL depois do primeiro carregamento. Falta só `cmd/nekuroddo-gui` ligar os flags acima e a janela
Wails a esse host. Esta seção será corrigida para "como implementado" quando o binário nascer.

## A cena

**Em construção** (`gui/web/src/scene`, WP-6.3 — o módulo ainda não exporta o componente `Scene` que o
`App.tsx` final vai montar, mas as peças puras já têm código e teste: modelo de cena, animação, laço de
render sob demanda). O plano:

Uma janela, três camadas: uma cena WebGL ao fundo (uma prateleira/escrivaninha rasa em low-poly, câmera fixa
levemente isométrica), janelas DOM dos terminais flutuando por cima, e o chrome em DOM (topo, árvore, barra de
status) por cima de tudo. Uma faixa de ~72 px na base — a **calçada** — fica livre de janelas; é por onde o
gato da inbox anda (próxima seção).

Cada sessão vira um gatinho procedural (`MeshToonMaterial`, cabeça esfera/orelhas cones/corpo cápsula — zero
assets baixados na v1). A pose segue o mesmo vocabulário de estado que a TUI já usa
(`store.Session.State`/`Attention`):

| estado | pose do gato | pista secundária |
|---|---|---|
| `busy` | sentado "digitando", patinhas batendo, orelhas em pé | brilho fino sob as patas na cor `busy` do tema |
| `blocked` / tem atenção pendente | senta, rabo bate, balão `!` acima | balão na cor `blocked`; é o que chama o gato da inbox |
| `done` / terminou | enrola e dorme (respiração lenta) | `zZ`, como na TUI |
| `idle` / parada | sentado quieto, pisca de vez em quando | coleira apagada |
| erro (`claude rm` falhou etc.) | pelo arrepia (um pop de escala no rabo), volta ao estado anterior | flash vermelho único, sem loop |
| `peer.message` (✉ entre sessões de um grupo) | remetente "joga"; um envelope percorre um arco até o destinatário | mesmo tempo do ciclo do ✉ na TUI (pisca 90 s, depois apaga por 15 min) |

**Filhotes** são os subagents: a mesma geometria em escala menor (`InstancedMesh`), ao lado do pai; os
terminados somem (ou ficam visíveis com `H`/`view.show_all`). **Coleiras** são a cor do grupo
(`core.GroupColorIndex(tag)` — a mesma paleta que a TUI usa na barra `▍`, servida pelo `GET /v1/theme`
(`contexts[]`)); os pares da sessão sob o cursor ganham a coleira acesa. Clicar num gato roda `session.open`;
passar o mouse mostra o label.

## O gato da inbox

**Quase pronto** (`gui/web/src/inbox`, WP-6.4 — o módulo já exporta `InboxWalk` e `installInboxKeys` pelo
contrato; falta o `App.tsx` da Fase 3 (WP-6.2) montá-lo junto com a cena). O que já existe em código,
independente do desenho da cena: o comando `inbox.summon` (escolhe a sessão mais urgente por `core.SortInbox` e a
abre, publicando `inbox.summoned {session_id, label}`) e a tecla `shift+space`, já registrados no core
(`internal/app/commands.go`) — então `nk run inbox.summon` já funciona hoje pela linha de comando, mesmo sem
GUI.

O plano para a cena: quando um item entra na inbox (uma sessão vira `blocked`/devolvida, ou chega um
`peer.message`), um gato entra pela esquerda da calçada carregando o ícone do que traz (`!` ou ✉) e caminha
até parar sob a janela ou o gato da sessão; itens seguintes entram em fila indiana. **Clique** nele abre a
sessão que ele carrega (`session.open`); **`shift+space`** dispara `inbox.summon` com o foco na janela do GUI.

Fora da janela (ou se o compositor não deixar um atalho global chegar ao navegador/webview), a solução honesta
é ligar `nk run inbox.summon` a um atalho do próprio compositor — ele fala com o core pelo socket, sem
depender de foco:

**Hyprland** (`~/.config/hypr/hyprland.conf`):

```ini
bind = SUPER, space, exec, nk run inbox.summon
```

**KDE Plasma**: Configurações do Sistema → Atalhos → Atalhos Personalizados → Novo → Atalho Global →
Comando/URL; defina o atalho (ex.: `Meta+Space`) e o comando `nk run inbox.summon`.

Com `prefers-reduced-motion` (ver [Acessibilidade](#acessibilidade)), o gato não anda: aparece um badge
estático no canto da calçada com a mesma informação.

## Janelas

**Em construção** (`gui/web/src/windows`, WP-6.2 — hoje só o esqueleto do módulo; o lado do protocolo que elas
vão usar — `POST /v1/panes` e o WebSocket do painel — já está implementado e documentado em
[protocolo.md](protocolo.md#painéis--ptys-remotos)). Cada janela de terminal é um painel real
(`claude attach`, não uma reimplementação): `xterm.js` com `@xterm/addon-webgl` (já nas dependências do
frontend; cai para DOM se o contexto WebGL falhar) recebendo os bytes crus do PTY pelo WebSocket.

Arrastar pela barra de título é limitado à área acima da calçada; `z` alterna zoom (a janela ocupa a cena);
`w` fecha. Focar numa janela entra em **INSERT**: as teclas vão para o `claude` dentro do painel, exceto
`ctrl+]` (volta para o nekuroddo) e `ctrl+k` (abre o switcher) — o mesmo par que a TUI usa.

## Teclado

O GUI carrega `GET /v1/keymap` ao iniciar (já funciona hoje, é só `curl` — ver
[protocolo.md](protocolo.md#get-v1keymap)) e resolve `KeyboardEvent → nome do comando` pela mesma tabela que a
TUI usa, com `POST /v1/commands/{nome}` para os comandos de escopo `core`. Comandos de escopo `client`
(`nav.*`, `tab.*`, `view.*`, `palette.open`…) rodam no próprio frontend, sobre a árvore que ele já tem — eles
não fazem uma chamada de rede por tecla. **Todas** as teclas de [teclas.md](teclas.md) valem no GUI; nada é só
de mouse (arrastar janela também tem `pane.move` por setas, clicar no gato é só um atalho para `session.open`).

A resolução de teclas em si (`gui/web/src/keymap`) é **em construção** (WP-6.2): a tabela tecla→comando e a
navegação já têm código e teste, mas o módulo ainda não exporta o contrato que o `chrome`/`windows` vão
consumir.

## Lua

O GUI não tem runtime Lua próprio: é o **mesmo** `init.lua`/`plugins/*.lua` carregado uma vez no core (TUI,
`nk serve` ou o core embutido no `nkg`) que vale para os dois clientes — um `nk.command` ou `nk.keymap.set`
aparece em `GET /v1/commands` e `GET /v1/keymap` sozinho, sem nada específico de GUI. Veja [lua.md](lua.md)
para a API completa.

A única diferença é por **capacidade** (`nk.ui.has(cap)`, §"Capacidades" de [lua.md](lua.md#capacidades-nkuihas)):
a constante `lua.CapScene` (`"scene"`) já existe em `internal/lua/runtime.go`, mas **nenhum host declara ela
hoje** — nem a TUI, nem `nk serve` (que não tem cena para desenhar) — porque `cmd/nekuroddo-gui` ainda não
existe. Até o WP-6.5 ligar essa capacidade, `nk.ui.has("scene")` sempre responde `false`, mesmo com um GUI
conectado. Um script Lua que quiser efeito visual na cena deve checar essa capacidade antes de assumir que
existe (o mesmo padrão de fallback que `nk.ui.has("notify")` já usa hoje).

## Acessibilidade

**Planejado** (a vir junto com o WP-6.3/6.4 que desenham a cena):

- `prefers-reduced-motion`: poses estáticas, sem respiração, e o gato da inbox vira um badge parado em vez de
  andar (ver [O gato da inbox](#o-gato-da-inbox)).
- **Modo 2D** (comando `view.flat`, ainda não registrado em `internal/app/commands.go`): troca a cena 3D por
  uma lista plana — o gato em texto (`ᓚᘏᗢ`) e as mesmas pílulas de estado da TUI — mantendo tudo funcional sem
  WebGL. É também o fallback automático se o WebGL falhar de vez (ver
  [Solução de problemas](#solução-de-problemas)).

## Orçamento de desempenho

Metas do plano (§4.7), ainda sem o script de verificação (`scripts/gui-idle-cpu.sh` não existe no repo hoje —
também WP-6.5):

- **CPU ociosa ≈ 0:** render sob demanda — o `requestAnimationFrame` só roda enquanto houver `invalidate()`
  pendente (evento, drag, caminhada, miado); respiração/rabo capados a 10 fps, 60 fps só durante drag/caminhada;
  `visibilitychange: hidden` para tudo; `devicePixelRatio` limitado a 1,5.
- **Memória:** o `nkg` inteiro (Go + processo web do WebKitGTK) ≤ 250 MB com 3 painéis abertos; a parte Go
  sozinha ≤ 40 MB.
- **Tamanho:** binário `nkg` ≤ 20 MB; bundle do frontend ≤ 1 MB gzip (three.js ~150 KB, xterm ~100 KB).
- **Partida:** primeiro quadro ≤ 1,5 s; snapshot inicial ≤ 300 ms (hoje já é rápido — é o mesmo
  `store.Refresh` da TUI, servido por `GET /v1/snapshot`).

## Solução de problemas

- **Tela em branco / cena não aparece no `nkg` nativo, mas funciona no navegador:** é o caso clássico de WebGL
  quebrado dentro do WebKitGTK (comum com NVIDIA proprietário em X11, ou renderização por DMABuf). Tente
  relançar com:

  ```sh
  WEBKIT_DISABLE_DMABUF_RENDERER=1 nkg
  ```

  Isso desliga o caminho de renderização por DMABuf do WebKitGTK, que é onde a maioria dos bugs de WebGL
  nessa combinação aparece.
- **`nkg --safe-gfx`** (planejado, WP-6.5): força direto o modo 2D (sem tentar WebGL), para quando nem a
  variável de ambiente acima resolve.
- **NVIDIA / X11:** os bugs documentados de WebGL no WebKitGTK 6.0 concentram-se nessa combinação; em
  Wayland (o alvo principal, CachyOS) a renderização costuma ser direta. Se `nkg` nem abrir a janela, confira
  se o WebKitGTK 6.0 e o GTK4 estão instalados (ver [Como rodar](#como-rodar) acima e
  [instalacao.md](instalacao.md#gui-nkg)).
- **Nada de WebGL funciona de jeito nenhum:** `nkg --browser` abre no navegador padrão do sistema, que
  costuma ter um caminho de WebGL mais maduro que o WebKitGTK 6.0 ainda em beta; ou espere o modo 2D
  (`view.flat`, [Acessibilidade](#acessibilidade)) quando ele existir — o plano já prevê isso como fallback
  automático, com aviso, se o WebGL falhar de vez.

## Ver também

- [plano-arquitetura.md](plano-arquitetura.md) — §1.2 (stack), §4 (cena e UX completas), §4.7 (orçamento),
  §4.9 (como o GUI se liga ao core) e "Fase 3" (pacotes de trabalho WP-6.1 a WP-6.6).
- [protocolo.md](protocolo.md) — o protocolo v1 que o GUI fala (já implementado e documentado).
- [lua.md](lua.md) — a API `nk.*`, eventos e capacidades de UI.
- [teclas.md](teclas.md) — todas as teclas e o comando por trás de cada uma.
