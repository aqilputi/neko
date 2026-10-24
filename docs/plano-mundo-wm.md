---
okf_version: "0.2"
type: Plan
title: Plano — o mundo como WM de verdade (nkw): telas no espaço, paridade com o niri e a sessão própria
description: Segundo adendo ao plano do mundo. O que falta para o nkw ser um gerenciador de janelas completo como o niri (lacuna por área, com fase), a especificação das telas no espaço (criar, arrastar no ar com o mouse, redimensionar em 3D, grupos, arranjos, telas que seguem você, ímãs, desfazer), o que mais entra nesta fase (DECORAR, notas, salas pelo telefone, modo foto), as decisões de arquitetura da sessão própria (um binário, backends, nunca tela preta, layer-shell e multimonitor no mundo 3D, roteamento do ponteiro, config e IPC), orçamentos, testes e o roteiro em cinco fases com go/no-go.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/office
tags: [plano, mundo, nkw, wayland, smithay, compositor, wm, niri, drm, kms, libinput, libseat, layer-shell, multimonitor, portais, ipc, arrumar, telas-no-ar, mouse, decorar, sessão]
timestamp: 2026-10-24T18:00:00Z
generated: { by: agent:claude-code/claude-fable-5-1, at: 2026-10-24T18:00:00Z }
status: draft
stale_after: 2027-04-24T00:00:00Z
sources:
  - id: plano-mundo
    resource: plano-mundo.md
    title: Plano — o mundo (nkw); este adendo não o substitui, estende (D1, §3, §4, §10 fase 7, §11)
  - id: plano-objetos
    resource: plano-mundo-objetos.md
    title: Plano — objetos do mundo; FEEL (§3), portas, cofre, notificações best-effort
  - id: mundo
    resource: mundo.md
    title: O mundo (nkw) — o que existe hoje, inclusive a matriz de compatibilidade
  - id: log
    resource: log.md
    title: Log — ondas 2, 3 e 4 do mundo (2026-10-22 a 24), medidas e pendências
  - id: smithay
    resource: https://github.com/Smithay/smithay
    title: Smithay — backends udev/DRM/libinput/libseat, DrmCompositor, layer-shell, session-lock, idle-notify, input-method, data-control
  - id: anvil
    resource: https://github.com/Smithay/smithay/tree/master/anvil
    title: anvil — compositor de exemplo do Smithay (MIT), referência de código do backend udev
  - id: niri
    resource: https://github.com/YaLTeR/niri
    title: niri — compositor de rolagem (GPL-3.0); referência de comportamento e de recursos, nunca de código
  - id: niri-wiki
    resource: https://github.com/YaLTeR/niri/wiki
    title: niri wiki — Configuration, IPC (niri msg), Layer-Shell, Screencasting, Workspaces, Floating Windows
  - id: wayland-protocols
    resource: https://gitlab.freedesktop.org/wayland/wayland-protocols
    title: wayland-protocols — ext-session-lock, ext-idle-notify, ext-image-copy-capture, ext-data-control, ext-foreign-toplevel-list, keyboard-shortcuts-inhibit, text-input-v3, pointer-gestures, tablet-v2
  - id: wlr-protocols
    resource: https://gitlab.freedesktop.org/wlroots/wlr-protocols
    title: wlr-protocols — layer-shell, screencopy, gamma-control, output-management, data-control, virtual-pointer
  - id: xdp-wlr
    resource: https://github.com/emersion/xdg-desktop-portal-wlr
    title: xdg-desktop-portal-wlr — backend de ScreenCast/Screenshot para compositores wlr-like (MIT)
  - id: xdp-gtk
    resource: https://github.com/flatpak/xdg-desktop-portal-gtk
    title: xdg-desktop-portal-gtk — FileChooser, Settings, Notification, AppChooser
  - id: vkms
    resource: https://docs.kernel.org/gpu/vkms.html
    title: VKMS — Virtual Kernel Mode Setting (testes de DRM sem GPU, writeback)
  - id: libseat
    resource: https://git.sr.ht/~kennylevinsen/seatd
    title: seatd / libseat — gerenciamento de seat sem privilégio
---

# Plano — o mundo como WM de verdade (`nkw`)

O pedido do usuário, na íntegra: *"OK, para isto virar um WM de verdade parecido com o niri, o que falta? Eu
queria criar telas e arrastá-las pelo ar, e outras coisas. Faça um plano para isso."*

Este documento responde em sete partes: a **lacuna** frente ao niri, área por área (§1); a especificação das
**telas no espaço**, que é o pedido explícito (§2); as **outras coisas** que valem entrar nesta fase (§3); as
**decisões de arquitetura** da sessão própria e do mundo com mouse (§4); **orçamentos e testes** (§5); o
**roteiro** com ordem, aceites e go/no-go (§6); e as **perguntas** que só o usuário responde (§7).

Ele estende o [plano do mundo](plano-mundo.md) e o [adendo dos objetos](plano-mundo-objetos.md). Tudo o que
eles decidem continua valendo, em especial: aninhado primeiro (D1), Smithay (D2), GLES próprio (D3), teclado
primeiro (§3.1), 1:1 no FOCO, os orçamentos de leveza (§1.3) e o FEEL especificado (§3 dos objetos). A fase 7
do plano original ("sessão própria") vira aqui a **fase D**, com conteúdo completo.

Onde estamos: o HEAD tem as ondas 1 a 4. O mundo roda aninhado no niri, com NAV, ANDAR, FOCO, CONVERSA, SALTO,
COMANDOS e telefone; salas, mesas, pilhas, telas no ar (`p` no NAV, `g` no ANDAR), HUD e telefone, biblioteca e
cofre, objetos e áudio por proximidade, apps de verdade com matriz de compatibilidade. É um WM aninhado
utilizável, com um só monitor, sem mouse além de clicar e olhar, e sem nada do que uma **sessão** precisa.

## 0. Em uma página

- **O que falta para ser "um WM como o niri"** cai em três grupos, e a ordem importa:
  1. **Telas no espaço** (o pedido): criar telas onde você mira, arrastá-las pelo ar com o mouse e pelo teclado,
     redimensionar em 3D, agrupar, salvar arranjos, telas que seguem você. Tudo isso roda **aninhado**, é
     visível já na primeira semana e não arrisca a sessão de ninguém. **Vai primeiro (fase A).**
  2. **Paridade de WM** com o niri, ainda aninhado: IPC `nkw msg`, regras de janela completas, recarga do
     `world.toml`, mesas em abas, salas criadas e renomeadas, `spawn` na partida, capturas de tela, hover com
     o mouse, urgência, animações configuráveis, ferramentas de depuração. **Fase B.** Mais o que o 3D
     oferece de graça e o niri não tem: DECORAR, notas na parede, modo foto. **Fase C.**
  3. **A sessão própria**: DRM/KMS atômico, libinput, libseat, multimonitor, layer-shell, bloqueio, ociosidade,
     captura e portais, gama, DPMS, IME, cursor, notificações. É o grosso do esforço e o único pedaço que pode
     deixar você sem tela. **Fase D, atrás de `nkw --session`**, com a sessão niri continuando padrão até um
     **go/no-go** medido (fase E).
- **Decisões novas** (§4): multimonitor como **"um olho, várias facetas"** (D11) — todos os monitores são
  janelas da mesma vista, lado a lado como uma baia de vidro; um monitor pode ser uma **vista fixa** (o mapa, uma
  estação). **Um binário** (`nkw`, `nkw --session`, `nkw --headless`) com um trait de backend (D12). **Nunca
  tela preta** (D13): modo plano de emergência, hook de panic que salva o estado, unidade de reinício e queda
  para o greeter. **Barras e lançadores externos são planos**, em 2D sobre o mundo; a faixa do HUD se recolhe
  quando há barra (D14). **Mouse**: arrastar move; clicar seleciona; `world+arrastar` funciona em todo modo,
  inclusive no FOCO (D15). **`world.toml` continua canônico, com recarga a quente; `nkw msg` é a IPC**, JSON
  por socket, com fluxo de eventos (D16). **Portais**: `xdg-desktop-portal-wlr` para captura e `-gtk` para o
  resto; `wlr-screencopy` + `ext-image-copy-capture` escritos à mão (D17).
- **As telas no espaço reaproveitam o que existe**: `Station::Air`, `carry.rs`, `collide::ray_hit_quad`,
  `onetoone.rs`, `persist::Air`, as intenções de lugar, o `--script` com ponteiro sintético. O novo é um
  **modo ARRUMAR** (`v` no NAV), uma máquina de estados do ponteiro, `magnet.rs` (ímãs e guias), `history.rs`
  (desfazer), **molduras vazias** (a próxima janela nasce ali), **clusters**, **arranjos** nomeados e **telas
  ancoradas** (seguem a vista e, a 1:1, são compostas em 2D exato, sem FOCO — o "segundo monitor" mesmo num
  monitor só).
- **Esforço**: A 3–4 semanas · B 3 · C 2 · D 6–8 · E 3–4, um dev com agents; 17 a 21 semanas no total. O
  go/no-go para a sessão virar padrão fica no fim da E, com critérios medidos (§6.7).

## 1. A lacuna frente ao niri

Legenda: ✓ existe · ◐ parcial · ✗ não existe · — não se aplica ao modelo do mundo. "Fase" é a deste plano
(A–E). "Aninhado" diz se o item faz sentido antes da sessão própria.

### 1.1 Sessão e backends

