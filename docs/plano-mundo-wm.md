---
okf_version: "0.2"
type: Plan
title: Plano — o mundo como WM de verdade (nkw): telas no espaço, paridade com o niri e a sessão própria
description: Segundo adendo ao plano do mundo. O que falta para o nkw ser um gerenciador de janelas completo como o niri (lacuna por área, com fase), as decisões do usuário de 2026-10-24, a especificação das telas no espaço (criar, arrastar no ar com ctrl+arrastar em qualquer ponto, redimensionar em 3D, grupos, arranjos das telas abertas, telas que seguem você, ímãs, desfazer), o HUD de jogo que absorve o papel da barra, o que mais entra nesta fase (DECORAR, notas, salas pelo telefone, modo foto), as decisões de arquitetura da sessão própria (um binário, backends, nunca tela preta, layer-shell e multimonitor no mundo 3D, roteamento do ponteiro, config e IPC, captura e compartilhamento de tela), orçamentos, testes e o roteiro em cinco fases com go/no-go.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/office
tags: [plano, mundo, nkw, wayland, smithay, compositor, wm, niri, drm, kms, libinput, libseat, layer-shell, multimonitor, portais, ipc, arrumar, telas-no-ar, mouse, decorar, sessão, hud, waybar, compartilhamento-de-tela, screencast]
timestamp: 2026-10-24T21:00:00Z
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

Este documento responde em oito partes: as **decisões do usuário** sobre as perguntas da primeira versão
(logo depois do resumo); a **lacuna** frente ao niri, área por área (§1); a especificação das **telas no
espaço**, que é o pedido explícito (§2); as **outras coisas** que valem entrar, inclusive o **HUD de jogo**
(§3); as **decisões de arquitetura** da sessão própria e do mundo com mouse (§4); **orçamentos e testes** (§5);
o **roteiro** com ordem, aceites e go/no-go (§6); e as **perguntas** (§7), agora quase todas respondidas.

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
  2. **HUD de jogo e paridade de WM** com o niri, ainda aninhado: o **HUD de jogo** que faz o papel da barra
     (salas como barra de atalhos, relógio, bandeja, áudio, rede, bateria, CPU/RAM, mídia, notificações,
     agentes; minimapa, bússola, inbox como diário de missões, medidores) é o primeiro pacote da fase; depois
     IPC `nkw msg`, regras de janela completas, recarga do `world.toml`, mesas em abas, salas criadas e
     renomeadas, `spawn` na partida, capturas de tela, **modo apresentação** (compartilhar uma janela só,
     aninhado), hover com o mouse, urgência, animações configuráveis, ferramentas de depuração. **Fase B.**
     Mais o que o 3D oferece de graça e o niri não tem: DECORAR, notas na parede, modo foto. **Fase C.**
  3. **A sessão própria**: DRM/KMS atômico, libinput, libseat, multimonitor, layer-shell, bloqueio, ociosidade,
     captura e portais, gama, DPMS, IME, cursor, notificações. É o grosso do esforço e o único pedaço que pode
     deixar você sem tela. **Fase D, atrás de `nkw --session`**, com a sessão niri continuando padrão até um
     **go/no-go** medido (fase E).
- **Decisões novas** (§4): multimonitor como **"um olho, várias facetas"** (D11) — todos os monitores são
  janelas da mesma vista, lado a lado como uma baia de vidro; um monitor pode ser uma **vista fixa** (o mapa, uma
  estação). **Um binário** (`nkw`, `nkw --session`, `nkw --headless`) com um trait de backend (D12). **Nunca
  tela preta** (D13): modo plano de emergência, hook de panic que salva o estado, unidade de reinício e queda
  para o greeter. **Barras e lançadores externos são planos**, em 2D sobre o mundo; o HUD de jogo esconde o
  que a barra já mostra e recua da zona exclusiva dela (D14, D19). **Mouse**: `ctrl+arrastar` move a tela a
  partir de qualquer ponto no NAV, no ARRUMAR e no ANDAR com ponteiro livre; o clique simples seleciona (e,
  no FOCO, é do app); `world+arrastar` funciona em todo modo e é o único jeito de mover no FOCO (D15).
  **`world.toml` continua canônico, com recarga a quente; `nkw msg` é a IPC**, JSON por socket, com fluxo de
  eventos (D16). **Portais e compartilhamento de tela**: `xdg-desktop-portal-wlr` para captura e `-gtk` para o
  resto; `wlr-screencopy` + `ext-image-copy-capture` escritos à mão; o seletor do portal é o SALTO do mundo;
  compartilhar tela e janela numa reunião é **condição do go/no-go** (D17).
- **As telas no espaço reaproveitam o que existe**: `Station::Air`, `carry.rs`, `collide::ray_hit_quad`,
  `onetoone.rs`, `persist::Air`, as intenções de lugar, o `--script` com ponteiro sintético. O novo é um
  **modo ARRUMAR** (`v` no NAV), uma máquina de estados do ponteiro, `magnet.rs` (ímãs e guias), `history.rs`
  (desfazer), **molduras vazias** (a próxima janela nasce ali), **clusters**, **arranjos** nomeados e **telas
  ancoradas** (seguem a vista e, a 1:1, são compostas em 2D exato, sem FOCO — o "segundo monitor" mesmo num
  monitor só).
- **Esforço**: A 3–4 semanas · B 4–5 · C 2 · D 6–8 · E 3–4, um dev com agents, no máximo quatro pacotes em
  paralelo por onda (a máquina do usuário não aguenta mais); 18 a 23 semanas no total. O go/no-go para a
  sessão virar padrão fica no fim da E, com critérios medidos (§6.7).

## Decisões do usuário (2026-10-24)

As perguntas da §7 da primeira versão tiveram estas respostas. Elas mudam a §2 (arrasto e arranjos), criam a
§3.5 (HUD de jogo) e a D19, e mexem em D14, D15, D17, §5 e §6.

| # | pergunta | decisão | onde muda |
|---|---|---|---|
| 2 | arrastar no NAV | **`ctrl+arrastar` em qualquer ponto da tela** (aro ou conteúdo) move a tela no NAV, no ARRUMAR e no ANDAR com ponteiro livre; o **clique simples** continua selecionando (e, no FOCO, vai para o app). **`world+arrastar`** vale em todo modo, inclusive no FOCO. No FOCO, `ctrl+clique` dentro do app é do app (abrir link em aba nova, seleção múltipla…), então **no FOCO só `world+arrastar` move**; no NAV e no ARRUMAR, `ctrl+arrastar` em qualquer ponto. O modificador é configurável (`[mouse] drag_modifier`) | §2.3, §2.4, §2.12, D15 |
| 3 | arranjos | **só arrumam as janelas já abertas**: nada é lançado; o que o arranjo pede e não está aberto vira um aviso com a lista | §2.6, fase A |
| 5 | barras | hoje roda a waybar, mas o desejado é um **HUD de jogo** que absorva o papel da barra (salas no lugar de workspaces, relógio, bandeja, áudio, rede, bateria, CPU/RAM, mídia, contagem de notificações, agentes), com cara de jogo: minimapa e bússola nos cantos, a inbox como diário de missões, medidores de status. **Barras layer-shell de verdade continuam suportadas na sessão** para quem as quer: planas, em 2D, e o HUD esconde as partes que elas já cobrem quando há zona exclusiva | §3.5, D14, D19, fase B |
| 6 | compartilhar tela | **é condição do go/no-go**: o ScreenCast pelo portal (`xdg-desktop-portal-wlr` com `wlr-screencopy`/`ext-image-copy-capture`) tem de funcionar na sessão própria, tela inteira e janela única; aninhado também, do jeito documentado em D17 (o niri já compartilha a janela inteira do `nkw`; uma janela só pelo modo apresentação) | D17, §5.1, fases B e D, §6.7 |
| 1 | monitores | sem resposta. **Premissa**: o multimonitor é desenhado de forma genérica (D11 vale para 1..N saídas, quaisquer posições, escalas e rotações), e a primeira entrega da D cobre 1 saída, 2 lado a lado, 1 + 1 vertical e 3 | D11, fase D |
| 4 | sessão própria neste ciclo | sem resposta. **Premissa**: a sessão própria **continua no roteiro** (fase D); o compartilhamento de janela pelo portal e o go/no-go dependem dela. Se o usuário decidir parar antes, A–C ficam de pé sozinhas | §6.6 |

O que **não** mudou: aninhado primeiro, teclado primeiro (todo gesto de mouse tem caminho por teclado),
1:1 exato no FOCO, câmera parada durante um arrasto, orçamentos de leveza, e a sessão niri como padrão até o
go/no-go.

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
| layer-shell (waybar, mako, fuzzel, swaybg) | ✗ | `wlr-layer-shell` pelo Smithay (`WlrLayerShellState`, `LayerMap`): **D14** — `top`/`overlay` compostos em 2D sobre o mundo com zona exclusiva respeitada pelo HUD (que esconde o que a barra já mostra, D19); `background`/`bottom` viram "o lado de fora" (visto pelas janelas das salas); teclado exclusivo/on-demand pelas regras do protocolo | D |
| o papel da barra sem barra | ◐ (faixa de status da onda 4) | **HUD de jogo** (§3.5, D19): salas como barra de atalhos, relógio, bandeja (StatusNotifierItem), áudio, rede, bateria, CPU/RAM, mídia, notificações, agentes, minimapa, bússola, diário de missões; módulos custom com o protocolo JSON da waybar | B |
| bloqueio de sessão (swaylock) | ✗ | `ext-session-lock-v1` (`SessionLockManagerState`): a superfície de bloqueio é composta 2D em toda saída; o passe do mundo para; o cofre tranca; sem superfície do cliente, uma tela creme própria com o relógio | D |
| ociosidade (swayidle) | ✗ | `ext-idle-notify-v1` (`IdleNotifierState`), alimentado por libinput; combina com o `idle-inhibit` dos clientes que já registramos; `[session] idle_after_s` → noite no mundo (as luzes baixam) | D |
| captura: screenshot e screencast | ✗ (só `--shot`) | `wlr-screencopy-v1` **e** `ext-image-copy-capture-v1` (+ `ext-image-capture-source-v1` para saída e para toplevel), à mão ou pelo Smithay se a rev pinada já os tiver, com `block_out_from_capture` das regras e o cofre trancando antes; captura de **uma tela** = a textura da janela (exata, independe do 3D); a captura de saída sai **sem o HUD** por padrão (`[capture] hud = false`); ON AIR acende no HUD local durante uma sessão de captura | D |
| backend de portal | ✗ | **D17**: `xdg-desktop-portal-wlr` (ScreenCast, Screenshot) + `xdg-desktop-portal-gtk` (FileChooser, Settings, Notification, AppChooser); `nkw-portals.conf` e `XDG_CURRENT_DESKTOP=nkw` só na sessão própria; o seletor de saída/janela do `-wlr` é o SALTO do mundo (`nkw msg pick --dmenu`) | D |
| compartilhar tela numa reunião (go/no-go) | ◐ (aninhado: o niri compartilha a janela inteira do `nkw`) | sessão própria: tela inteira e janela única pelo portal em navegador e app Electron (§6.7); aninhado: o mundo inteiro pelo portal do niri (já funciona) e **uma janela só pelo modo apresentação** (`world.present`: a tela em 1:1 ocupando a janela do `nkw`, sem HUD, sem o mundo em volta) | B (aninhado) · D |
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
| acessibilidade | ◐ (teclado total, movimento reduzido, alto contraste, escala de fonte, giro em passos) | mantém; soma: guias de ímã em alto contraste, tamanho do cursor (`[input] cursor_size`), modificador do arrasto configurável (`[mouse] drag_modifier = "ctrl" \| "alt" \| "world" \| "none"`), "sem arrasto" (`[mouse] drag = false`: tudo por teclado), HUD com escala, alto contraste e modo de foco por teclado (§3.5); toque não entra nesta fase | A · B · D |

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
| pose de tela | posição (m), `yaw`, `pitch` (±25°, novo), `scale` (0,5–3×, novo); nunca roll | `AirPose { pos, yaw, pitch, scale }`; no arquivo `Air { pos_mm, yaw_mdeg, pitch_mdeg, scale_pct, wall, cluster }` |
| moldura vazia | uma tela sem janela, no ar ou numa mesa; a próxima janela sem lugar nasce nela | `Frame { id: FrameId, pose, armed }` |
| tela ancorada | presa à vista (segue você), num encaixe do HUD; sempre 2D, exata a 1× | `DockSlot` (8 encaixes) + escala |
| cluster | grupo rígido de telas no ar que se move junto e é uma estação só | `Cluster { id: ClusterId, name, screens }` |
| arranjo | poses salvas por `app_id` (e `title` opcional) de uma sala; só arruma o que está aberto | `layouts/<name>.json` |
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
| `ctrl+n` (`world.frame`) | só a moldura, sem paleta: "a próxima janela vai aqui". A mais recente armada ganha; uma regra explícita de `[[rules]]` ganha da moldura. (Não é `shift+N`: `N` já é `shift+n`.) |
| botão do meio em espaço vazio (parede, chão, ar) | o mesmo que `N` naquele ponto |
| botão direito em espaço vazio | um **balão de menu** (uma página compacta no estilo do telefone, ancorada no ponteiro): "nova tela aqui", "terminal aqui", "moldura vazia aqui", "arranjos…"; na fase C entram "nota aqui" e, na E, "colar aqui" (uri-list → livro, texto → nota); `j`/`k`/`enter`/`esc` ou clique. Botão direito **sobre uma tela** abre o menu dela: ARRUMAR, ancorar, prender no ar, mandar para a sala…, fechar. Pelo teclado: `shift+f10` ou a tecla Menu (`world.menu`) na tela selecionada |
| telefone › Apps › `a` | abre o app **no ar**, à sua frente, em vez de numa mesa |
| paleta: `nova tela <app>` | idem ao `N`, a partir de qualquer modo |
| `T` / `world+enter` | continua: terminal numa mesa nova à direita. `world+shift+enter` (`world.terminal_air`): terminal no ar, na mira (não é `shift+T`: `T` já é `shift+t`) |

A moldura vazia é um objeto do mundo: uma moldura creme com "…" pulsando devagar (0,5 Hz, para, com movimento
reduzido). É estação (`h`/`l` chegam nela; `enter` abre a paleta nela), aparece no SALTO, persiste no
`world.json`, e `Q` a remove. Quando a janela chega, a moldura some com a janela crescendo de dentro (180 ms,
ease-out cúbico). Uma janela cujo `configure` o cliente ignora fica centrada e cortada na moldura, como no
FOCO hoje.

### 2.3 Arrastar com o mouse

**Onde pegar** (decisão do usuário, 2026-10-24). O gesto de mover é **`ctrl+botão esquerdo` arrastando a
partir de qualquer ponto da tela** — aro ou conteúdo — e **`world+botão esquerdo`**, como `Mod+arrastar` em
todo WM. O botão sem modificador nunca move nada sem querer:

| modo | botão esquerdo simples | `ctrl+esquerdo` arrastando | `world+esquerdo` arrastando | redimensionar |
|---|---|---|---|---|
| NAV, ARRUMAR | clique seleciona; duplo clique = FOCO; nas **alças** (cantos e bordas da tela selecionada ou sob o ponteiro) arrasta e redimensiona; em espaço vazio, laçada | move a tela de qualquer ponto | move | alças; `ctrl+direito` ou `world+direito` arrastando: pelo canto mais próximo |
| ANDAR, ponteiro livre (`alt` alterna) | como no NAV | move | move | como no NAV |
| ANDAR, mira (ponteiro travado) | interage (existe); direito pega (`g`, existe) | **segurar carrega**: a tela na mira vai para a mão enquanto o botão está apertado e é solta onde estiver ao soltar (o `g` sem segurar) | idem | — (só pelo ARRUMAR) |
| FOCO | é do app, `ctrl+clique` inclusive (abrir link em aba nova, seleção múltipla) | **é do app** | move: sai do FOCO, a câmera recua para a estação (≤ 260 ms) e a tela já vem pela mão | `world+direito` arrastando |
| CONVERSA, SALTO, COMANDOS, telefone | nada no mundo (a caixa/painel 2D pode ser clicável) | nada | move (sai do modo como o `esc`) | `world+direito` |

Clicar no mundo escurecido em volta do FOCO continua saindo do FOCO (como hoje). Uma tela **ancorada a 1×**
recebe o ponteiro como um segundo monitor (rolar uma referência sem sair do FOCO da outra), mas `ctrl+` e
`world+arrastar` sobre ela a movem. `[mouse] drag_modifier = "ctrl"` é o padrão; `"alt"`, `"world"` (só o
`world+`) e `"none"` (o botão simples arrastando move em qualquer ponto, a proposta original, para quem
prefere) existem. `world+` é `ctrl+alt` aninhado e `super` na sessão própria, então `world+arrastar` também
satisfaz o `ctrl`.

**Clique ou arrasto.** Um pressionar seguido de movimento abaixo de `[mouse] drag_threshold_px = 6` é
clique (seleciona; no já selecionado, ou com duplo clique em ≤ 400 ms, FOCO conforme `click_focuses`).
`ctrl+clique` sem arrastar é um clique. Acima do limiar, com o modificador, é arrasto: a tela "levanta" 3 cm
na sua direção e a sombra-bolha cresce (80 ms); sem o modificador, fora das alças, o pressionar vira nada (o
cursor mostra a mão aberta para ensinar o `ctrl`).

**O plano do arrasto.** A tela se move no **plano paralelo à imagem que passa pelo centro dela**: o raio do
ponteiro cruza esse plano e o centro vai para lá. Profundidade constante, até você mudar:

| gesto | efeito |
|---|---|
| mover | translada no plano da vista |
| rodinha / `[` `]` | profundidade, ±0,2 m por passo, 0,5 a 6 m, no máximo 1 m/s (conforto) |
| `shift` segurado | **encaixe forte**: grade de 25 cm, `yaw` em passos de 15°, alinhamento com paredes e slots |
| `alt` segurado + mover | **girar**: horizontal = `yaw`, vertical = `pitch` (±25°); sem `alt`, a tela fica virada para o olho ("billboard") e, ao soltar, mantém a direção que tinha para o olho |
| `alt` + rodinha | escala visual, 0,5 a 3×, passos de 10 % (também sem arrasto, sobre a tela selecionada) |
| `x` (alterna) ou `ctrl` apertado de novo | sem ímã nenhum enquanto valer |
| `g` (alterna) | encaixe forte sem segurar `shift` |
| `esc` ou botão direito durante o arrasto | cancela: a tela volta à pose de origem (≤ 200 ms) |
| soltar | `drop_target` (§2.8): slot de mesa, parede, encaixe do HUD (ancorar), outra tela (cluster, só com `shift`) ou o ar |