| área | hoje | o que fazer | fase |
|---|---|---|---|
| backend winit (aninhado) | ✓ | continua; o fork do Smithay com movimento relativo vira PR upstream (pendência da onda 4) | A |
| backend headless | ✓ | ganha `--outputs N` (várias saídas falsas) e eventos de ponteiro com botão do meio e rodinha no `--script` | A |
| DRM/KMS atômico | ✗ | `backend/udev.rs`: `UdevBackend` + `DrmDevice` + `DrmCompositor` (planos primário, cursor e overlay; scanout direto; VRR) via `GbmAllocator`; features `backend_udev`, `backend_drm`, `backend_gbm`, `backend_session`, `backend_libinput`, `renderer_multi` atrás da feature cargo `session` | D |
| libinput | ✗ | `LibinputInputBackend` no `calloop`; mesmas rotas de `process_input`; config `[input]` (teclado XKB, repeat, touchpad: tap, natural scroll, aceleração; mouse) | D |
| libseat | ✗ | `LibSeatSession`; `--session --no-seat` só para VKMS em VM | D |
| troca de VT | ✗ | `SessionEvent::PauseSession` solta o master DRM, para o render e os timers; `ActivateSession` redesenha tudo (`Sched.full`) e devolve o ponteiro | D |
| suspend/resume | ✗ | pela mesma pausa/retomada + `org.freedesktop.login1` `PrepareForSleep` (zbus, já no `nkw-sys`): tranca (se `[session] lock_on_sleep`), devolve volumes, re-arma o relógio | D |
| hotplug de monitor | ✗ | `UdevEvent::Changed` → `DrmOutputManager`; saída nova entra como faceta (D11) ou pela `[[output]]` da config; saída que some devolve as telas ancoradas nela à faceta primária | D |
| multi-GPU | ✗ | `GpuManager<GbmGlesBackend<GlowRenderer>>`: renderiza na GPU primária, copia para as outras; híbridos (iGPU + dGPU) só com `[session] render_node` explícito até a fase E | D ◐ · E |
| perda do dispositivo DRM | ✗ | espera o `udev` re-adicionar; enquanto isso, nada é desenhado e nada é perdido (estado em memória) | D |
| entrada `.desktop` de sessão e unidades | ✗ | `nkw.desktop` em `wayland-sessions/`, `nkw-session` (importa o ambiente no `systemd --user`, sobe `nkw.service`, `nkw-shutdown.target`), como o niri faz | D |

### 1.2 Saídas (monitores)

| área | hoje | o que fazer | fase |
|---|---|---|---|
| uma saída (`wl_output` + `xdg_output`) | ✓ | — | — |
| várias saídas | ✗ | **D11**: um olho, N facetas; `role = "view"` para vista fixa; HUD e telefone na primária | D |
| escala e transformações | ◐ (`--scale`, fracionária) | por saída (`[[output]] scale`, `transform`), rotação de 90°/270° como faceta vertical | D |
| modo e taxa | ✗ | `[[output]] mode = "2560x1440@144"`; VRR `on \| off \| on_demand` (padrão `on_demand`: só com FOCO em tela cheia e `content-type` jogo/vídeo) | D |
| protocolo de gerenciamento de saídas (kanshi, wdisplays) | ✗ | `wlr-output-management-v1` à mão (o Smithay não tem): posição, modo, escala, transformação, ligar/desligar; `yaw_deg` da faceta é só nosso | D |
| `ext-foreign-toplevel-list` | ✗ | para barras e docks listarem janelas; cada tela é um toplevel; o título vem com a sala entre colchetes se `[ipc] room_in_title` | D |
| DPMS | ✗ | `[session] dpms_after_s` sobre `ext-idle-notify`; desliga CRTC; `nkw msg action power-off-monitors` / `power-on-monitors`; no mundo, "apagar as luzes" (a sala escurece antes de desligar, 400 ms) | D |

### 1.3 Camadas, bloqueio, ociosidade e captura

| área | hoje | o que fazer | fase |
|---|---|---|---|
| layer-shell (waybar, mako, fuzzel, swaybg) | ✗ | `wlr-layer-shell` pelo Smithay (`WlrLayerShellState`, `LayerMap`): **D14** — `top`/`overlay` compostos em 2D sobre o mundo com zona exclusiva respeitada pelo HUD; `background`/`bottom` viram "o lado de fora" (visto pelas janelas das salas); teclado exclusivo/on-demand pelas regras do protocolo | D |
| bloqueio de sessão (swaylock) | ✗ | `ext-session-lock-v1` (`SessionLockManagerState`): a superfície de bloqueio é composta 2D em toda saída; o passe do mundo para; o cofre tranca; sem superfície do cliente, uma tela creme própria com o relógio | D |
| ociosidade (swayidle) | ✗ | `ext-idle-notify-v1` (`IdleNotifierState`), alimentado por libinput; combina com o `idle-inhibit` dos clientes que já registramos; `[session] idle_after_s` → noite no mundo (as luzes baixam) | D |
| captura: screenshot e screencast | ✗ (só `--shot`) | `wlr-screencopy-v1` **e** `ext-image-copy-capture-v1`, à mão (como niri e cosmic-comp), com `block_out_from_capture` das regras e o cofre trancando antes; captura de **uma tela** = a textura da janela (exata, independe do 3D); ON AIR acende durante uma sessão de captura | D |
| backend de portal | ✗ | **D17**: `xdg-desktop-portal-wlr` (ScreenCast, Screenshot) + `xdg-desktop-portal-gtk` (FileChooser, Settings, Notification, AppChooser); `nkw-portals.conf` e `XDG_CURRENT_DESKTOP=nkw` só na sessão própria | D |
| gama / luz noturna (wlsunset, gammastep) | ✗ | `wlr-gamma-control-v1` à mão → LUT de gama do CRTC; o mundo já tem hora do dia, mas a gama vai no CRTC para o 1:1 continuar exato | D |
| screenshot UI própria | ✗ | `world.screenshot {output \| screen \| area}`: `screen` captura a textura da tela selecionada; `area` é um retângulo 2D sobre o quadro; salva em `~/Imagens/nekuroddo/` e copia; funciona aninhado | B |

### 1.4 Entrada, área de transferência, portais, notificações, cursor

| área | hoje | o que fazer | fase |
|---|---|---|---|
| `keyboard-shortcuts-inhibit` | ✗ | `KeyboardShortcutsInhibitState`: com inibidor ativo no FOCO, só `focus_escape` é interceptado (máquinas virtuais, área de trabalho remota); aninhado, pelo backend SCTK, para o host nos deixar usar `super` (critério de D4) | D (B para o SCTK, se bater o critério) |
| IME: `input-method-v2` + `text-input-v3` (fcitx5, IBus) | ◐ (`--text-input` sem IME) | sessão própria: `InputMethodManagerState` + `TextInputManagerState` + `VirtualKeyboardManagerState`; a janela de candidatos é um popup 2D ancorado ao cursor da janela em FOCO; aninhado: ponte host→cliente pelo SCTK, depois | D · E (aninhado) |
| `data-control` (cliphist, wl-clipboard) | ◐ (ponte com o host como cliente) | servidor `ext-data-control-v1` + `wlr-data-control-v1` (ambos no Smithay); a ponte aninhada continua | D |
| arrastar e soltar entre janelas | ◐ (Smithay, não testado) | **DnD atravessando o mundo**: o arrasto que sai do retângulo em FOCO recua a câmera, o ícone segue o ponteiro no 3D, entrar em outra tela entrega a oferta, soltar no chão vira **nota** (texto) ou **livro** (uri-list) na mesa de leitura | E |
| seletor de arquivo (portal) | ◐ (abre fora, aninhado) | sessão própria: o diálogo do `-gtk` é uma janela normal, filha pela `xdg-foreign` → vira diálogo na mesa da mãe | D |
| agente polkit | ✗ | `[[spawn]]` na partida (ex.: `polkit-gnome-authentication-agent-1`); o diálogo é janela comum | D |
| notificações | ◐ (plano: best-effort) | daemon `org.freedesktop.Notifications` próprio → gatos carteiros; `[notifications] daemon = "own" \| "external"`; `own` é o padrão só na sessão própria | D |
| Settings do portal (tema escuro, animações) | ◐ (lê `enable-animations`) | lê também `color-scheme` (noite no mundo quando escuro, se `[render] follow_color_scheme`); exporta nada (o `-gtk` responde pelo gsettings) | B |
| temas de cursor | ✗ (cursor do host) | sessão própria: XCursor (`XCURSOR_THEME`/`SIZE`), plano de cursor por hardware, formas do `cursor-shape` mapeadas para nomes do tema, imagens próprias dos clientes; no NAV/ANDAR, mira em vez de cursor quando o ponteiro está travado | D |
| gestos de touchpad | ✗ | `pointer-gestures` repassado ao cliente em FOCO; no mundo: 3 dedos horizontal = `h`/`l` (deslize que encaixa), vertical = `J`/`K`; pinça = ARRUMAR na tela selecionada (afastar/aproximar); respeita movimento reduzido | E |
| tablet | ◐ (seat para `cursor-shape`) | `TabletManagerState` + libinput tablet no FOCO; fora do FOCO, a caneta aponta como mouse | E |
| acessibilidade | ◐ (teclado total, movimento reduzido, alto contraste, escala de fonte, giro em passos) | mantém; soma: guias de ímã em alto contraste, tamanho do cursor (`[input] cursor_size`), tempo de pressão longa configurável, "sem arrasto" (`[mouse] drag = false`: tudo por teclado), toque não entra nesta fase | A · D |

### 1.5 Recursos de WM frente ao niri