**O modificador que começou o arrasto não conta como gesto** enquanto não for solto: no `ctrl+arrastar` o
`ctrl` segurado não desliga os ímãs, e no `world+arrastar` aninhado (`ctrl+alt`) o `alt` segurado não gira.
Soltar o modificador no meio não encerra o arrasto (quem encerra é o botão); apertá-lo de novo passa a
valer como gesto. É uma regra só, testada por tabela, e evita que o atalho de pegar vire outra coisa. As
teclas durante o arrasto (`esc`, `[` `]`, `x`, `g`, `q` `e` para girar 15°) ficam num modo de teclas próprio,
`world.drag`, ativo só enquanto um arrasto do mouse dura.

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
mouse-look e ponteiro livre, e o HUD mostra qual está ativo. Com a mira, `ctrl+esquerdo` **segurado** carrega
a tela enquanto você olha e anda, e soltar o botão a solta. O `g` (carregar) continua sendo a forma de
**andar com a tela** sem segurar nada: ela flutua à sua frente enquanto você anda, e o arrasto é a forma de
**ficar parado e mover a tela**. As três usam as mesmas regras de soltar.

### 2.4 Redimensionar em 3D

- **Mouse**: alças nos quatro cantos (24 px em espaço de tela) e nas bordas (12 px) da tela selecionada e da
  que está sob o ponteiro, no NAV, no ARRUMAR e no ANDAR com ponteiro livre; arrastá-las **não precisa de
  modificador** (é a única exceção: alça é alça). Arrastar um canto muda largura e altura; uma borda, só um
  eixo; `shift` trava a proporção; `ctrl+botão direito` (NAV, ARRUMAR) e `world+botão direito` (todo modo,
  inclusive FOCO) em qualquer ponto redimensionam pelo canto mais próximo (como `Mod+botão direito` nos WMs).
- **O que muda**: o tamanho lógico do cliente. A projeção do novo retângulo na profundidade da tela vira
  milímetros (1 px = 1 mm). Limites: mínimo 200×150 lógicos, máximo o útil da saída a 1:1, e os `min_size`/
  `max_size` do `xdg_toplevel`. Encaixe em presets quando a 16 px: largura 1/3, 1/2, 2/3, 1 e altura 1/2,
  2/3, 1 do útil.
- **`configure` ao vivo**, no máximo 10 por segundo durante o arrasto, e um final ao soltar (clientes GTK e
  navegadores redesenham caro). A tela mostra o tamanho ("1280 × 800") enquanto redimensiona. Até o cliente
  comitar o tamanho novo, o quad usa a textura antiga esticada só dentro da moldura, como no niri.