| recurso do niri | hoje | o que fazer | fase |
|---|---|---|---|
| layout de rolagem em colunas | ✓ (faixa de mesas) | — | — |
| presets de largura, consume/expel, maximizar, tela cheia | ✓ | `shift+-`/`shift+=` para presets de altura das telas no ar (§2.4) | A |
| colunas em abas | ✗ | **mesa fechada** (`world.desk_tabbed`): a mesa mostra uma tela por vez com marcadores de aba no aro; `j`/`k` trocam; persistido | B |
| janelas flutuantes | ◐ (telas no ar, diálogos) | §2 inteiro: arrastar, redimensionar, ancorar; regra `open = "air" \| "dock"` | A |
| regras de janela | ◐ (`app_id`, sala, mesa, largura) | v2: `title` (regex), `open = desk \| air \| dock`, `size`, `scale`, `opacity`, `fullscreen`, `tabbed`, `block_out_from_capture`, `follow_cat` (a janela de um agente na mesa dele, já planejado) | B |
| workspaces nomeados | ✓ (salas) | criar, renomear, apagar vazia pelo telefone e pelo `nkw msg`; `[[room]]` fixas no `world.toml` | B |
| workspaces por saída | — | não existe no modelo: a sala é o lugar, o monitor é uma janela para ele (D11) | — |
| overview | ◐ (mapa `o`) | o mapa ganha miniaturas vivas (≤ 4 Hz) e arrasto: puxar uma tela de uma mesa para outra ou para outra sala | B |
| screenshot UI | ✗ | `world.screenshot` (§1.3) | B |
| config + recarga a quente | ◐ (lido na partida; `world.walk_set` ao vivo) | `inotify` em `world.toml` e `ui.toml`; validação completa antes de aplicar; erro = toast e config antiga continua | B |
| IPC | ✗ (`--script`, `dump`) | **D16**: `nkw msg` por socket JSON, consultas, ações e `event-stream` | B |
| spawn na partida | ✗ | `[[spawn]] argv = [...]` com `room`, `open`; roda uma vez por sessão | B |
| mover/redimensionar interativo com o mouse | ✗ | §2.3 e §2.4 | A |
| foco segue o mouse / warp | ✗ | `[mouse] hover_selects = true`: passar por cima seleciona a tela da estação (nunca muda de estação sozinho); `click_focuses = "double" \| "single"` (padrão `double`: 1 clique seleciona, 2 entra no FOCO); `warp_to_focus` leva o ponteiro ao centro da janela ao entrar no FOCO | B |
| urgência | ◐ (ativação sem token → carteiro) | o aro da tela pisca em âmbar (`urgent`), `a`/`A` incluem telas urgentes, `nkw msg` emite `screen.urgent` | B |
| animações configuráveis | ◐ (constantes, movimento reduzido) | `[animations] speed = 1.0`, `off = false`, por família (`glide`, `arrange`, `ui`); as curvas não mudam (§3 dos objetos) | B |
| ferramentas de depuração | ◐ (`NKW_STATS`, `dump`, `--gl-debug`) | `nkw msg debug {damage \| lod \| aabb \| frametime} on/off`, sobreposição de tempo de quadro no HUD, `--flat` (modo plano) | B |
| xwayland | ✓ (satellite) | mantém; na sessão, `[launch] xwayland = "on"` por padrão | — |

## 2. Telas no espaço

O que o usuário pediu: *criar telas e arrastá-las pelo ar*. Esta seção especifica isso por completo, teclado e
mouse, apoiada no que já existe: `Station::Air`, `persist::Air {pos, yaw, wall}`, `carry.rs` (`hand_pose`,
`drop_target`), `collide::ray_hit_quad`, `onetoone.rs`, as intenções de lugar (`take_intent`) e o `--script`
com ponteiro sintético.

### 2.1 Vocabulário e modelo

| termo | o quê | código |
|---|---|---|
| pose de tela | posição (m), `yaw`, `pitch` (±25°, novo), `scale` (0,5–3×, novo); nunca roll | `Air { pos_mm, yaw_mdeg, pitch_mdeg, scale_pct, wall, cluster }` |
| moldura vazia | uma tela sem janela, no ar ou numa mesa; a próxima janela sem lugar nasce nela | `Placeholder { id, pose, armed }` |
| tela ancorada | presa à vista (segue você), num encaixe do HUD; a 1:1 é composta em 2D exato | `Dock { screen, slot: Corner \| Side, scale }` |
| cluster | grupo rígido de telas no ar que se move junto e é uma estação só | `Cluster { id, name, screens }` |
| arranjo | poses salvas por `app_id` (e `title` opcional) de uma sala, com lista de `spawn` opcional | `layouts/<name>.json` |
| ARRUMAR | sobreposição de modo (como SALTO) para mover a tela selecionada pelo teclado | `Kind::Arrange` |
| ímã | alvo de encaixe com tolerância: parede, slot de mesa, borda de outra tela, grade | `magnet.rs` |

Regras que valem sempre:

- **A câmera nunca se move por causa de um arrasto.** Mover uma tela é mover a tela. Ir até ela é `h`/`l`,
  SALTO ou ANDAR, como hoje.
- **A escala e o tamanho são coisas diferentes.** Tamanho é `configure` do cliente (1 px = 1 mm). Escala é
  só visual no 3D, e o 1:1 a leva em conta: `d = scale · 0,001 · f / s`.
- **Toda operação de layout é desfazível** (`u`), menos fechar uma janela.
- **Tudo tem caminho por teclado**, e `[mouse] drag = false` desliga o arrasto para quem não o quer.

### 2.2 Criar telas

Hoje uma janela nova entra numa mesa nova à direita, ou onde uma intenção ou regra manda. O novo é criar
**onde você mira**:

| como | o quê acontece |
|---|---|
| `N` no NAV ou no ANDAR (`world.new_screen`) | uma **moldura vazia** aparece onde a mira cruza o mundo (parede, mesa ou ar a 1,2 m, o primeiro que servir), virada para você; a paleta abre já nos apps com o título "nova tela: …"; o app escolhido nasce nela (intenção de lugar com o token de ativação do lançamento e o `pid`). `esc` na paleta deixa a moldura armada |
| `shift+N` (`world.placeholder`) | só a moldura, sem paleta: "a próxima janela vai aqui". A mais recente armada ganha; uma regra explícita de `[[rules]]` ganha da moldura |
| botão do meio em espaço vazio (parede, chão, ar) | o mesmo que `N` naquele ponto |
| botão direito em espaço vazio | um **balão de menu** (reusa a caixa de escolhas da CONVERSA): "nova tela aqui", "terminal aqui", "nota aqui", "colar aqui" (uri-list → livro, texto → nota); `j`/`k`/`enter`/`esc` |
| telefone › Apps › `a` | abre o app **no ar**, à sua frente, em vez de numa mesa |
| paleta: `nova tela <app>` | idem ao `N`, a partir de qualquer modo |
| `T` / `world+enter` | continua: terminal numa mesa nova à direita. `shift+T`: terminal no ar, na mira |

A moldura vazia é um objeto do mundo: uma moldura creme com "…" pulsando devagar (0,5 Hz, para, com movimento
reduzido). É estação (`h`/`l` chegam nela; `enter` abre a paleta nela), aparece no SALTO, persiste no
`world.json`, e `Q` a remove. Quando a janela chega, a moldura some com a janela crescendo de dentro (180 ms,
ease-out cúbico). Uma janela cujo `configure` o cliente ignora fica centrada e cortada na moldura, como no
FOCO hoje.

### 2.3 Arrastar com o mouse

**Onde pegar.** No NAV e no ANDAR, qualquer ponto da tela (aro ou conteúdo): nenhum cliente recebe o ponteiro
fora do FOCO, então não há conflito. No FOCO, o conteúdo é do cliente; pega-se pelo **aro** visível em volta
do retângulo 1:1 (as margens) ou com **`world+botão esquerdo`** em qualquer lugar, como `Mod+arrastar` em
todo WM. Começar um arrasto no FOCO sai do FOCO: a câmera recua para a estação (≤ 260 ms) e a tela já vem
pela mão.

**Clique ou arrasto.** Um pressionar seguido de movimento abaixo de `[mouse] drag_threshold_px = 6` é
clique (seleciona; no já selecionado, FOCO conforme `click_focuses`). Acima, é arrasto. A tela "levanta"
3 cm na sua direção e a sombra-bolha cresce (80 ms).

**O plano do arrasto.** A tela se move no **plano paralelo à imagem que passa pelo centro dela**: o raio do
ponteiro cruza esse plano e o centro vai para lá. Profundidade constante, até você mudar:

| gesto | efeito |
|---|---|
| mover | translada no plano da vista |
| rodinha / `[` `]` | profundidade, ±0,2 m por passo, 0,5 a 6 m, no máximo 1 m/s (conforto) |
| `shift` segurado | **encaixe forte**: grade de 25 cm, `yaw` em passos de 15°, alinhamento com paredes e slots |
| `alt` segurado | **girar**: horizontal = `yaw`, vertical = `pitch` (±25°); sem `alt`, a tela fica virada para o olho ("billboard") e, ao soltar, mantém a direção que tinha para o olho |
| `ctrl` segurado | sem ímã nenhum |
| `ctrl` + rodinha | escala visual, 0,5 a 3×, passos de 10 % |
| `esc` ou botão direito durante o arrasto | cancela: a tela volta à pose de origem (≤ 200 ms) |
| soltar | `drop_target` (§2.8): slot de mesa, parede, encaixe do HUD (ancorar), outra tela (cluster, só com `shift`) ou o ar |

**Captura.** Durante o arrasto o mundo é dono do ponteiro: nenhum cliente recebe movimento; o cursor vira
`grabbing`. O botão solto fora da janela do host (aninhado) ou a janela do mundo perdendo o foco encerram o
arrasto como `soltar`. Botões presos nunca sobram: ao trocar de modo, o hub solta tudo o que pressionou,
como já faz com as teclas.

**Enquanto arrasta, guias** (2D, pixel inteiro, como os crachás): uma linha tracejada até o chão com a altura
("1,42 m"), a profundidade ("1,2 m"), o fantasma do alvo de encaixe (o contorno do slot da mesa, a sombra na
parede, o encaixe do HUD) e o **anel 1:1**: quando a tela está exatamente à distância de leitura da saída, o
aro ganha um anel fino e o rótulo diz "1:1". É o jeito de pôr uma referência legível no ar sem entrar no FOCO
(e a composição 2D exata entra sozinha, §2.7).

**No ANDAR**, tudo isso vale com o ponteiro **destravado**: `alt` (toque, não segurado) alterna entre
mouse-look e ponteiro livre, e o HUD mostra qual está ativo. O `g` (carregar) continua sendo a forma de
**andar com a tela**: ela flutua à sua frente enquanto você anda, e o arrasto é a forma de **ficar parado e
mover a tela**. As duas usam as mesmas regras de soltar.

### 2.4 Redimensionar em 3D

- **Mouse**: alças nos quatro cantos (24 px em espaço de tela) e nas bordas (12 px). Arrastar um canto muda
  largura e altura; uma borda, só um eixo; `shift` trava a proporção; `world+botão direito` em qualquer ponto
  redimensiona pelo canto mais próximo (como `Mod+botão direito` nos WMs).
- **O que muda**: o tamanho lógico do cliente. A projeção do novo retângulo na profundidade da tela vira
  milímetros (1 px = 1 mm). Limites: mínimo 200×150 lógicos, máximo o útil da saída a 1:1, e os `min_size`/
  `max_size` do `xdg_toplevel`. Encaixe em presets quando a 16 px: largura 1/3, 1/2, 2/3, 1 e altura 1/2,
  2/3, 1 do útil.
- **`configure` ao vivo**, no máximo 10 por segundo durante o arrasto, e um final ao soltar (clientes GTK e
  navegadores redesenham caro). A tela mostra o tamanho ("1280 × 800") enquanto redimensiona. Até o cliente
  comitar o tamanho novo, o quad usa a textura antiga esticada só dentro da moldura, como no niri.
- **Teclado**: `-`/`=` presets de largura (existe), `shift+-`/`shift+=` presets de altura (novo), `1`…`4` no
  ARRUMAR escolhem a largura 1/3, 1/2, 2/3, 1 direto, `0` leva a tela à distância 1:1 ("ponto de leitura").
- **Escala** (visual) é `ctrl+rodinha` ou `ctrl+-`/`ctrl+=`; `ctrl+0` volta a 1×. Uma "TV" grande e longe é
  escala 2× a 4 m, com a janela ainda em 1280×720.

### 2.5 Teclado: o modo ARRUMAR

`v` no NAV (ou `world.arrange` pela paleta) sobre a tela selecionada; `v` também no ANDAR sobre a tela na
mira. O HUD mostra `ARRUMAR` e uma legenda de uma linha. Teclas (modo `world.arrange` do keymap, religáveis):

| tecla | comando | o quê |
|---|---|---|
| `h` `j` `k` `l` / setas | `world.nudge_left` / `_down` / `_up` / `_right` | move 10 cm no plano da vista (`shift`: 50 cm; `ctrl`: 1 cm) |
| `[` `]` | `world.nearer` / `world.farther` | profundidade ±20 cm |
| `q` `e` | `world.yaw_left` / `world.yaw_right` | `yaw` ±15° (`ctrl`: ±5°) |
| `shift+k` `shift+j` | `world.pitch_up` / `world.pitch_down` | `pitch` ±5°, até ±25° |
| `-` `=` · `shift+-` `shift+=` | `world.narrower` / `world.wider` · `world.shorter` / `world.taller` | presets de largura · de altura |
| `1` `2` `3` `4` | `world.size_preset {n}` | largura 1/3, 1/2, 2/3, 1 |
| `0` | `world.read_distance` | leva à distância 1:1 e vira para o olho |
| `ctrl+-` `ctrl+=` `ctrl+0` | `world.scale_down` / `world.scale_up` / `world.scale_reset` | escala visual |
| `g` | `world.snap_toggle` | liga/desliga o encaixe forte (grade) neste arrasto |
| `w` · `d` · `a` | `world.to_wall` / `world.to_desk` / `world.to_air` | encosta na parede atrás · entra no slot de mesa mais perto · fica no ar aqui |
| `p` | `world.dock` | ancora na vista (§2.7); de novo desancora |
| `space` | `world.select_toggle` | marca/desmarca esta tela na seleção múltipla; `h`/`l` com `space` segurado pulam de tela em tela sem mover |
| `G` · `shift+G` | `world.cluster` / `world.uncluster` | agrupa a seleção · desfaz o grupo |
| `s` | `world.layout_save` | salva o arranjo da sala (pede o nome na linha) |
| `u` · `ctrl+r` | `world.undo` / `world.redo` | desfaz · refaz (também fora do ARRUMAR, no NAV) |
| `enter` | `world.arrange_done` | confirma e volta ao modo de origem |
| `esc` | `world.arrange_cancel` | desfaz tudo desde que entrou e volta |

Cada toque anima ≤ 120 ms (ease-out quadrático, como o manípulo da vitrola); com movimento reduzido, corta.
As guias do §2.3 aparecem iguais. Sair do ARRUMAR com a tela sobreposta a outra em mais de 10 % a empurra
para o lugar livre mais perto no mesmo plano (≤ 200 ms) — a "física" de §2.10.

### 2.6 Seleção múltipla, clusters e arranjos

- **Selecionar**: `space` no ARRUMAR; `shift+clique` no NAV soma; arrastar a partir de espaço vazio no NAV
  desenha uma **laçada** em espaço de tela e seleciona as telas cujo centro cai nela. A seleção aparece como
  anel creme em cada tela e um contador no HUD ("3 telas").
- **Mover junto**: com várias selecionadas, arrastar (mouse ou `hjkl`) move todas rigidamente; `alt` gira o
  conjunto em volta do centro dele. Soltar aplica os ímãs ao conjunto, não a cada uma.
- **Cluster** (`G`): a seleção vira um grupo com nome (padrão: os `app_id`, "firefox + foot"). Um cluster é
  **uma estação** (`Station::Cluster`): `h`/`l` param diante do conjunto inteiro, `j`/`k` percorrem as telas
  dele, `enter` dá FOCO na atual; arrastar qualquer membro move todos; `shift+G` desfaz. Persistido.
- **Arranjo** (`s`, `world.layout_save {name}`): salva, por sala, as poses das telas no ar, ancoradas e dos
  clusters, por `app_id` (+ `title` regex opcional), mais a largura das mesas. `world.layout_load {name}`
  faz as telas presentes deslizarem para as poses (≤ 260 ms, escalonadas 30 ms entre si; corte com movimento
  reduzido). Telas que não casam ficam onde estão. Um arranjo pode ter `spawn = [["firefox"], ["foot","-e","htop"]]`
  para abrir o que falta — é a "cabine": arranjo + apps. Ficam em `~/.local/state/nekuroddo/layouts/<name>.json`
  (`{"version": 1, …}`), listados no telefone (app **Arranjos**: carregar, salvar, apagar, renomear) e na
  paleta (`arranjo <name>`). A **pergunta 3** (§7) decide se o `spawn` entra já na fase A.

### 2.7 Telas que seguem você (ancoradas)

`p` no ARRUMAR, `world.dock`, ou soltar uma tela sobre um **encaixe do HUD** (as bordas e os cantos da saída
ficam destacados durante o arrasto) ancoram a tela à vista. Ela deixa de estar no mundo e passa a estar na
**sua** vista, num de 8 encaixes (4 cantos, 4 lados), com escala `[dock] scale = 0.33` por padrão (uma
imagem-em-imagem) ou 1× (a tela inteira, legível).

- **A 1:1 ela é composta em 2D exato** pelo caminho do Smithay, como a janela em FOCO, mas **sem foco de
  teclado**. É o "segundo monitor" num monitor só: referência legível enquanto você digita em outra tela.
  Várias ancoradas a 1× dividem o útil da saída pelos encaixes escolhidos (esquerda/direita = metades).
- No **FOCO**, as ancoradas continuam visíveis e vivas (não escurecem; LOD "estação atual"); a janela em
  FOCO fica no útil restante ou por cima delas, conforme `[dock] focus = "beside" \| "over"` (padrão `beside`
  quando couber a 1:1, senão `over`).
- São estações (`Station::Dock(slot)`): aparecem no SALTO sempre; `enter` dá FOCO em cima delas, sem
  deslize. `world+h/l` no FOCO também passam por elas.
- **No ANDAR** ficam (é o ponto: seguem você), em 0,33× por padrão; `[dock] hide_while_walking = true` as
  esconde para quem prefere a vista limpa.
- Limite `[dock] max = 4`. Persistidas por saída (faceta); uma saída que some devolve as suas à primária.
- Conforto: fixas à vista, não têm vecção; a mudança de escala anima 120 ms; nenhuma ancorada pode cobrir o
  rótulo do modo nem os selos (o HUD tem prioridade no mesmo canto; a ancorada desce).

### 2.8 Ímãs e guias

`nkw-core/src/magnet.rs`, puro: `snap(pose, size, &Targets, Strength) -> (Pose, Vec<Guide>)`.