- **Teclado**: `-`/`=` presets de largura (existe), `_`/`+` (`shift+-`/`shift+=`) presets de altura (novo),
  `1`…`4` no ARRUMAR escolhem a largura 1/3, 1/2, 2/3, 1 direto, `0` leva a tela à distância 1:1 ("ponto de
  leitura").
- **Escala** (visual) é `alt+rodinha` ou `ctrl+-`/`ctrl+=` no ARRUMAR; `ctrl+0` volta a 1×. (Não é
  `ctrl+rodinha`: o `ctrl` começa o arrasto.) Uma "TV" grande e longe é escala 2× a 4 m, com a janela ainda em
  1280×720.

### 2.5 Teclado: o modo ARRUMAR

`v` no NAV (ou `world.arrange` pela paleta) sobre a tela selecionada; `v` também no ANDAR sobre a tela na
mira. O HUD mostra `ARRUMAR` e uma legenda de uma linha. Teclas (modo `world.arrange` do keymap, religáveis,
sem cair nas teclas do NAV: só as globais `world+…` valem além destas):

| tecla | comando | o quê |
|---|---|---|
| `h` `j` `k` `l` / setas | `world.nudge_left` / `_down` / `_up` / `_right` | move 10 cm no plano da vista |
| `H` `J` `K` `L` · `alt+h` `alt+j` `alt+k` `alt+l` | `world.nudge_*_big` · `world.nudge_*_fine` | 50 cm · 1 cm |
| `[` `]` · `{` `}` | `world.nearer` / `world.farther` · `world.nearer_big` / `world.farther_big` | profundidade ±20 cm · ±50 cm |
| `q` `e` · `alt+q` `alt+e` | `world.yaw_left` / `world.yaw_right` · `world.yaw_left_fine` / `world.yaw_right_fine` | `yaw` ±15° · ±5° |
| `ctrl+k` `ctrl+j` | `world.pitch_up` / `world.pitch_down` | `pitch` ±5°, até ±25° |
| `-` `=` · `_` `+` | `world.narrower` / `world.wider` · `world.shorter` / `world.taller` | presets de largura · de altura |
| `1` `2` `3` `4` | `world.size_preset_1` … `_4` | largura 1/3, 1/2, 2/3, 1 |
| `0` | `world.read_distance` | leva à distância 1:1 e vira para o olho |
| `ctrl+-` `ctrl+=` `ctrl+0` | `world.scale_down` / `world.scale_up` / `world.scale_reset` | escala visual |
| `g` · `x` | `world.snap_toggle` · `world.magnets_toggle` | encaixe forte (grade) · sem ímã, até alternar de novo |
| `w` · `d` · `a` | `world.to_wall` / `world.to_desk` / `world.to_air` | encosta na parede atrás · entra no slot de mesa mais perto · fica no ar aqui |
| `p` | `world.dock` | ancora na vista (§2.7); de novo desancora |
| `space` | `world.select_toggle` | marca/desmarca esta tela na seleção múltipla |
| `tab` · `shift+tab` | `world.arrange_next` / `world.arrange_prev` | passa a arrumar a próxima / a anterior tela da sala, sem sair |
| `c` · `C` | `world.cluster` / `world.uncluster` | agrupa a seleção · desfaz o grupo (não é `G`/`shift+G`: `G` já é `shift+g`) |
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
- **Mover junto**: com várias selecionadas, arrastar (`ctrl+arrastar` qualquer uma, ou `hjkl` no ARRUMAR)
  move todas rigidamente; `alt` gira o conjunto em volta do centro dele. Soltar aplica os ímãs ao conjunto,
  não a cada uma.
- **Cluster** (`c` no ARRUMAR): a seleção vira um grupo com nome (padrão: os `app_id`, "firefox + foot"). Um
  cluster é **uma estação** (`Station::Cluster`): `h`/`l` param diante do conjunto inteiro, `j`/`k` percorrem
  as telas dele, `enter` dá FOCO na atual; arrastar qualquer membro move todos; `C` desfaz. Persistido.
- **Arranjo** (`s`, `world.layout_save {name}`): salva, por sala, as poses das telas no ar, ancoradas, nas
  mesas (mesa, posição na pilha, largura) e dos clusters, por `app_id` (+ `title` opcional, um padrão simples
  com `*` e `?`, sem regex). `world.layout_load {name}` faz as telas **já abertas** deslizarem para as poses
  (≤ 260 ms, escalonadas 30 ms entre si; corte com movimento reduzido). **Decisão do usuário: arranjo só
  arruma o que está aberto; nada é lançado.** Telas abertas que o arranjo não cita ficam onde estão; o que o
  arranjo cita e não está aberto vira um aviso ("faltam 2 do arranjo: firefox, mpv"), sem moldura nem
  `spawn`. Quem quiser a "cabine" (arranjo + apps) a monta com `[[spawn]]` (fase B) ou com um script na IPC.
  Casamento: entradas na ordem do arquivo; uma com `title` casa primeiro as telas cujo título casa; cada tela
  aberta é usada uma vez; empate, a de id menor. Ficam em `~/.local/state/nekuroddo/layouts/<name>.json`
  (`{"version": 1, …}`, nome `[a-z0-9_-]{1,32}`), listados no telefone (app **Arranjos**: carregar, salvar,
  apagar, renomear) e na paleta (`arranjo <name>`). Carregar um arranjo é **uma** operação do desfazer.

### 2.7 Telas que seguem você (ancoradas)

`p` no ARRUMAR, `world.dock`, ou soltar uma tela sobre um **encaixe do HUD** (as bordas e os cantos da saída
ficam destacados durante o arrasto) ancoram a tela à vista. Ela deixa de estar no mundo e passa a estar na
**sua** vista, num de 8 encaixes (4 cantos, 4 lados), com escala `[dock] scale = 0.33` por padrão (uma
imagem-em-imagem) ou 1× (a tela inteira, legível).

- **Ela é sempre composta em 2D** pelo caminho do Smithay (nunca no passe 3D), na escala dela; **a 1× é 2D
  exato**, como a janela em FOCO, mas **sem foco de teclado**. É o "segundo monitor" num monitor só:
  referência legível enquanto você digita em outra tela; a 1× ela recebe o ponteiro (rolar, clicar) sem
  tirar o teclado de quem o tem. Várias ancoradas a 1× dividem o útil da saída pelos encaixes escolhidos
  (esquerda/direita = metades). `enter` (FOCO) numa ancorada a 0,33× a mostra a 1× no mesmo encaixe enquanto
  durar o FOCO (120 ms), e `ctrl+]` a devolve à escala dela.
- No **FOCO**, as ancoradas continuam visíveis e vivas (não escurecem; LOD "estação atual"); a janela em
  FOCO fica no útil restante ou por cima delas, conforme `[dock] focus = "beside" \| "over"` (padrão `beside`
  quando couber a 1:1, senão `over`).
- São estações (`Station::Dock(slot)`): aparecem no SALTO sempre; `enter` dá FOCO em cima delas, sem
  deslize. `world+h/l` no FOCO também passam por elas.
- **No ANDAR** ficam (é o ponto: seguem você), em 0,33× por padrão; `[dock] hide_while_walking = true` as
  esconde para quem prefere a vista limpa.
- Limite `[dock] max = 4`. Persistidas por saída (faceta); uma saída que some devolve as suas à primária.
- Conforto: fixas à vista, não têm vecção; a mudança de escala anima 120 ms; nenhuma ancorada pode cobrir o
  rótulo do modo nem os selos (o HUD tem prioridade no mesmo canto; a ancorada desce). O overlay publica os
  retângulos que o HUD ocupa (`hud_reserved`), e os encaixes são calculados fora deles; com o HUD de jogo
  (§3.5) a regra é a mesma, na direção contrária também: **conteúdo 1:1 (FOCO ou ancorada a 1×) nunca é
  coberto pelo HUD**, que recolhe para "pips" nas margens.

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

`x` (ou o `ctrl` apertado de novo durante o arrasto, §2.3) desliga todos. A força do ímã é só na soltura e no
`g` do ARRUMAR: enquanto você arrasta, a tela segue o ponteiro e o **fantasma** mostra onde ela vai parar.
Isso evita o "puxão" que irrita em editores.

### 2.9 Desfazer

`nkw-core/src/history.rs`: um anel de `[arrange] history = 50` operações (`Move`, `Resize`, `Scale`, `Dock`,
`Undock`, `Cluster`, `Uncluster`, `LayoutLoad` com as poses anteriores, `Pin`, `ToDesk`, `ToWall`, `ToAir`,
`Frame`). `u` desfaz, `ctrl+r` refaz, com um toast curto ("desfeito: mover firefox"). Um arrasto inteiro
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
  Entram `Station::Cluster(id)`, `Station::Dock(slot)` e `Station::Frame(id)`, com a mesma ordem.
  Uma tela ancorada não conta na ordem da sala (ela está em você).
- **FOCO de qualquer orientação.** `onetoone.rs` ganha `pitch` e `scale`: a pose 1:1 fica sobre a normal da
  tela, à distância `scale · 0,001 · f / s`, com o pitch da tela (≤ 25°) e yaw dela; o ajuste submilimétrico
  ao pixel continua; teste: cantos a menos de 0,01 px do retângulo 2D para uma grade de `yaw × pitch × scale`.
  Se a pose cair dentro de um móvel ou parede (tela encostada), a câmera recua até o ponto livre mais perto na
  normal e o 2D continua exato (a troca quad→2D independe do caminho).
- **Persistência** (`world.json` v2, com `persist::migrate` v1→v2): `Air` ganha `pitch_mdeg`, `scale_pct`,
  `cluster`; `DockRec { app_id, output, slot, scale_pct }`; `FrameRec { room, air, armed, app_id }`;
  `ClusterRec { name, room, app_ids }`; `desk.tabbed` (reservado para a fase B). Arranjos em `layouts/`.
  Escrita atômica com o debounce de 2 s, como hoje; a migração v1→v2 guarda o `.bak`.
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
- Tudo por teclado (ARRUMAR, e o balão pelo `shift+f10`); `[mouse] drag = false` desliga o arrasto,
  `drag_modifier` troca o `ctrl` por `alt`, só `world` ou nenhum, e `hover_selects = false` o hover.
- O `esc` cancela em qualquer ponto e devolve a pose de origem; nada é destrutivo além de `Q`.

### 2.13 O que muda no código

| crate | novo / muda | o quê |
|---|---|---|
| `nkw-core` | `arrange.rs`, `dock.rs`, `layouts.rs` (novos) | `AirPose`, movimento no plano da vista (unprojeção + interseção com o plano), limites de profundidade/pitch/escala, redimensionar → tamanho lógico com limites e presets, laçada em espaço de tela, `configure` a ≤ 10/s; encaixes das ancoradas fora do HUD; esquema e casamento dos arranjos |
| `nkw-core` | `magnet.rs`, `history.rs` (novos) | §2.8, §2.9 |
| `nkw-core` | `pointer.rs` (novo) | a máquina do ponteiro, pura (D15): `Idle → Hover → Pressed → Drag{Move, Resize, Lasso, Carry} → Idle`, a tabela modo × alvo × botão × modificadores, a trava do modificador que começou o arrasto |
| `nkw-core` | `mode.rs`, `ids.rs` | `Kind::Arrange`, `Action::{Arrange, ArrangeDone, Click}`, `Effect::{ArrangeStart, ArrangeCommit, ArrangeRevert}`; `FrameId`, `ClusterId` |
| `nkw-core` | `strip.rs`, `onetoone.rs`, `persist.rs`, `rules.rs` | estações novas; escala e centro deslocado no 1:1 (FOCO ao lado das ancoradas); v2 do arquivo; `[mouse]`, `[arrange]`, `[dock]`; o `drop_target` do `g` fica como está, e o mouse e o ARRUMAR soltam pelo `magnet::snap` (que sabe de HUD e cluster) |
| `nkw` | `pointer/` (novo), `input.rs`, `router/` | picking, captura, modificadores, rodinha, cursor, o modo de teclas `world.drag`, o balão |
| `nkw` | `wm/world_ops.rs`, `world.rs`, `wm/layouts.rs` (novo) | comandos `world.*` novos; molduras; clusters; ancoradas; arranjos; a API de arrasto que o ponteiro chama |
| `nkw` | `frame.rs`, `render.rs`, `scene.rs` | composição 2D das ancoradas (exata a 1×); o FOCO ao lado delas; levantar e sombra |
| `nkw-gl` / `overlay` | `draw.rs`, `kit.rs`, `overlay/guides.rs` (novo) | moldura vazia (malha), alças, guias, anel 1:1, laçada, encaixes do HUD, o balão |
| `nkw` | `backend/script_input.rs`, `script.rs` | `pointer_to`, `button middle`, `click`, `scroll`, `drag x y x2 y2 [ms] [mods]`, `expect near`, `expect pose` para os testes headless |
| core (Go) | `internal/app/world.go`, `internal/core/keymap.go` | os comandos `world.*` novos no registro (`scope: client`), os modos `world.arrange`, `world.drag` e `world.menu` no keymap, teclas padrão |

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

### 3.5 HUD de jogo (fase B, primeiro pacote da onda 6)

**Decisão do usuário** (2026-10-24): o HUD do mundo absorve o papel da barra (hoje a waybar), com cara de
**HUD de jogo** — cantos e bordas, não uma faixa: minimapa e bússola, a inbox como diário de missões,
medidores de status, as salas como barra de atalhos. Isso revisa, para o status, a regra "diegético
primeiro, HUD só selo" do [adendo dos objetos](plano-mundo-objetos.md): os objetos continuam (o relógio de
parede, a vitrola, a bateria na recepção), mas a informação de barra passa a morar no HUD. `[hud] style =
"game" | "cozy" | "minimal"`: `cozy` é a faixa da onda 4, `minimal` é só o rótulo do modo e os selos, `game` é
este; `game` vira o padrão quando o usuário aprovar as capturas. Por que não entra na fase A: a onda 5 já
ocupa os quatro pacotes paralelos que a máquina aguenta; o que a A precisa do HUD (os retângulos reservados,
`hud_reserved`, que as ancoradas evitam) entra lá, e o HUD de jogo cabe depois sem retrabalho.

**Zonas** (pixels lógicos a `[hud] scale = 1.0`, margem de 16 px, a 1920×1080; tudo escala com a saída e com
o `font_scale`):

```
+--------------------------------------------------------------------------------+
| [TL] NAVEGAR . minha sala     [TC] --|-- bússola --|--          [TR] ( mini  ) |
|      firefox - título             salas, portas, gatos            ( mapa  )   |
|      selos: inbox 2 . ON AIR                                       14:05 sex   |
|                                                              [R] MISSÕES       |
|                                                              ! confirmar (1)   |
|                                 (mira no ANDAR)              > alpha quer falar|
|                                                                                |
| [BL] vida  bateria ######-- 72%  [BC] [1 minha][2 alpha*][3 bib]  [BR] > faixa |
|      enrg  CPU     ##------ 18%                                     vol ### BR  |
|      mana  RAM     ####---- 9,1 G   (a caixa da CONVERSA ocupa    [][][] (3) g4 |
|      xp    uso 5 h ###----- 41%      o BC quando aberta)                        |
+--------------------------------------------------------------------------------+
```

| zona | tamanho | o quê | quando |
|---|---|---|---|
| TL "onde estou" | ≤ 420 × 96 | rótulo do modo (existe), nome da sala com o ícone dela, a tela selecionada (`app_id` — título até 40 caracteres), os selos (existem) | sempre (no FOCO, só um pip) |
| TC bússola | 480 × 28 | fita de rumo: portas e salas na direção em que estão, gatos esperando (✉), telas urgentes (!), objetos; o norte é a recepção | ANDAR; NAV se `[hud.compass] show = "always"` |
| TR minimapa | 168 × 168 (círculo) + 40 | a sala vista de cima (mesas, telas, telas no ar, gatos como pontos na cor do contexto, portas, você como seta); gira com o rumo (`rotate = true`) ou norte fixo; relógio e dia embaixo; ícones no anel: rede, áudio, inibidor de ociosidade | sempre |
| R diário de missões | 300 × ≤ 5 linhas | a inbox como missões (abaixo) | quando há algo; vazio, some |
| BL medidores | 220 × 4 barras | bateria como "vida", CPU como "energia", RAM como "mana", uso de 5 h dos agents como "XP"; número sempre ao lado | `[hud.gauges] show = "always" \| "alert"` (padrão `always` no estilo `game`) |
| BC barra de salas | 44 px por sala, até 9 | as salas como barra de atalhos de jogo (`1`…`9`, as teclas que já existem): inicial ou ícone, quantas telas, a atual com anel, ponto âmbar se há gato esperando ou tela urgente ali | NAV e ANDAR; some na CONVERSA (a caixa ocupa o lugar) e no FOCO |
| BR bandeja e sistema | ≤ 360 × 88 | mini player (tocar/pausar, faixa até 28 caracteres, linha de progresso), volume, layout do teclado, ícones da bandeja (até 8, depois "+N"), sino com a contagem de notificações, agents ativos/bloqueados | sempre (no FOCO, pips) |
| centro | — | mira (ANDAR), telefone, caixa da CONVERSA, balão, toasts (sobem para logo abaixo da bússola) | como hoje |