| alvo | tolerância padrão (suave) | com `shift` (forte) | guia |
|---|---|---|---|
| parede da sala (plano) | 8 cm, 8° | encosta a 5 cm, `yaw` da parede | sombra retangular na parede |
| slot de mesa (`DeskSlot`, já existe) | 0,6 m do centro do slot | idem | contorno do slot |
| borda de outra tela no ar (co-planar, borda com borda, mesmo centro) | 2 cm, 5° | 4 cm, grade | linha fina entre as bordas; "alinhado" |
| grade | — | 25 cm e 15° | pontos da grade no plano, só perto da tela |
| altura do olho | 3 cm | idem | linha do horizonte tracejada |
| distância 1:1 | 2 cm | idem | o anel 1:1 |
| encaixe do HUD (ancorar) | bordas da saída, 48 px | idem | o encaixe acende |
| outra tela (cluster) | só com `shift`, soltar em cima | — | as duas ganham o anel de grupo |

`ctrl` desliga todos. A força do ímã é só na soltura e no `g` do ARRUMAR: enquanto você arrasta, a tela segue
o ponteiro e o **fantasma** mostra onde ela vai parar. Isso evita o "puxão" que irrita em editores.

### 2.9 Desfazer

`nkw-core/src/history.rs`: um anel de `[arrange] history = 50` operações (`Move`, `Resize`, `Scale`, `Dock`,
`Undock`, `Cluster`, `Uncluster`, `LayoutLoad` com as poses anteriores, `Pin`, `ToDesk`, `ToWall`,
`Placeholder`). `u` desfaz, `ctrl+r` refaz, com um toast curto ("desfeito: mover firefox"). Um arrasto inteiro
é uma operação; dez `h` seguidos no ARRUMAR em menos de 1 s são uma. Fechar janela não entra (o cliente se
foi). A história é por sessão, não persiste.

### 2.10 Sensação (colisão suave)

Sem física de verdade; só o que dá a impressão de coisas com corpo, com as curvas do §3.1 dos objetos:

- **Levantar** ao começar o arrasto (3 cm, sombra maior, 80 ms) e **assentar** ao soltar (mola leve: 3 % de
  excesso, 180 ms; sem mola com movimento reduzido).
- **Não atravessar**: duas telas no ar não ocupam o mesmo volume; a solta sobre outra sem `shift` desliza até
  o lugar livre mais perto no mesmo plano (≤ 200 ms). Telas não entram em paredes nem em móveis (`Aabb`
  dos obstáculos, já no `collide.rs`): encostam.
- **Peso** só visual: telas grandes levantam menos (2 cm) e assentam mais devagar (220 ms).
- **Som** (quando `[sound]` estiver ligado): um "toc" de madeira ao assentar numa mesa, um "tac" na parede,
  nada no ar; o mixer do `nkw-objects` já tem os cliques.

### 2.11 Estações, FOCO de qualquer pose, persistência

- **Estações.** As telas no ar já são `Station::Air`, ordenadas na sala pela posição projetada no eixo dela.
  Entram `Station::Cluster(id)`, `Station::Dock(slot)` e `Station::Placeholder(id)`, com a mesma ordem.
  Uma tela ancorada não conta na ordem da sala (ela está em você).
- **FOCO de qualquer orientação.** `onetoone.rs` ganha `pitch` e `scale`: a pose 1:1 fica sobre a normal da
  tela, à distância `scale · 0,001 · f / s`, com o pitch da tela (≤ 25°) e yaw dela; o ajuste submilimétrico
  ao pixel continua; teste: cantos a menos de 0,01 px do retângulo 2D para uma grade de `yaw × pitch × scale`.
  Se a pose cair dentro de um móvel ou parede (tela encostada), a câmera recua até o ponto livre mais perto na
  normal e o 2D continua exato (a troca quad→2D independe do caminho).
- **Persistência** (`world.json` v2, com `persist::migrate` v1→v2): `Air` ganha `pitch_mdeg`, `scale_pct`,
  `cluster`; `DockRec { app_id, output, slot, scale }`; `PlaceholderRec { pose, armed }`; `ClusterRec { name,
  app_ids }`; `desk.tabbed`. Arranjos em `layouts/`. Escrita atômica com o debounce de 2 s, como hoje.
- **Memória de lugar** continua por `app_id` e passa a lembrar também pose no ar, ancoragem e escala.

### 2.12 Conforto e acessibilidade

- A câmera não se move num arrasto; a profundidade muda a no máximo 1 m/s; nada gira a câmera para seguir a
  tela (`[mouse] edge_turn = false` por padrão; ligado, encostar o ponteiro na borda por 300 ms gira devagar,
  40°/s, e para ao sair da borda).
- Telas nunca ficam a menos de 0,5 m do olho (uma tela colada na cara enjoa); a mínima vale também para
  arranjos carregados.
- Toda animação ≤ 260 ms, cortada com movimento reduzido; a mola de assentar some com movimento reduzido.
- Guias em alto contraste (tinta escura sobre creme) com a preferência já existente; o anel 1:1 também vira
  rótulo textual.
- Tudo por teclado (ARRUMAR); `[mouse] drag = false` desliga o arrasto e `hover_selects = false` o hover.
- O `esc` cancela em qualquer ponto e devolve a pose de origem; nada é destrutivo além de `Q`.

### 2.13 O que muda no código

| crate | novo / muda | o quê |
|---|---|---|
| `nkw-core` | `arrange.rs` (novo) | `ArrangeOp`, movimento no plano da vista (unprojeção + interseção com o plano), limites de profundidade/pitch/escala, redimensionar → tamanho lógico com limites e presets, laçada em espaço de tela |
| `nkw-core` | `magnet.rs`, `history.rs` (novos) | §2.8, §2.9 |
| `nkw-core` | `mode.rs` | `Kind::Arrange`, `Action::{Arrange, Nudge, …}`, `Effect::{ConfigureLive, DockChanged, …}`, seleção múltipla; `Pick::Placeholder`, `Pick::Dock` |
| `nkw-core` | `strip.rs`, `onetoone.rs`, `persist.rs`, `carry.rs` | estações novas; pitch e escala no 1:1; v2 do arquivo; `drop_target` ganha HUD e cluster |
| `nkw` | `wm/pointer.rs` (novo), `input.rs` | a máquina do ponteiro (§4, D15): `Idle → Hover → Pressed → Drag{Move, Resize, Rotate, Lasso} → Idle`, captura, modificadores, cursor |
| `nkw` | `wm/world_ops.rs`, `world.rs` | comandos `world.*` novos; molduras; clusters; ancoradas; arranjos (`wm/layouts.rs`, novo) |
| `nkw` | `frame.rs`, `render.rs` | composição 2D exata das ancoradas a 1:1; LOD das ancoradas; o FOCO ao lado delas |
| `nkw-gl` / `overlay` | `draw.rs`, `overlay/guides.rs` (novo) | levantar/sombra, moldura vazia (malha), guias, anel 1:1, laçada, encaixes do HUD |
| `nkw` | `backend/script_input.rs`, `script.rs` | `pointer`, `button middle`, `scroll`, `drag x y → x2 y2 [mods]`, `expect pose` para os testes headless |
| core (Go) | `internal/app/world.go` | os comandos `world.*` novos no registro (`scope: client`), modo `world.arrange` no keymap, teclas padrão |

## 3. Outras coisas desta fase

Critério: entra o que é WM ou o que o 3D dá quase de graça em cima do §2. Fica de fora o que é conteúdo.

### 3.1 DECORAR (fase C)

O mesmo arrasto do §2, para **móveis e objetos**: vitrola, relógio, planta, pote, lareira, mesas vazias,
estantes decorativas. `world.decorate` (paleta, ou telefone › Decorar) entra no modo `DECORAR`, onde o SALTO
e o clique pegam objetos em vez de telas; grade de 25 cm sempre (é AC); `q`/`e` giram 90°; `enter` assenta,
`esc` cancela. Os objetos têm `Aabb`, então a colisão e a acústica os seguem de graça (`Plugin::obstacles`).
Posições vão para `world.json` (`[objects]` planejado nos objetos, agora com pose). **Papel de parede e
piso por sala** vêm de um catálogo pequeno (os padrões de 32–64 px que já existem, mais 6 novos), escolhidos
no telefone › Decorar › Sala. Justificativa: reaproveita 90 % do §2, dá ao usuário o que o AC dá, e mantém
as regras de proximidade dos objetos (o lugar deles passa a ser escolha sua). Fora: comprar/colecionar,
inventário, móveis novos além dos que já existem.

### 3.2 Notas (fase C)

Um objeto `Note`: um papel colado numa parede, numa mesa ou no aro de uma tela, com até 280 caracteres,
cor por escolha (4), tipografia da interface, rasterizado pelo mesmo caminho dos crachás (2D em pixel
inteiro de perto, textura de longe). Cria-se com `world.note {text}` (a linha de texto pede o texto), pelo
balão do botão direito ("nota aqui") ou soltando texto no mundo (DnD, fase E). `enter` edita, `Q` apaga,
arrastar move (é o §2). Até 64 por sala, só no `world.json`, nunca no core nem nos agentes. Justificativa:
é o "post-it do monitor", custa pouco (rasterizador e arrasto prontos) e faz o lugar parecer seu. Fora:
quadro branco/desenho à mão, markdown, sincronização com os todos do core.

### 3.3 Salas pelo telefone e pela IPC (fase B)

Telefone › Mapa: `n` sala nova (pede nome), `r` renomeia, `d` apaga uma sala vazia (as com gatos ou telas
recusam com toast), `h`/`l` reordenam. `[[room]]` no `world.toml` fixa salas que sempre existem (`name`,
`wallpaper`, `floor`), e `nkw msg action room-new name=…` faz o mesmo de fora. É a paridade com workspaces
nomeados do niri; custa um dia.

### 3.4 Modo foto (fase C)

`world.photo`: a câmera se solta do olho (órbita em volta do ponto de mira, `hjkl` giram, `[`/`]` aproximam,
FOV só aqui de 40° a 90°), o HUD some, `enter` tira a foto (PNG em `~/Imagens/nekuroddo/`), `esc` volta ao
olho com corte. O cofre tranca antes (regra do `--shot`) e `[photo] blur_screens = false` pixela o conteúdo
das telas para compartilhar sem vazar. É o `--shot` com câmera livre: duas tardes.

### 3.5 O que fica de fora, e por quê

| ideia | motivo |
|---|---|
| "mesa do dia" (uma vista que junta o que importa hoje) | é conteúdo e agregação de dados do core, não WM; o quadro de avisos e a inbox já cobrem; revisitar com os todos |
| quadro branco com desenho | precisa de tablet/caneta decentes e de um rasterizador de traço; as notas bastam agora |
| conteúdo 3D de clientes, VR, multiplayer | não-objetivos do plano original, continuam |
| toque | sem hardware para testar; a sessão própria o deixa pronto no libinput, mas o mundo não o liga |
| móveis novos / catálogo grande | arte cara; DECORAR usa o que existe |

## 4. Decisões de arquitetura

Numeração continua a do plano original (D1–D10) e do adendo.

### D11 — Multimonitor: um olho, várias facetas; vistas fixas por escolha

**Decisão.** Todas as saídas ligadas são **facetas de uma única câmera**: a mesma posição do olho, com um
`yaw` por faceta derivado da **disposição física** informada pelo layout de saídas (duas saídas lado a lado =
duas facetas com `yaw` ±(FOV/2), como vidros de uma baia; uma vertical = faceta girada). Cada faceta é uma
projeção própria, com o seu tamanho, escala e taxa. A HUD, o telefone, a caixa de diálogo e a paleta ficam na
**primária** (`[[output]] primary = true`, ou a primeira); nas outras, só selos e crachás. `[[output]] role =
"view"` tira uma saída da baia e a faz **vista fixa**: `view = "map"`, `view = "station:<sala>/<mesa>"` ou
`view = "room:<sala>"`, re-renderizada só com dano (≤ 4 Hz), para um monitor vertical mostrar a sala dos
gatos ou o mapa enquanto você trabalha na primária.

**Como se comportam as coisas.**

- **FOCO**: o 1:1 acontece na faceta onde a tela está (a de menor ângulo até a normal dela). Só essa faceta
  congela e escurece; as outras seguem vivas no LOD. Telas ancoradas a 1:1 em outra faceta ficam em 2D exato
  (§2.7). `world.send_output {n}` (`world+shift+1..9`) põe a tela no ar à distância 1:1 no centro da faceta n;
  é o "mandar para o monitor 2".
- **Rolagem**: `h`/`l` são laterais e cruzam facetas sem efeito especial (é a mesma sala).
- **Hotplug**: faceta nova entra à direita com `yaw` calculado; se o layout do `wlr-output-management` a
  puser à esquerda, à esquerda. Faceta que some devolve as ancoradas à primária.
- **Custo**: um passe do mundo por faceta (uploads de textura uma vez por quadro; culling por frustum de cada
  faceta). Orçamento em §5.1.

**Por quê.** Em primeira pessoa, "workspaces por monitor" não têm significado: o lugar é um só e o monitor é
um vidro. Facetas dão o que dois monitores dão de melhor (mais campo de visão, referência ao lado) sem duas
câmeras brigando. Vista fixa cobre o caso "monitor vertical de status". **Descartadas**: *uma câmera por
monitor* (dois "você" no mesmo lugar; o FOCO e o ANDAR ficam ambíguos); *salas por monitor* (quebra a planta e
o corredor; é o modelo do niri porque monitores são planos separados, não é o nosso).

### D12 — Um binário, um trait de backend, a sessão atrás de uma feature

**Decisão.** `nkw` (aninhado), `nkw --headless` e `nkw --session` são **o mesmo binário**. O `Host` de hoje
(`Winit | Headless`) vira o trait `Backend`:

```
trait Backend {
    fn outputs(&self) -> &[OutputInfo];             // nome, modo, escala, transformação, posição, vrr
    fn render(&mut self, out: OutputId, elems: &[Elem]) -> RenderResult;  // DrmCompositor decide planos e scanout
    fn present(&mut self, out: OutputId, damage) -> PresentToken;        // page-flip / RedrawRequested / vblank falso
    fn cursor(&mut self, img: CursorImage) -> CursorPlane;              // plano por hardware, host, ou nenhum
    fn set_vrr(&mut self, out, on); fn set_dpms(&mut self, out, on); fn set_gamma(&mut self, out, lut);
    fn pause(&mut self); fn resume(&mut self);     // VT, suspend
}
```

com `backend/winit.rs`, `backend/headless.rs` e `backend/udev.rs`. A entrada entra por `process_input`
como hoje (winit, script, libinput). A feature cargo `session` liga `backend_udev`, `backend_drm`,
`backend_gbm`, `backend_libinput`, `backend_session` e `renderer_multi` do Smithay, além de `xcursor`;
sem ela o binário aninhado continua com o tamanho de hoje. O `--session` sem a feature falha com mensagem
clara. **Por quê**: um binário evita dois lançadores, duas configs e divergência; a feature evita pagar
libseat/libinput no build aninhado. **Descartada**: binário `nkw-session` separado (duplica a cola).

### D13 — Nunca tela preta

Cinco camadas, da mais barata à última:

1. **Modo plano** (`--flat`, e automático): se o `WorldRenderer` falhar ao subir ou perder o contexto
   (`gfx.world` já é `Option` por isso), o mundo vira um **compositor 2D de rolagem**: as mesas são colunas,
   a pilha é a coluna, o HUD vira uma barra. Só o caminho 2D do Smithay. É a prova de que o WM é o
   `nkw-core`, e serve também como opção para máquinas fracas e como acessibilidade.
2. **Hook de panic** grava `world.json` (poses, estação) e `crash-<ts>.json` (modo, últimas 200 linhas de
   log, versão) antes de repanicar; o `nkw-objects` devolve os volumes (já previsto).
3. **Reinício**: `nkw.service` com `Restart=on-failure`, `RestartSec=1`, `StartLimitBurst=3` em 2 min; a
   segunda partida seguida em menos de 2 min sobe em `--safe` (sem plugins de sistema, sem som, luz fixa,
   modo plano se o crash foi no render).
4. **Queda para o greeter**: esgotado o limite, `nkw-shutdown.target` encerra a sessão e o greeter volta; a
   sessão niri continua instalada e é a padrão até o go/no-go.
5. **Honestidade sobre clientes**: na sessão própria um crash derruba as janelas (o Wayland não reconecta).
   O que se salva é o **lugar** de tudo (as telas voltam vazias como molduras armadas por `app_id` e a
   partida seguinte as preenche quando os apps reabrem via `[[spawn]]` ou à mão).

### D14 — Layer-shell no plano 2D; o HUD se recolhe

**Decisão.** Superfícies `top` e `overlay` (barras, notificações, lançadores) são compostas **em 2D sobre o
mundo**, na faceta primária (ou na que o cliente pedir), com âncoras, margens e zona exclusiva pelo protocolo.
Com uma barra `top` de zona exclusiva, a faixa de status do HUD some (`[hud] strip = "auto"`) e o rótulo do
modo desce para fora da zona. O foco de teclado `exclusive`/`on_demand` vale pelas regras do protocolo
(um lançador toma o teclado enquanto está aberto; `esc` nele devolve). `background` e `bottom` viram **o
lado de fora**: a textura aparece pelas janelas das salas e pela porta da recepção (e é o fundo do modo
plano); `swaybg` funciona e dá paisagem às janelas. **Por quê**: barras esperam pixel exato e cliques;
torná-las objetos 3D quebraria todas. O diegético continua sendo o caminho recomendado (faixa, objetos,
telefone, carteiros), e quem quer waybar tem waybar. **Descartada**: barra como quad 3D "na parede".

### D15 — Roteamento do ponteiro

Prioridade, de cima para baixo, por evento:

1. um **arrasto do mundo** ativo (§2.3) é dono do ponteiro;
2. uma superfície **layer-shell** `overlay`/`top` sob o ponteiro;
3. no **FOCO**, a janela e os seus popups (inclusive `pointer-constraints`), **exceto** com `world` segurado;
4. o **pick 3D** (`wm/pointer.rs`): raio pelo pixel; alvos na ordem alça de redimensionar > aro/tela >
   moldura vazia > objeto > gato > porta; o hover é recalculado só quando o alvo muda (como hoje) e a
   seleção por hover segue `[mouse] hover_selects`;
5. espaço vazio: botão do meio cria, direito abre o balão, esquerdo arrastando laça.

A máquina `Idle → Hover → Pressed{pos, pick, t} → Drag{Move | Resize(corner) | Rotate | Lasso} → Idle` é pura
(`nkw-core/src/arrange.rs`) e testada por tabela; o `nkw` só traduz eventos e aplica. Botões e teclas
pressionadas ao trocar de modo são soltos pelo hub (padrão que já existe para teclas). Cursor: `default`,
`grab`/`grabbing`, `nwse-resize` etc. pelo `cursor-shape` ao host (aninhado) ou pelo tema (sessão).

### D16 — Config canônica e IPC

- **`world.toml` continua a config do compositor** e ganha **recarga a quente** (inotify, debounce 200 ms,
  validação total antes de aplicar; erro = toast com linha e coluna, a config antiga segue). `ui.toml` fica
  para o que o telefone muda. **Teclas e scripts seguem no core** (`/v1/keymap`, Lua), decisão D10. Não há
  um segundo formato nem um segundo runtime. **Descartada**: KDL ou Lua no compositor.