**O que a waybar faz e onde vai** (para quem migra a config):

| módulo da waybar | no HUD | zona | fonte |
|---|---|---|---|
| `niri/workspaces`, `sway/workspaces`, `hyprland/workspaces` | barra de salas | BC | o `World` |
| `niri/window`, `sway/window` | a tela selecionada | TL | o `World` |
| `clock` | relógio e dia | TR | `nkw-sys` (relógio, existe) |
| `tray` | bandeja (StatusNotifierItem: ícone, ativar, ativar secundário, menu como página do telefone) | BR | `nkw-sys/sni.rs` (novo): *host* sempre; *watcher* quando o nome `org.kde.StatusNotifierWatcher` estiver livre (sessão própria sem waybar) |
| `pulseaudio`, `wireplumber` | volume e mudo | BR | `nkw-sys` (PipeWire, existe) |
| `network` | rede (barras, cabo/wi-fi; nunca o nome da rede no `dump`) | TR | `nkw-sys` (NetworkManager, existe) |
| `battery`, `upower` | bateria (barra de vida) | BL | `nkw-sys` (UPower, existe) |
| `cpu`, `memory` | barras de CPU e RAM | BL | `nkw-sys/proc.rs` (novo): `/proc/stat` e `/proc/meminfo` a cada 2 s, só com o HUD visível |
| `temperature`, `disk` | opcionais (`[hud.gauges] extra = ["temp", "disk"]`) | BL | `/sys/class/hwmon`, `statvfs` |
| `mpris` | mini player | BR | `nkw-sys` (MPRIS, existe) |
| `idle_inhibitor` | olho no anel do minimapa quando há inibidor | TR | protocolo `idle-inhibit` (existe) |
| `backlight` | barra transitória de brilho (3 s) | BL | `brightness.*` (existe) |
| `keyboard-state`, `language` | layout do teclado ("BR", "US") | BR | o seat (XKB) |
| `custom/*` com `exec` e `return-type = "json"` | **módulo custom com o mesmo protocolo JSON** (`text`, `alt`, `tooltip`, `class`, `percentage`) | a zona que a config disser | `[[hud.custom]] exec = [...], interval_s = 30, zone = "BR"` |
| notificações (swaync, mako) | sino com contagem; as notificações viram missões | BR, R | daemon próprio na sessão (fase D); aninhado, só o que o mundo produz (carteiros, avisos do core), a menos que nenhum daemon tenha o nome |
| agents (nosso) | ativos/bloqueados (BR), missões (R), uso de 5 h e custo (BL) | BR, R, BL | o plugin dos gatos (existe) |
| `bluetooth`, `power-profiles-daemon` | fora desta fase (o custom cobre) | — | — |

**Minimapa e bússola.** O minimapa reaproveita a folha do mapa (`o`, `overlay/map.rs`) em 168 px, recortada
em círculo, com a sala atual centrada em você; no corredor, o andar inteiro. Redesenha só quando a pose muda
≥ 5 cm ou ≥ 1°, ou quando a sala muda (no máximo 15 Hz no ANDAR, parado no NAV). A bússola projeta o rumo
de cada marcador em relação ao `yaw` da câmera, com ±90° visíveis; os marcadores fora da fita aparecem como
setinhas na ponta. Clicar no minimapa abre o mapa; clicar num marcador da bússola vai até ele (glide).