- **`nkw msg`** é a IPC, como `niri msg`: socket `$XDG_RUNTIME_DIR/nkw-0.ipc` (`NKW_SOCKET` para o cliente),
  uma linha JSON por pedido e por resposta. Consultas: `screens`, `rooms`, `outputs`, `focus`, `mode`,
  `layouts`, `version`, `dump`. Ações: `action <world.cmd> [k=v …]` (os mesmos nomes do keymap, nada novo
  para aprender). `event-stream`: `{"event":"screen.opened"|"screen.closed"|"screen.focused"|
  "screen.urgent"|"room.changed"|"mode.changed"|"output.added"|"output.removed"|"layout.loaded", …}`.
  `--json` nas consultas. O `--script` continua para testes (`.nks`), e passa a poder falar com um `nkw` já
  rodando via `nkw msg script -`. O Lua do core chega ao mundo como hoje (`nk.ui.scene`), e a IPC é para
  shell, waybar, kanshi e quem mais quiser. Segurança: só o usuário (modo 0600 no socket), sem rede.

### D17 — Captura e portais

`wlr-screencopy-v1` **e** `ext-image-copy-capture-v1` implementados à mão no `nkw` (o Smithay não os traz;
é o que niri e cosmic-comp fazem), com: fonte "saída" = o quadro composto, fonte "toplevel" = a textura da
tela (exata, sem 3D), dano real, `block_out_from_capture` pinta a tela de creme na cópia, o cofre tranca ao
iniciar qualquer sessão de captura (regra existente), ON AIR acende. Backend de portal: **`xdg-desktop-
portal-wlr`** (ScreenCast e Screenshot; reconhece `XDG_CURRENT_DESKTOP=nkw` pela `nkw-portals.conf`) e
**`-gtk`** (FileChooser, Settings, Notification, AppChooser). **Descartadas**: `-gnome` (exige as interfaces
D-Bus do Mutter, que o niri implementa a um custo alto); portal próprio (cabe depois, para um seletor de
janela diegético via SALTO).

### D18 — Licenças

Smithay e anvil MIT; `wayland-protocols` e `wlr-protocols` MIT; `xcursor` MIT; libinput MIT; libseat MIT;
`xdg-desktop-portal-wlr` MIT (processo separado); `-gtk` LGPL (processo separado). O **niri é GPL-3.0**: serve
para listar recursos e entender comportamento (este documento); ao implementar, o código nasce das
especificações dos protocolos e da API do Smithay, nunca da leitura do código do niri. Regra de review: um PR
que implemente um protocolo cita a especificação e o módulo do Smithay, não o niri.

## 5. Orçamentos e testes

### 5.1 Orçamentos da sessão própria

Os de `plano-mundo.md` §1.3 continuam para o aninhado. Para `--session`, na máquina alvo (Intel Iris Xe,
Mesa), somam-se:

| métrica | cenário | alvo | falha acima de | como medir |
|---|---|---|---|---|
| scanout direto | S10: FOCO em tela cheia (mpv `--fs`, 60 s) | ≥ 95 % dos quadros sem composição | 80 % | `NKW_STATS`: `scanout_direct / frames` |
| cursor sem recompor | S11: ponteiro em movimento 60 s, NAV parado | 0 recomposições por evento de ponteiro; plano de cursor ativo | 1 recomposição/s | `recompose_for_cursor` no `NKW_STATS` |
| ocioso | S1 na sessão, 1 saída | ≤ 0,1 % e ≤ 1 wakeup/s | 0,5 % / 5/s | como hoje |
| bloqueado e DPMS off | S12: tela trancada 60 s | 0 quadros, ≤ 0,2 wakeups/s | 1 wakeup/s | idem |
| duas facetas | S8: NAV com 4 telas, 2 saídas 1080p | CPU por quadro p95 ≤ 3 ms; GPU ≤ 2 ms por faceta | p95 4,5 ms | `NKW_STATS` por saída |
| arrasto | S9: arrasto roteirizado 60 s, 4 telas no ar | 60 fps, 0 perdidos, CPU p95 ≤ 2 ms; ≤ 10 `configure`/s no redimensionar | 2 perdidos seguidos | `NKW_STATS` + log do testclient |
| latência de entrada | S2 na sessão | tecla no cliente < 1 ms; ponteiro→quadro ≤ 1 vblank | 2 vblanks | carimbo libinput → `presentation-time` |
| partida da sessão | `nkw --session` até o 1º quadro | ≤ 600 ms | 1,2 s | `NKW_STATS` do `starttime` |
| volta de VT / resume | `chvt` de volta, resume | quadro completo em ≤ 300 ms | 1 s | idem |
| RAM | 8 janelas, 2 saídas, sessão | ≤ 130 MB RSS | 170 MB | como hoje |
| binário | perfil `world` com `session` | ≤ 18 MB (aninhado continua ≤ 15 MB) | 25 MB | `stat` |

### 5.2 Estratégia de testes

| camada | o quê | onde |
|---|---|---|
| `nkw-core` puro | `arrange` (plano da vista, limites, redimensionar com presets e hints; tabela da máquina do ponteiro), `magnet` (cada alvo, tolerâncias, `ctrl`/`shift`), `history` (50 ops, coalescência de toques em 1 s, refazer), `onetoone` com pitch e escala (< 0,01 px numa grade), `persist` v1→v2 e arranjos (ida e volta, arquivo corrompido), estações novas na ordem da sala | `cargo test -p nkw-core`, ≥ 90 % de linhas (gate existente) |
| headless + `--script` | ponteiro com `pointer`, `button left\|middle\|right`, `scroll`, `drag`, modificadores; asserções `expect pose <id> ≈` e no log do `nkw-testclient` (tamanhos de `configure` durante o arrasto ≤ 10/s, final exato); shots `shot-arr-*.nks` (guias, anel 1:1, laçada, moldura, ancorada em 2D exato: recorte == buffer, 0 px) | `make world-shot`, `make world-test` |
| headless multi-saída | `--headless --outputs 2 --layout "1920x1080+0+0,1920x1080+1920+0"`: facetas, `send_output`, ancoradas por saída, hotplug simulado por `inject` | fase D |
| DRM sem GPU (CI local) | **VKMS** (`modprobe vkms`) numa VM ou num VT livre: `nkw --session --drm /dev/dri/cardN --no-seat` roda S1, S9, S10 e S12; o conector *writeback* do VKMS lê o quadro de volta para golden (0 px de diferença no 1:1) | `make world-session-smoke` (precisa de `sudo` para o módulo; documentado) |
| DRM de verdade | VT livre na máquina alvo (`chvt`, login, `nkw --session`): a lista do aceite da fase D, à mão, com checklist em `mundo.md`; S10/S11 medidos aqui | fim da fase D e semanalmente na E |
| protocolos | o `nkw-testclient` ganha layer-shell (barra com zona exclusiva, lançador com teclado exclusivo), session-lock, idle-notify, screencopy (pede um quadro e confere o pixel), data-control, keyboard-shortcuts-inhibit; `make world-compat` roda tudo sem janela | fase D |
| apps reais | a matriz de `mundo.md` ganha colunas "arrastar", "redimensionar ao vivo", "captura" e, na sessão, "IME", "DnD", "layer-shell" (waybar, mako, fuzzel, swaybg, swaylock, swayidle, wl-clipboard, grim, kanshi) | `make world-compat-nested`, e na sessão |
| desempenho | S8–S12 em `world-perf.sh` com as linhas novas no `world-perf-budget.toml`; números por fase no `log.md` | `make world-perf` |
| fuzz leve | tempestade de `configure`/commit do testclient durante um arrasto; perda e volta do dispositivo DRM simuladas no VKMS (unbind/bind) | fase D |
| conforto | 10 minutos arrumando telas com o mouse e 10 com o teclado, com os padrões, sem desconforto, assinado pelo usuário (como o aceite da fase 3) | fase A |

## 6. Roteiro

Esforço em semanas de um dev com agents, com as ondas de pacotes paralelos como nas ondas 2–4. Cada fase
é uma branch `mundo/fase-X-<slug>`, termina com números no `log.md` e é aceita pelo usuário.

### Fase A — Telas no espaço (aninhado, 3–4 semanas)

**Entregas**: §2 inteiro: moldura vazia e `N`/`shift+N`/botão do meio/balão/`a` no telefone; arrasto com o
mouse (plano da vista, rodinha, `shift`/`alt`/`ctrl`, cancelar); redimensionar com alças e presets, `configure`
a ≤ 10 Hz; ARRUMAR por teclado; seleção múltipla, laçada, clusters; arranjos (salvar/carregar/apagar, telefone
› Arranjos, paleta); ancoradas com 2D exato a 1:1; ímãs e guias; desfazer; sensação; `onetoone` com pitch e
escala; `world.json` v2; `--script` com ponteiro completo; comandos no core e modo `world.arrange`;
`mundo.md` ("Arrumar as telas"), `teclas.md`, `comandos.md`.

**Aceite**:
- `--script`: arrastar uma tela de uma mesa para o ar, redimensionar, girar, ancorar e desfazer tudo, com
  `expect pose` em cada passo e 0 erros de protocolo no testclient;
- FOCO pixel-exato (0 px) a partir de 12 poses no ar (yaw × pitch × escala) e de uma ancorada;
- S9: 60 fps, 0 quadros perdidos, CPU p95 ≤ 2 ms; ≤ 10 `configure`/s durante o redimensionar;
- um arranjo com 5 telas salva e carrega igual (ida e volta, poses ao milímetro);
- 10 + 10 minutos de arrumação sem desconforto, assinados;
- `nkw-core` ≥ 90 %; S1 e S4 sem regressão (> 20 % falha).

### Fase B — Paridade de WM, aninhado (3 semanas)

**Entregas**: `nkw msg` com consultas, ações e `event-stream`; recarga a quente do `world.toml`; regras v2;
mesa fechada (abas); salas pelo telefone e IPC, `[[room]]`; `[[spawn]]`; `world.screenshot`; hover e clique
configuráveis, `warp_to_focus`; urgência no aro; `[animations]`; `nkw msg debug` e sobreposição de tempo de
quadro; `--flat` (modo plano, que a fase D usa como rede); `color-scheme` do portal; mapa com miniaturas e
arrasto; backend SCTK **se** o critério de D4 bater (keyboard-shortcuts-inhibit para `super` aninhado).

**Aceite**: `nkw msg screens --json` lista o que o `dump` lista; um script em shell move uma janela de sala
pela IPC; editar `world.toml` muda a velocidade do ANDAR sem reiniciar e um erro de sintaxe vira toast; uma
regra `title` manda a janela certa para o ar; waybar lê `event-stream` (módulo custom) sem engasgar; `--flat`
passa a lista do dia a dia da fase 1; orçamentos verdes.

### Fase C — DECORAR, notas, foto (2 semanas)

**Entregas**: §3.1, §3.2, §3.4; catálogo de 6 padrões novos; telefone › Decorar; `[objects]` com pose no
`world.json`; `[photo]`.

**Aceite**: mover a vitrola para outra sala mantém a acústica e a colisão certas (teste do `obstacles` e do
`acoustic`); uma nota colada no aro acompanha a tela arrastada; foto sai sem HUD e com o cofre trancado;
S1 e S3 sem regressão.

### Fase D — Sessão própria atrás de `--session` (6–8 semanas)

**Entregas**: `Backend` (D12) e `backend/udev.rs` (DRM atômico, planos, scanout direto, cursor por hardware,
VRR); libinput e `[input]`; libseat, VT, suspend/resume; hotplug e `DrmOutputManager`; facetas e vistas (D11),
`[[output]]`, `wlr-output-management`, `ext-foreign-toplevel-list`; layer-shell (D14); `ext-session-lock`;
`ext-idle-notify` e DPMS; `wlr-screencopy` + `ext-image-copy-capture`, `nkw-portals.conf` (D17); gama;
`keyboard-shortcuts-inhibit`; `input-method-v2` + `text-input-v3` + `virtual-keyboard`; `ext-data-control`;
XCursor e imagens de cursor dos clientes; daemon de notificações próprio → carteiros; `[[spawn]]` do agente
polkit; `nkw.desktop`, `nkw-session`, unidades; D13 inteiro (panic hook, `--safe`, reinício, queda ao greeter);
`--headless --outputs`; VKMS e `make world-session-smoke`; S8, S10, S11, S12; multi-GPU com cópia.

**Aceite** (na máquina alvo, pelo greeter):
- entra pelo greeter e volta a ele ao sair; `loginctl` mostra a sessão; 2 monitores com hotplug (tirar e
  pôr o cabo) sem perder tela nenhuma;
- `swaylock` tranca e destranca; `swayidle` apaga e acende; suspend e resume com a sessão trancada;
- `chvt` para outro VT e de volta em ≤ 300 ms; um crash provocado (`nkw msg debug panic`) volta ao greeter
  em ≤ 5 s com `crash-*.json` gravado e, na partida seguinte, as molduras no lugar;
- compartilhar a tela e uma única janela num navegador pelo portal; `grim` tira screenshot; o cofre tranca
  ao começar;
- waybar, mako, fuzzel e swaybg funcionam; `cliphist` captura; fcitx5 escreve japonês num GTK4 em FOCO;
- `wlsunset` muda a gama; `kanshi` muda modo e escala;
- orçamentos de §5.1 verdes: scanout ≥ 95 %, cursor sem recompor, ocioso 0,1 %, trancado 0 quadros.

### Fase E — Uso diário e endurecimento (3–4 semanas)

**Entregas**: duas semanas de uso diário pelo usuário com a sessão `nkw` escolhida no greeter (niri
continua instalado); correções; DnD atravessando o mundo e para o mundo (nota, livro); gestos de touchpad;
tablet; VRR por saída validado; híbridos iGPU/dGPU; IME aninhado pelo SCTK; passe de acessibilidade; guia
`mundo.md` com a seção "A sessão própria" e a checklist; go/no-go.

**Aceite**: 10 dias úteis seguidos sem perda de trabalho nem queda ao greeter não provocada; a matriz de
compatibilidade completa na sessão; os orçamentos de §5.1 verdes em três medições semanais; o usuário
assina o go/no-go (§6.7).

### 6.6 Ordem, e por quê

**A → B → C → D → E.** A fase A vai primeiro porque é o pedido, roda aninhada (risco zero para a sessão), é
visível em dias e assenta a máquina do ponteiro de que D precisa. B vem antes de D porque IPC, recarga,
regras e modo plano são **pré-requisitos** da sessão (kanshi/waybar falam IPC; o modo plano é a rede de
D13) e ainda são aninhados. C é pequena e aproveita A quente; pode ser trocada de lugar com B se o usuário
preferir "mais lugar" a "mais WM". D só começa com A e B aceitas e com a pergunta 4 (§7) respondida. E é
o único lugar em que a sessão vira padrão, e só com o go/no-go.

**Riscos** desta fase e as mitigações:

| risco | mitigação |
|---|---|
| mudança de API do `DrmCompositor`/`GpuManager` no Smithay | rev pinada; re-pin em PR próprio; o `Backend` isola |
| NVIDIA e híbridos | `drm_syncobj` (explicit sync) na D; híbridos só com `render_node` explícito até a E; a sessão niri continua como alternativa no greeter |
| screencopy e output-management à mão (sem helper do Smithay) | escopo mínimo pelas especificações; testclient cobre; `-wlr` como primeiro cliente real |
| IME (IM v2 + text-input v3) é intrincado | fcitx5 como alvo único na D; aninhado fica para a E |
| layer-shell versus HUD disputando bordas | zona exclusiva manda; testes com waybar em cima/embaixo/lado; HUD recua sempre |
| enjoo com profundidade pela rodinha e com arrasto | 1 m/s no máximo, câmera parada, `edge_turn` desligado, aceite assinado de 20 minutos |
| botões ou teclas presas após um arrasto cruzando modos | o hub solta tudo ao trocar de modo (padrão existente); teste de integração dedicado |
| `configure` demais ao redimensionar | ≤ 10/s + final; medido em S9 |
| `world.json` v2 corromper o v1 do usuário | `migrate` com teste de ida e volta; `.bak` automático (regra existente) |
| escopo do DECORAR crescer | catálogo fechado; sem inventário; fase de 2 semanas |
| crash na sessão derruba as janelas | D13; `[[spawn]]` e molduras armadas por `app_id` reduzem o custo de voltar; honestidade na doc |
| captura vazar o cofre ou telas sensíveis | cofre tranca ao iniciar captura; `block_out_from_capture`; ON AIR visível |
| fork do Smithay (movimento relativo) perdurar | PR upstream nesta fase; se recusado, o SCTK da B o substitui |
| multi-saída dobrar o custo de GPU | culling por faceta, uploads únicos, vista fixa a ≤ 4 Hz; S8 com orçamento |

### 6.7 Go/no-go para a sessão virar padrão

Ao fim da E, a sessão `nkw` só vira a padrão do greeter se **todos** valerem:

1. 10 dias úteis seguidos de uso diário sem perda de trabalho e sem queda ao greeter não provocada;
2. os orçamentos de §5.1 verdes em três medições em semanas distintas;
3. a lista do aceite da D refeita verde na versão final;
4. um crash provocado volta ao greeter em ≤ 5 s com o estado salvo, três vezes seguidas;
5. os apps do dia a dia do usuário (terminal, navegador, editor, chat, reunião com compartilhamento de tela)
   na matriz com ✓ ou ◐ documentado, nenhum ✗;
6. a sessão niri continua instalada e funcional como alternativa, e a doc diz como voltar a ela.

Faltando qualquer um: a sessão fica opcional (`nkw` no greeter como segunda opção), o aninhado continua
suportado, e a próxima fase decide se insiste ou para.

## 7. Perguntas abertas

1. **Monitores**: você usa dois ou mais? Lado a lado, um vertical? A resposta valida o modelo "um olho,
   várias facetas" (D11) e diz se a vista fixa (`role = "view"`) precisa estar na primeira entrega da D.
2. **Arrastar no NAV**: botão esquerdo arrastando em qualquer ponto da tela move (proposto), ou só o aro e
   `world+arrastar`, como nos WMs 2D? A primeira é mais direta; a segunda nunca move nada sem querer.
3. **Arranjos**: só arrumam as telas que existem, ou também abrem os apps que faltam (`spawn`), virando uma
   "cabine" que restaura a sessão de trabalho? Isso muda o escopo da fase A em uns 3 dias.
4. **Sessão própria**: você quer entrar no mundo pelo greeter neste ciclo (fase D, 6–8 semanas) ou prefere
   parar na C e seguir aninhado por mais tempo? É a pergunta 5 do plano original, ainda aberta, e é a que mais
   pesa no total.
5. **Barras**: você roda waybar (ou outra barra) hoje? Se sim, barra plana em 2D sobre o mundo (D14) basta,
   ou a faixa do HUD deve absorver o que você tem na barra (módulos custom via `nkw msg`)?
6. **Captura**: compartilhar tela ou janela numa reunião é condição para o go/no-go (proposto que sim, via
   `xdg-desktop-portal-wlr`), ou pode ficar para depois da sessão virar padrão?