**Diário de missões.** A inbox como lista de missões, na ordem: confirmação pedida (!), gato bloqueado,
gato esperando você (✉), tela urgente, notificação. Até 5 linhas (ícone, texto ≤ 32 caracteres, idade "2
min"); o resto vira "+N". Cada uma tem a sua ação: confirmação → CONVERSA com o gato; gato → ir até ele e
conversar; tela urgente → ir até ela; notificação → abrir o app dela (a ação padrão da notificação).
Missão resolvida sai riscada (400 ms; some na hora com movimento reduzido). A lista completa é o app
**Inbox** do telefone.

**Medidores.** Barras de 220 × 10 px com o rótulo e o número ao lado (nunca só cor): bateria vermelha
abaixo de 20 % e com ícone de raio carregando; CPU média de 2 s; RAM usada/total; uso de 5 h com a marca
do limite. `alert` só os mostra fora do nominal (bateria < 30 %, CPU > 80 % por 10 s, RAM > 85 %).

**Convivência com barras layer-shell** (sessão própria, D14). Por saída, cada borda com zona exclusiva > 0
empurra para dentro as zonas do HUD daquela borda. `[hud] with_bar = "hide_duplicates"` (padrão) esconde os
widgets que fazem papel de barra (relógio, bandeja, barra de salas, rede, áudio, bateria, CPU/RAM, mídia,
notificações, teclado) quando há uma barra exclusiva naquela saída e mantém o que a barra não tem (rótulo
do modo, selos, minimapa sem os ícones do anel, bússola, missões, mira); `"keep"` mantém tudo; `"minimal"`
deixa só o rótulo e os selos. Superfícies sem zona exclusiva (docks, notificações flutuantes) não mexem no
HUD, mas ganham do HUD no ponteiro. Aninhado não há layer-shell dentro do `nkw`: a waybar continua no niri,
fora da janela; com o `nkw` em tela cheia no niri, a waybar fica escondida e o HUD faz o papel dela.

**FOCO, ancoradas e várias saídas.** No FOCO o HUD **nunca cobre o retângulo 1:1** nem uma ancorada a 1×:
recolhe para pips nas margens livres (modo e sala, contagem de missões, bateria crítica); com a janela
cobrindo a saída, nenhum HUD, só toast crítico (≤ 3 s). `world+z` mostra o HUD inteiro por cima por 3 s (ou
até apertar de novo) — o "segurar tab" dos jogos. Várias saídas: o HUD completo na primária e pips nas
outras (`[[output]] hud = "full" \| "pips" \| "none"`); a vista fixa (`role = "view"`) não tem HUD.

**Teclado, mouse e telefone.** `z` (NAV) esconde e mostra o HUD (existe); `world+z`, a espiada no FOCO;
`world+i` (`world.hud_focus`) entra no **modo HUD** (keymap `world.hud`): `h`/`l`/`j`/`k` passam pelos
widgets na ordem das zonas, `enter` faz a ação (ativar um item da bandeja, ir até a missão, ir para a sala,
tocar/pausar), `shift+f10` abre o menu do item (o menu da bandeja vira uma página do telefone), `1`…`5`
escolhem a missão N, `esc` sai. Com o mouse, os widgets são clicáveis no NAV e no ANDAR com ponteiro livre
(e no FOCO só durante a espiada); a rodinha sobre o volume muda o volume. A barra de salas usa as teclas
`1`…`9` que já existem. O telefone ganha **Ajustes › HUD** (estilo, cada widget ligado/desligado, bússola,
minimapa girando ou norte fixo, medidores, escala, `with_bar`, a lista dos módulos custom só para leitura).

**Acessibilidade.** Alto contraste e escala de fonte valem para tudo; `[hud] scale` de 0,75 a 1,5; nenhuma
informação só por cor (número ao lado de toda barra, ícone muda de forma); movimento reduzido: nada pulsa,
o minimapa fica norte fixo e atualiza a 4 Hz, a bússola sem suavização, missões sem animação; tudo o que o
HUD mostra existe como texto no telefone (Ajustes › HUD › "o que o HUD diz") e no `dump()`; tudo tem tecla
(modo HUD). Leitor de tela (AT-SPI) fica fora deste ciclo, documentado.

**Orçamento** (aninhado, máquina alvo; entra no `world-perf-budget.toml`):

| métrica | cenário | alvo | falha acima de |
|---|---|---|---|
| ocioso com o HUD de jogo inteiro | S1 | ≤ 0,1 % de CPU, ≤ 1,5 wakeups/s (o minuto do relógio + o amostrador de 2 s + sinais D-Bus) | 0,5 % / 3/s |
| composição do HUD | S7h: NAV com tudo ligado, missões mudando a cada 2 s | p50 ≤ 0,5 ms, p95 ≤ 1,0 ms | p95 1,5 ms |
| ANDAR com minimapa e bússola | S3 | quadro p95 + ≤ 0,3 ms sobre o S3 sem HUD | + 0,6 ms |
| RAM e binário | S6 | + ≤ 8 MB RSS; + ≤ 0,5 MB no binário | + 15 MB / + 1 MB |
| módulos custom | — | intervalo ≥ 5 s (preso), tempo-limite 2 s, saída ≤ 4 KB, só com o HUD visível | — |

Nenhum widget pede `Rate` em repouso; o HUD compõe só quando o `hud_hash` muda; ícones da bandeja são
rasterizados uma vez por tamanho; os amostradores param com o HUD escondido (`z`) ou com o FOCO cobrindo a
saída.

**Privacidade e testes.** O `dump()` traz contagens, ids e estados, nunca título de mídia, nome de rede,
título de item da bandeja ou corpo de notificação; as capturas usam só o `--demo` com `inject overlay {"hud":
…}`; goldens `shot-hud-{game,contrast,focus-pips,with-bar,compass-walk,tray-menu}.nks`; a convivência com
barra é testada headless com um testclient layer-shell na fase D.

**Código (onda 6).** `crates/nkw/src/overlay/hud_game/` (zonas, widgets, minimapa, bússola, missões),
widgets novos no kit `ui/` (o `Gauge` e as `Bars` já existem), `nkw-sys/src/{proc,sni}.rs`, o `Status` (S20)
ganha `cpu_pct`, `ram_mb`, `volume`, `kbd_layout`, `notifications`, `tray`; `[hud.*]` no `world.toml`;
Ajustes › HUD. Um pacote, ~1,5 semana; se a bandeja (SNI + dbusmenu) pesar, ela vira um segundo pacote da
mesma onda.

### 3.6 O que fica de fora, e por quê

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

**Genérico** (a pergunta 1 ficou sem resposta): nada supõe um número ou uma disposição de monitores. A
baia é calculada para 1..N saídas em quaisquer posições, escalas, taxas e rotações (o `yaw` de cada faceta
sai do centro físico dela em relação à primária; uma saída acima ou abaixo vira `pitch` da faceta, limitado a
±30°); o HUD segue `[[output]] hud`; a matriz de testes headless da D cobre 1 saída, 2 lado a lado, 1 + 1
vertical, 3 em arco e 2 com escalas diferentes (1 e 1,5).

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
Com uma barra de zona exclusiva, as zonas do HUD daquela borda recuam para fora dela e, com `[hud] with_bar =
"hide_duplicates"` (padrão), os widgets que fazem papel de barra somem naquela saída (§3.5); no estilo
`cozy`, a faixa de status some (`[hud] strip = "auto"`) e o rótulo do modo desce para fora da zona. O foco de teclado `exclusive`/`on_demand` vale pelas regras do protocolo
(um lançador toma o teclado enquanto está aberto; `esc` nele devolve). `background` e `bottom` viram **o
lado de fora**: a textura aparece pelas janelas das salas e pela porta da recepção (e é o fundo do modo
plano); `swaybg` funciona e dá paisagem às janelas. **Por quê**: barras esperam pixel exato e cliques;
torná-las objetos 3D quebraria todas. O diegético continua sendo o caminho recomendado (faixa, objetos,
telefone, carteiros), e quem quer waybar tem waybar. **Descartada**: barra como quad 3D "na parede".

### D15 — Roteamento do ponteiro

Prioridade, de cima para baixo, por evento:

1. um **arrasto do mundo** ativo (§2.3) é dono do ponteiro;
2. uma superfície **layer-shell** `overlay`/`top` sob o ponteiro (sessão própria);
3. a interface 2D do mundo sob o ponteiro: o balão, o telefone, a caixa da CONVERSA e, a partir da fase B, os
   widgets do HUD;
4. no **FOCO**, a janela e os seus popups (inclusive `pointer-constraints`) recebem tudo, `ctrl+clique`
   inclusive, **exceto** os acordes `world+botão` (esquerdo move, direito redimensiona); clicar no mundo
   escurecido em volta sai do FOCO (existe);
5. as **ancoradas** (retângulos 2D): `ctrl+` ou `world+esquerdo` arrastando movem; a 1×, o resto vai ao
   cliente dela (sem foco de teclado); abaixo de 1×, um clique seleciona;
6. o **pick 3D** (`crates/nkw/src/pointer/`): raio pelo pixel; alvos na ordem alça de redimensionar > tela
   (aro ou conteúdo) > moldura vazia > objeto, gato, porta, livro (o clique faz o que o rótulo do SALTO
   faria); botão simples = clique (seleciona; duplo = FOCO), `shift+clique` soma à seleção, `ctrl+`/`world+`
   arrastando move (`[mouse] drag_modifier`), alça arrastando redimensiona, `ctrl+`/`world+direito`
   arrastando redimensiona pelo canto mais próximo, `alt+rodinha` escala; o hover é recalculado só quando o
   alvo muda e a seleção por hover segue `[mouse] hover_selects` (fase B);
7. espaço vazio: botão do meio cria, direito abre o balão, esquerdo arrastando laça (`shift`: soma).

No ANDAR com a mira (ponteiro travado), os botões seguem a mira: esquerdo interage, direito pega (existem) e
`ctrl+esquerdo` segurado carrega.

A máquina `Idle → Hover → Pressed{pos, hit, mods, t} → Drag{Move | Resize(grip) | Lasso | Carry} → Idle` é
pura (`nkw-core/src/pointer.rs`) e testada por tabela (modo × alvo × botão × modificadores → gesto), com a
regra da trava: o modificador que começou o arrasto não vale como gesto até ser solto (§2.3). O `nkw` só
traduz eventos e aplica: a matemática do arrasto (`arrange.rs`) e os ímãs (`magnet.rs`) ficam no `World`,
que o ponteiro chama. Botões e teclas pressionadas ao trocar de modo são soltos pelo hub (padrão que já existe
para teclas); um arrasto vivo numa troca de modo, na perda de foco da janela do host ou com o botão solto fora
dela termina como `soltar`. Cursor: `default`, `grab`/`grabbing`, `nwse-resize` etc. pelo `cursor-shape` ao
host (aninhado) ou pelo tema (sessão).

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

### D17 — Captura, portais e compartilhamento de tela

**Compartilhar a tela numa reunião é condição do go/no-go** (decisão do usuário, 2026-10-24).

**Sessão própria.** `wlr-screencopy-v1` **e** `ext-image-copy-capture-v1` com `ext-image-capture-source-v1`
(fonte de saída e fonte de toplevel, esta sobre `ext-foreign-toplevel-list-v1`) no `nkw`; à mão, como niri e
cosmic-comp, ou pelos módulos do Smithay se a rev pinada já os tiver (verificar no primeiro dia da D). Fonte
"saída" = o quadro composto **sem o HUD, os toasts e o balão** (`[capture] hud = false`, padrão; uma
composição extra só enquanto houver sessão de captura, na taxa que o cliente pede); fonte "toplevel" = a
textura da tela (exata, sem 3D, independe de onde ela está no mundo, inclusive em outra sala); dano real;
cursor embutido ou como metadado, como o cliente pedir; `block_out_from_capture` pinta a tela de creme na
cópia; o cofre tranca ao iniciar qualquer sessão de captura (regra existente); ON AIR acende **no HUD local**,
nunca na cópia. Backend de portal: **`xdg-desktop-portal-wlr`** (ScreenCast e Screenshot; reconhece
`XDG_CURRENT_DESKTOP=nkw` pela `nkw-portals.conf`; janela única pede a versão com `ext-image-copy-capture`,
saída funciona também só com `wlr-screencopy`) e **`-gtk`** (FileChooser, Settings, Notification,
AppChooser). O **seletor** do `-wlr` é o mundo: `chooser_type = "dmenu"` com `chooser_cmd = "nkw msg pick
--dmenu"`, que recebe a lista de saídas e janelas e abre um SALTO diegético (rótulos nas telas e nas
facetas); `esc` cancela o compartilhamento. O fluxo vai por PipeWire (dmabuf quando o cliente aceita, senão
shm). Áudio do compartilhamento não é do portal (cada app trata o seu), documentado.

**Aninhado** (como funciona hoje e o que a fase B soma):

1. **O mundo inteiro**: os apps dentro do `nkw` falam com o portal da sessão do host (o do niri). O seletor é
   o do niri, e ele compartilha a janela do `nkw` inteira (o mundo, com o HUD) ou uma saída do niri. Já
   funciona e vai documentado em `mundo.md`.
2. **Uma janela só**: o niri só vê a janela do `nkw`, então a fase B traz o **modo apresentação**
   (`world.present`, `world+shift+p`): a tela escolhida em 1:1 ocupando a janela do `nkw` (configurada ao
   tamanho dela), sem HUD, sem o mundo em volta, até `world+shift+p` de novo; compartilha-se a janela do `nkw`
   no seletor do niri. O ON AIR aparece só por 2 s ao entrar (para não vazar na cópia).
3. **Ferramentas de captura apontadas para o display aninhado** (`grim` com `WAYLAND_DISPLAY` do `nkw`)
   funcionam a partir da D, quando os protocolos existirem; é o caminho para desenvolver e testar a captura sem
   sair do niri. O portal `-wlr` não serve aninhado (o frontend da sessão escolhe o backend do niri).

**Descartadas**: `-gnome` (exige as interfaces D-Bus do Mutter, que o niri implementa a um custo alto);
portal próprio (o seletor via SALTO já resolve o que ele daria); compartilhar por uma saída virtual
(`headless output`) no aninhado (custo de uma segunda composição sempre ligada, sem ganho sobre o modo
apresentação).

### D18 — Licenças

Smithay e anvil MIT; `wayland-protocols` e `wlr-protocols` MIT; `xcursor` MIT; libinput MIT; libseat MIT;
`xdg-desktop-portal-wlr` MIT (processo separado); `-gtk` LGPL (processo separado). O **niri é GPL-3.0**: serve
para listar recursos e entender comportamento (este documento); ao implementar, o código nasce das
especificações dos protocolos e da API do Smithay, nunca da leitura do código do niri. Regra de review: um PR
que implemente um protocolo cita a especificação e o módulo do Smithay, não o niri. O mesmo vale para a
waybar (MIT) no HUD: o protocolo JSON dos módulos custom é uma interface documentada, reimplementada; nenhum
código copiado.

### D19 — O HUD de jogo faz o papel da barra; barras de verdade continuam

**Decisão.** O status de barra (salas, relógio, bandeja, áudio, rede, bateria, CPU/RAM, mídia, notificações,
agentes) mora no **HUD de jogo** (§3.5), desenhado pelo overlay com o kit `ui/` próprio, em zonas de canto e
borda, alimentado pelo `Status` dos plugins e por fontes novas do `nkw-sys` (CPU/RAM, StatusNotifierItem).
Barras layer-shell continuam suportadas na sessão (D14) e, quando existem, mandam: o HUD recua da zona
exclusiva e esconde o que a barra já mostra. Módulos custom aceitam o protocolo JSON da waybar, para quem
migra. **Por quê**: o usuário quer o mundo com cara de jogo e não quer abrir mão do que a barra dá; o kit e o
`Status` da onda 4 já são a base, e o HUD funciona igual aninhado e na sessão. **Descartadas**: a waybar dentro
do mundo como quad 3D (D14); exigir a waybar (o mundo não teria status sem ela aninhado em tela cheia); um
segundo processo de barra nosso (duplicaria o kit e a IPC).

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
| compartilhar a saída | S13: ScreenCast 1080p60 pelo `-wlr` por 60 s, NAV com 4 telas vivas | 60 fps entregues, CPU extra p95 ≤ 1,5 ms por quadro capturado, 0 quadros com HUD ou cofre aberto na cópia | 2 ms / 1 quadro | `NKW_STATS` `capture_*` + o testclient de captura conferindo pixels |
| compartilhar uma janela | S13w: fonte toplevel de um navegador de teste por 60 s, a tela em outra sala | taxa da janela, CPU extra p95 ≤ 0,5 ms, pixels iguais à textura (0 px) | 1 ms | idem |
| partida da sessão | `nkw --session` até o 1º quadro | ≤ 600 ms | 1,2 s | `NKW_STATS` do `starttime` |
| volta de VT / resume | `chvt` de volta, resume | quadro completo em ≤ 300 ms | 1 s | idem |
| RAM | 8 janelas, 2 saídas, sessão | ≤ 130 MB RSS | 170 MB | como hoje |
| binário | perfil `world` com `session` | ≤ 18 MB (aninhado continua ≤ 15 MB) | 25 MB | `stat` |

### 5.2 Estratégia de testes

| camada | o quê | onde |
|---|---|---|
| `nkw-core` puro | `arrange` (plano da vista, limites, redimensionar com presets e hints; tabela da máquina do ponteiro), `magnet` (cada alvo, tolerâncias, `ctrl`/`shift`), `history` (50 ops, coalescência de toques em 1 s, refazer), `onetoone` com pitch e escala (< 0,01 px numa grade), `persist` v1→v2 e arranjos (ida e volta, arquivo corrompido), estações novas na ordem da sala | `cargo test -p nkw-core`, ≥ 90 % de linhas (gate existente) |
| `nkw-core` puro, ponteiro | a tabela modo × alvo × botão × modificadores → gesto (`ctrl+arrastar` em qualquer ponto no NAV/ARRUMAR, só `world+` no FOCO, alça sem modificador, `ctrl+segurar` carrega no ANDAR com mira), a trava do modificador que começou o arrasto, limiar e duplo clique, cancelar por `esc`/direito/perda de foco | `cargo test -p nkw-core` |
| headless + `--script` | ponteiro com `pointer_to`, `pointer`, `button left\|middle\|right`, `click`, `scroll`, `drag … [mods]`; asserções `expect pose <app_id> …`, `expect near` e no log do `nkw-testclient` (tamanhos de `configure` durante o arrasto ≤ 10/s, final exato); `ctrl+clique` no FOCO chega ao cliente (o testclient registra o botão e o modificador); shots `shot-arr-*.nks` (guias, anel 1:1, laçada, moldura, balão, ancorada em 2D exato: recorte == buffer, 0 px) | `make world-shot`, `make world-test` |
| HUD de jogo (fase B) | zonas por tamanho de saída e escala (tabela pura), `with_bar` com zonas exclusivas falsas, pips no FOCO (nenhum pixel do HUD dentro do retângulo 1:1), widgets sem dado somem, `dump()` sem títulos nem nomes; goldens `shot-hud-*` | `cargo test -p nkw`, `make world-shot` |
| captura (fase D) | o testclient ganha `--screencopy` e `--image-copy {output\|toplevel}`: pede um quadro e confere pixels (o HUD ausente na saída com `[capture] hud = false`, a janela exata na fonte toplevel, a tela bloqueada pintada de creme, o cofre trancado); `nkw msg pick --dmenu` com uma lista fixa na entrada | `make world-compat`, VKMS |
| headless multi-saída | `--headless --outputs 2 --layout "1920x1080+0+0,1920x1080+1920+0"`: facetas, `send_output`, ancoradas por saída, hotplug simulado por `inject` | fase D |
| DRM sem GPU (CI local) | **VKMS** (`modprobe vkms`) numa VM ou num VT livre: `nkw --session --drm /dev/dri/cardN --no-seat` roda S1, S9, S10 e S12; o conector *writeback* do VKMS lê o quadro de volta para golden (0 px de diferença no 1:1) | `make world-session-smoke` (precisa de `sudo` para o módulo; documentado) |
| DRM de verdade | VT livre na máquina alvo (`chvt`, login, `nkw --session`): a lista do aceite da fase D, à mão, com checklist em `mundo.md`; S10/S11 medidos aqui | fim da fase D e semanalmente na E |
| protocolos | o `nkw-testclient` ganha layer-shell (barra com zona exclusiva, lançador com teclado exclusivo), session-lock, idle-notify, screencopy (pede um quadro e confere o pixel), data-control, keyboard-shortcuts-inhibit; `make world-compat` roda tudo sem janela | fase D |
| apps reais | a matriz de `mundo.md` ganha colunas "arrastar", "redimensionar ao vivo", "captura" e, na sessão, "IME", "DnD", "layer-shell" (waybar, mako, fuzzel, swaybg, swaylock, swayidle, wl-clipboard, grim, kanshi) e "compartilhar" (tela inteira e janela num navegador de teste e num app Electron com perfil descartável, chamada de teste de 30 min) | `make world-compat-nested`, e na sessão |
| desempenho | S8–S12 em `world-perf.sh` com as linhas novas no `world-perf-budget.toml`; números por fase no `log.md` | `make world-perf` |
| fuzz leve | tempestade de `configure`/commit do testclient durante um arrasto; perda e volta do dispositivo DRM simuladas no VKMS (unbind/bind) | fase D |
| conforto | 10 minutos arrumando telas com o mouse e 10 com o teclado, com os padrões, sem desconforto, assinado pelo usuário (como o aceite da fase 3) | fase A |

## 6. Roteiro

Esforço em semanas de um dev com agents, com as ondas de pacotes paralelos como nas ondas 2–4. Cada fase
é uma branch `mundo/fase-X-<slug>`, termina com números no `log.md` e é aceita pelo usuário.

### Fase A — Telas no espaço (aninhado, 3–4 semanas, onda 5 em quatro pacotes)

**Entregas**: §2 inteiro: moldura vazia e `N`/`ctrl+n`/botão do meio/balão/`a` no telefone; arrasto com o
mouse por `ctrl+arrastar` em qualquer ponto (NAV, ARRUMAR, ANDAR com ponteiro livre) e `world+arrastar` em
todo modo (o único no FOCO), `ctrl+segurar` carregando no ANDAR com mira, plano da vista, rodinha,
`shift`/`alt`/`x`/`g`, a trava do modificador, cancelar; redimensionar com alças e presets, `configure` a ≤ 10
Hz; ARRUMAR por teclado; seleção múltipla, laçada, clusters; arranjos **das telas abertas** (salvar, carregar,
apagar, renomear; telefone › Arranjos; paleta); ancoradas sempre em 2D, exatas a 1×, fora dos retângulos do
HUD (`hud_reserved`); ímãs e guias; desfazer; sensação; `onetoone` com escala; `world.json` v2; `--script`
com ponteiro completo; comandos no core e os modos `world.arrange`, `world.drag`, `world.menu`; `mundo.md`
("Arrumar as telas"), `teclas.md`, `comandos.md`.

**Aceite**:
- `--script`: arrastar uma tela de uma mesa para o ar com `ctrl+arrastar` a partir do meio do conteúdo,
  redimensionar pela alça, girar, ancorar e desfazer tudo, com `expect pose` em cada passo e 0 erros de
  protocolo no testclient; um `ctrl+clique` no FOCO chega ao testclient com o `ctrl`, e um `world+arrastar` no
  FOCO sai do FOCO e move a tela;
- um arranjo que cita uma tela fechada carrega o resto e avisa a falta, sem lançar nada;
- FOCO pixel-exato (0 px) a partir de 12 poses no ar (yaw × pitch × escala) e de uma ancorada;
- S9: 60 fps, 0 quadros perdidos, CPU p95 ≤ 2 ms; ≤ 10 `configure`/s durante o redimensionar;
- um arranjo com 5 telas salva e carrega igual (ida e volta, poses ao milímetro);
- 10 + 10 minutos de arrumação sem desconforto, assinados;
- `nkw-core` ≥ 90 %; S1 e S4 sem regressão (> 20 % falha).

### Fase B — HUD de jogo e paridade de WM, aninhado (4–5 semanas, ondas 6 e 7)

**Entregas**: **primeiro pacote da onda 6: o HUD de jogo** (§3.5, D19): zonas, minimapa, bússola, diário de
missões, medidores, barra de salas, bandeja (StatusNotifierItem), mini player, volume, layout do teclado,
notificações, agents, módulos custom no protocolo da waybar, modo HUD (`world+i`), espiada no FOCO
(`world+z`), Ajustes › HUD, `[hud.*]`. Depois: `nkw msg` com consultas, ações e `event-stream`; recarga a
quente do `world.toml`; regras v2; mesa fechada (abas); salas pelo telefone e IPC, `[[room]]`; `[[spawn]]`;
`world.screenshot`; **modo apresentação** (`world.present`, compartilhar uma janela só pelo portal do niri);
hover e clique configuráveis, `warp_to_focus`; urgência no aro; `[animations]`; `nkw msg debug` e
sobreposição de tempo de quadro; `--flat` (modo plano, que a fase D usa como rede); `color-scheme` do
portal; mapa com miniaturas e arrasto; backend SCTK **se** o critério de D4 bater (keyboard-shortcuts-inhibit
para `super` aninhado). Onda 6: HUD, IPC + recarga, regras + `[[spawn]]` + salas + mesa fechada, screenshot +
modo apresentação + urgência + animações; onda 7: o resto.

**Aceite**: com o `nkw` em tela cheia no niri e a waybar escondida atrás, o usuário diz que não sente falta
dela por uma semana (relógio, bandeja, áudio, rede, bateria, CPU/RAM, mídia e notificações do mundo no HUD);
os orçamentos do HUD (§3.5) verdes e nenhum pixel do HUD dentro do retângulo 1:1 no FOCO; um módulo custom da
config da waybar do usuário roda sem mudança; compartilhar uma janela numa chamada de teste pelo modo
apresentação e o mundo inteiro pela janela do `nkw`; `nkw msg screens --json` lista o que o `dump` lista; um
script em shell move uma janela de sala pela IPC; editar `world.toml` muda a velocidade do ANDAR sem
reiniciar e um erro de sintaxe vira toast; uma regra `title` manda a janela certa para o ar; waybar lê
`event-stream` (módulo custom) sem engasgar; `--flat` passa a lista do dia a dia da fase 1; orçamentos
verdes.

### Fase C — DECORAR, notas, foto (2 semanas)

**Entregas**: §3.1, §3.2, §3.4; catálogo de 6 padrões novos; telefone › Decorar; `[objects]` com pose no
`world.json`; `[photo]`.

**Aceite**: mover a vitrola para outra sala mantém a acústica e a colisão certas (teste do `obstacles` e do
`acoustic`); uma nota colada no aro acompanha a tela arrastada; foto sai sem HUD e com o cofre trancado;
S1 e S3 sem regressão.

### Fase D — Sessão própria atrás de `--session` (6–8 semanas)

**Entregas**: `Backend` (D12) e `backend/udev.rs` (DRM atômico, planos, scanout direto, cursor por hardware,
VRR); libinput e `[input]`; libseat, VT, suspend/resume; hotplug e `DrmOutputManager`; facetas e vistas (D11,
genéricas para 1..N saídas), `[[output]]` (com `hud`), `wlr-output-management`, `ext-foreign-toplevel-list`;
layer-shell (D14) e o HUD recuando dela (D19); `ext-session-lock`; `ext-idle-notify` e DPMS; `wlr-screencopy`
+ `ext-image-copy-capture` + `ext-image-capture-source` (saída e toplevel), captura sem HUD, `nkw msg pick
--dmenu` como seletor do `-wlr`, `nkw-portals.conf` (D17), S13/S13w; gama;
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
- **compartilhar a tela inteira e uma única janela** numa chamada de teste de 30 min num navegador (perfil
  descartável) e num app Electron, pelo portal, escolhendo pelo SALTO; a cópia sem HUD e com a tela marcada
  `block_out_from_capture` em creme; a janela compartilhada continua exata quando muda de sala; `grim` tira
  screenshot; o cofre tranca ao começar;
- waybar na sessão: o HUD recua da zona exclusiva e esconde o que ela mostra (`hide_duplicates`), em cima,
  embaixo e do lado;
- a matriz headless de saídas (1, 2 lado a lado, 1 + 1 vertical, 3, escalas mistas) verde;
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
visível em dias e assenta a máquina do ponteiro de que D precisa. O **HUD de jogo abre a fase B** e não a A:
a onda 5 já usa os quatro pacotes paralelos que a máquina do usuário aguenta, e a A só precisa do HUD os
retângulos reservados, que entram nela; aninhado em tela cheia, o HUD é o que deixa a waybar dispensável, e
por isso vem antes do resto da paridade. B vem antes de D porque IPC, recarga, regras e modo plano são
**pré-requisitos** da sessão (kanshi/waybar falam IPC; o modo plano é a rede de D13) e ainda são aninhados;
o modo apresentação dá o compartilhamento de uma janela já no aninhado. C é pequena e aproveita A quente;
pode ser trocada de lugar com B se o usuário preferir "mais lugar" a "mais WM". D começa com A e B aceitas;
a pergunta 4 ficou sem resposta e a premissa é que a sessão própria continua no roteiro, porque o
compartilhamento pelo portal e o go/no-go dependem dela. E é o único lugar em que a sessão vira padrão, e só
com o go/no-go.

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
| `ctrl+arrastar` pegar quem queria só clicar com `ctrl` | limiar de 6 px; no FOCO o `ctrl` é do app; `drag_modifier` configurável; a trava do modificador testada por tabela |
| o HUD de jogo pesar no ocioso ou cobrir conteúdo | amostradores de 2 s só com o HUD visível, nada de `Rate` em repouso, S1/S3/S7h com orçamento; regra dura de não cobrir o 1:1 |
| bandeja (StatusNotifierItem + dbusmenu) ser um poço | ícone, ativar e menu como página do telefone; o resto do dbusmenu (ícones de item, submenus fundos) fica para depois; pode virar pacote próprio na onda 6 |
| `xdg-desktop-portal-wlr` instalado não ter janela única (`ext-image-copy-capture`) | verificar a versão no primeiro dia da D; saída inteira funciona com `wlr-screencopy`; janela única entra no go/no-go, então a versão mínima vai para `instalacao.md` |
| compartilhar vazar o HUD, o cofre ou uma tela marcada | composição de captura sem HUD por padrão, cofre tranca ao iniciar, `block_out_from_capture`, teste de pixels no testclient |

### 6.7 Go/no-go para a sessão virar padrão

Ao fim da E, a sessão `nkw` só vira a padrão do greeter se **todos** valerem:

1. 10 dias úteis seguidos de uso diário sem perda de trabalho e sem queda ao greeter não provocada;
2. os orçamentos de §5.1 verdes em três medições em semanas distintas;
3. a lista do aceite da D refeita verde na versão final;
4. um crash provocado volta ao greeter em ≤ 5 s com o estado salvo, três vezes seguidas;
5. os apps do dia a dia do usuário (terminal, navegador, editor, chat, reunião) na matriz com ✓ ou ◐
   documentado, nenhum ✗;
6. **compartilhamento de tela em reunião** (decisão do usuário): tela inteira e janela única, pelo portal, num
   navegador e num app Electron, em três chamadas reais de pelo menos 30 min em dias distintos, sem queda,
   sem o HUD nem o cofre na cópia; e o caminho aninhado documentado (o mundo inteiro pelo niri, uma janela
   pelo modo apresentação) testado uma vez na versão final;
7. a sessão niri continua instalada e funcional como alternativa, e a doc diz como voltar a ela.

Faltando qualquer um: a sessão fica opcional (`nkw` no greeter como segunda opção), o aninhado continua
suportado, e a próxima fase decide se insiste ou para.

## 7. Perguntas

Respondidas em 2026-10-24 (detalhe na seção "Decisões do usuário"):

- **2. Arrastar no NAV** — `ctrl+arrastar` em qualquer ponto da tela; clique simples seleciona (no FOCO, é
  do app); `world+arrastar` em todo modo e o único no FOCO.
- **3. Arranjos** — só as janelas já abertas; nada é lançado.
- **5. Barras** — HUD de jogo que absorve o papel da barra; barras layer-shell continuam na sessão, planas, com
  o HUD escondendo o que elas cobrem.
- **6. Captura** — compartilhar tela e janela numa reunião é condição do go/no-go; aninhado, pelo niri e pelo
  modo apresentação.

Ainda abertas (com a premissa em uso até a resposta):

1. **Monitores**: você usa dois ou mais? Lado a lado, um vertical? *Premissa*: multimonitor genérico (D11 para
   1..N); a resposta só decide se a vista fixa (`role = "view"`) entra na primeira entrega da D.
4. **Sessão própria**: você quer entrar no mundo pelo greeter neste ciclo (fase D, 6–8 semanas) ou prefere
   parar na C e seguir aninhado por mais tempo? *Premissa*: a D continua no roteiro, porque o compartilhamento
   de janela pelo portal e o go/no-go dependem dela.
7. **HUD de jogo como padrão** (nova): depois das capturas da onda 6, o estilo `game` vira o padrão no lugar do
   `cozy`, ou fica como opção?
