---
okf_version: "0.2"
type: Plan
title: Plano — o mundo (nkw), um gerenciador de janelas 3D aconchegante, em primeira pessoa, com os agentes como gatos
description: Decisões, especificação de modos e teclas, mundo, renderização, gatos, biblioteca, testes e roteiro por fases para o nkw — um compositor Wayland leve (Smithay + GLES próprio) que roda aninhado no niri e transforma o desktop num mundo low-poly no estilo Animal Crossing.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/office
tags: [plano, mundo, nkw, wayland, smithay, compositor, gles, glow, primeira-pessoa, gatos, low-poly, animal-crossing, teclado]
timestamp: 2026-10-21T22:00:00Z
generated: { by: agent:claude-code/claude-fable-5-1, at: 2026-10-21T22:00:00Z }
status: draft
stale_after: 2027-04-21T00:00:00Z
sources:
  - id: pesquisa-mundo
    resource: "pesquisa interna de 2026-10-21 (prior art de WMs 3D)"
    title: Pesquisa — Waylandcraft, waycraft (duas versões), Stardust XR, Simula, Safespaces/Arcan, xrdesktop/wxrd, Wayfire, Looking Glass, BumpTop, fsn, Eagle Mode; estética N64
  - id: smithay
    resource: https://github.com/Smithay/smithay
    title: Smithay — biblioteca Rust para compositores Wayland (backends winit/udev, GlesRenderer, protocolos)
  - id: anvil
    resource: https://github.com/Smithay/smithay/tree/master/anvil
    title: anvil — compositor de exemplo do Smithay (MIT), base de referência de código
  - id: smithay-egui
    resource: https://github.com/Smithay/smithay-egui
    title: smithay-egui — glow sobre o contexto EGL do GlesRenderer (padrão de GL próprio dentro do Smithay)
  - id: niri
    resource: https://github.com/YaLTeR/niri
    title: niri — compositor de rolagem em Smithay (GPL-3.0; referência de comportamento, nunca de código)
  - id: xwayland-satellite
    resource: https://github.com/Supreeeme/xwayland-satellite
    title: xwayland-satellite — Xwayland rootless como cliente Wayland comum
  - id: glow
    resource: https://github.com/grovesNL/glow
    title: glow — bindings GL/GLES finos
  - id: wayland-protocols
    resource: https://gitlab.freedesktop.org/wayland/wayland-protocols
    title: wayland-protocols — xdg-shell, viewporter, fractional-scale, presentation-time, xdg-activation, xdg-dialog
  - id: stardust
    resource: https://github.com/StardustXR/server
    title: Stardust XR — servidor de display XR (janelas 2D + objetos 3D)
  - id: simula
    resource: https://github.com/SimulaVR/Simula
    title: Simula — WM de VR (Godot + wlroots)
  - id: arcan
    resource: https://arcan-fe.com
    title: Arcan — display server + engine com Lua (Safespaces)
  - id: animal-crossing
    resource: https://en.wikipedia.org/wiki/Animal_Crossing_(video_game)
    title: Animal Crossing (Dōbutsu no Mori, N64, 2001) — referência de direção de arte
---

# Plano — o mundo (`nkw`)

O **mundo** é um compositor Wayland leve. Nele o desktop vira um lugar 3D aconchegante, visto em primeira pessoa:

- salas ao longo de um corredor;
- mesas com telas (as janelas), em faixas no estilo niri, e telas soltas no ar;
- uma biblioteca com os seus arquivos.

Tudo o que é vivo é um gato:

- os agentes do Claude Code são gatos aldeões nas suas mesas;
- os subagents são filhotes;
- as notificações chegam por gatos carteiros;
- a recepcionista é uma gata.

O teclado vem primeiro. Por padrão a navegação é discreta, entre estações, como no niri. Quando quiser, você anda livre com WASD, num modo que liga e desliga.

Pedido do usuário, em ordem:

1. Um WM que é um mundo 3D, onde você organiza as coisas: a biblioteca são os arquivos, as telas podem ficar no ar.
2. Os gatos são os seus agentes, e dá para conversar com eles.
3. Low-poly com herança "64", puxado para o **Animal Crossing**: arredondado, pastel, proporções de brinquedo, luz quente, caixas de diálogo com o texto aparecendo letra a letra.
4. **Primeira pessoa.**
5. Teclado e niri por padrão, com ANDAR (WASD) como modo ligado por um acorde.
6. **Extremamente leve**: ocioso perto de 0 % de CPU, binário pequeno, pouca RAM.

Nenhum projeto existente atende tudo isso (ver `pesquisa-mundo`), então o mundo é construído do zero sobre o Smithay.

Este plano segue a cultura do repo:

- TDD;
- lógica pura em módulos testados e cola fina com a engine;
- verificação por screenshot (`--shot`);
- o core Go como fonte da verdade dos agentes;
- Lua no core;
- a TUI e o `nkg` continuam funcionando;
- branches, nunca push em `main`.

## 0. Em uma página

- **Um binário novo, `nkw`**, em `gui/office`, em Rust. É um compositor Wayland em **Smithay** que roda **aninhado**: é uma janela dentro do niri. Os apps são lançados com o `WAYLAND_DISPLAY` do mundo. A sessão própria (DRM/KMS) é a fase 7, opcional.
- **O renderizador é próprio, em GLES 3.0 via `glow`**, no mesmo contexto EGL do `GlesRenderer` do Smithay.
  - O mundo é desenhado numa textura.
  - A janela em FOCO é composta pelo caminho 2D do Smithay, em 1:1 e nítida.
  - Não usamos Bevy nem wgpu.
- **Estilo "aconchego"**:
  - low-poly arredondado com cor por vértice;
  - half-lambert em 3 faixas;
  - sombras-bolha e névoa quente;
  - luz conforme a hora do dia.
  O preset "64" é opcional: baixa resolução, filtro 3-point e dither.
- **Modos**: NAV (estações, niri) · ANDAR (WASD) · FOCO (1:1, as teclas vão para a janela) · CONVERSA (com o gato à sua frente) · SALTO (rótulos tipo vimium/avy) · COMANDOS (paleta).
- **As teclas moram no keymap do core.**
  - São comandos `scope: client` chamados `world.*`, nos modos `normal.world` e `world.*`.
  - Dá para religar todas em Lua.
  - Existe uma **tecla do mundo** (`world`): `super` na sessão própria e `ctrl+alt` aninhado.
  - Sair do FOCO é `ctrl+]`, como na TUI.
  - ANDAR é `world+w` (e `w` no NAV).
- **Os agentes chegam pelo protocolo v1, via `nk-client`.** Só há mudanças aditivas pequenas no core.
  - O terminal de um gato é uma **superfície interna** (alacritty_terminal + rasterizador), não um cliente Wayland.
- **Crates**:
  - `nk-cats`: os módulos puros extraídos do `nkg`;
  - `nkw-core`: puro, sem Smithay e sem GL;
  - `nkw-gl`;
  - `nkw` (o binário);
  - `nkw-testclient`.
- **A leveza vem do desenho**:
  - o laço `calloop` não arma nenhum timer quando está ocioso;
  - só se redesenha quando há dano;
  - o mundo congela no FOCO;
  - telas distantes recebem callbacks de quadro com taxa limitada;
  - o cursor é o do host;
  - o `nk-client` deixa de fazer polling.

## 1. Objetivos, não-objetivos e orçamentos

### 1.1 Objetivos

1. Usar no dia a dia, aninhado no niri: terminais, navegador e apps GTK/Qt organizados em salas e mesas, com digitação e leitura sempre em 1:1.
2. Os agentes como gatos, com quem você conversa: responder perguntas, mandar mensagens, parar, renomear e mover, tudo sem sair do mundo.
3. Teclado total. Mouse opcional.
4. Leveza mensurável (§1.3), verificada por script e não por impressão.
5. Extensível pelo mesmo Lua do core (`init.lua`).

### 1.2 Não-objetivos (até a fase 6)

- VR/XR, física, multiplayer, PBR, shadow maps, pós-processamento pesado.
- Conteúdo 3D vindo de clientes. Os clientes são sempre janelas 2D; o 3D é do mundo.
- Editor visual do mundo. O layout se faz por teclado e pelo ANDAR.
- Substituir o niri como sessão padrão.
- Abas em colunas, gestos de touchpad, tablet, toque (só na fase 7, se ela acontecer).
- Leitor de tela para o mundo em si. Os apps dentro dele continuam com o AT-SPI deles.

### 1.3 Orçamentos

Medidos na máquina alvo (Intel Iris Xe, Mesa, niri, 1080p a 60 Hz) com o `nkw` aninhado. O orçamento é só do processo `nkw` e das threads dele; o `nk serve` mede-se à parte. Referência: o `nkg` (Bevy) gasta de 5 a 7 ms de CPU por quadro, ~10 % de um núcleo animado e ~1 % com movimento reduzido, e tem 82 MB de binário.

| métrica | cenário | alvo | falha acima de | como medir |
|---|---|---|---|---|
| CPU ocioso | S1: NAV parado, 3 janelas, core demo com `--reduced-motion` | ≤ 0,1 % de um núcleo | 0,5 % | Δ(utime+stime) de `/proc/PID/stat` em 60 s |
| wakeups ociosos | S1 | ≤ 1/s | 5/s | Σ Δ(voluntary + nonvoluntary ctxt switches) de `/proc/PID/task/*/status` |
| digitando no FOCO | S2: foot, 15 teclas/s por `--script` | ≤ 1 % | 3 % | como S1 |
| mundo animado | S3: demo ocupado, 5 gatos ativos e 1 carteiro | ≤ 3 % | 6 % | como S1 |
| CPU por quadro do mundo | S3/S4 | p50 ≤ 1 ms, p95 ≤ 2 ms | p95 3 ms | `NKW_STATS=1` (Instant em volta de update, cull e submit) |
| GPU por quadro | S4 a 1080p, MSAA 4× | ≤ 2 ms | 4 ms | `NKW_STATS=gpu` (`GL_TIME_ELAPSED`, EXT_disjoint_timer_query) |
| câmera em movimento | S4: deslizes h/l a cada 1 s; S5: laço de ANDAR | 60 fps, 0 quadros perdidos | 2 perdidos seguidos | quadros enviados vs callbacks de quadro do host |
| latência de tecla | S2 | tecla repassada ao cliente na mesma iteração do laço (< 1 ms) | 1 quadro | carimbo de tempo no `nkw-testclient` |
| RAM | partida sem janelas / 8 janelas | ≤ 90 MB / ≤ 110 MB RSS (eram 45 MB na partida; o driver Mesa sozinho ocupa ~48 MB, medido com `NKW_MEM=1`) | 150 MB | `VmRSS`/`VmHWM` (buffers de GPU do cliente não contam) |
| binário | release, stripped | ≤ 15 MB | 25 MB | `stat -c %s` |
| partida | até o 1º quadro / até os gatos aparecerem | ≤ 250 ms / ≤ 1 s | 500 ms / 2 s | `NKW_STATS` a partir do `starttime` do processo |
| build | release a frio / incremental (mexer no `nkw-core`) | ≤ 4 min / ≤ 15 s | 8 min / 30 s | `time cargo build` |
| desenho | sala típica | ≤ 80 draw calls, ≤ 100k triângulos | 200 draws | contadores do `NKW_STATS` |

### 1.4 Como medimos

- `gui/office/scripts/world-perf.sh` sobe o `nkw` com um cenário e dirige tudo por **`--script`**: entrada sintética que entra pelo mesmo caminho do teclado. Ele amostra `/proc`, coleta as estatísticas e compara com `gui/office/crates/nkw/perf-budget.toml`. Falha se passar do limite ou se regredir mais de 20 % sobre a última medida registrada.
- `make world-perf` roda os cenários S1 a S5. Roda localmente, não no GitHub, porque precisa de GPU. Cada fase registra os números em `docs/log.md`.
- O backend headless (llvmpipe) também roda os cenários, mas só como **regressão relativa**. O tempo absoluto de GPU no llvmpipe não é comparável.

## 2. Decisões

### D1 — Aninhado primeiro; a sessão própria fica por último e é opcional

- **Decisão.** Até a fase 6, o `nkw` é uma janela do niri. Ele usa o backend winit do Smithay e cria o seu próprio socket Wayland (`nkw-0`).
- **Por quê.**
  - Risco zero para a sessão do usuário.
  - Iteração rápida.
  - O niri continua cuidando do que é difícil e não diferencia o produto: monitores, bloqueio de tela, portais, screencast, energia, VT.
  - Um crash do mundo derruba só as janelas do mundo.
- **Revisitar** depois da fase 6, se o mundo virar o lugar onde você passa o dia (§10, fase 7).

### D2 — Smithay

- **Decisão.** Smithay, a mesma base do niri e do cosmic-comp, com o anvil (MIT) como referência de código.
- **Por quê.**
  - É Rust, como `gui/office`.
  - É modular: pegamos só os protocolos de que precisamos.
  - Tem backends winit e udev/DRM, `GlesRenderer` com acesso ao GL cru, dmabuf, xdg-shell, data-device, fractional-scale, presentation-time e xdg-activation.
  - Tem `OutputDamageTracker` para a composição 2D.
- **Descartadas.**
  - *wlroots*: é C, as bindings Rust estão paradas e o 3D próprio precisaria de GLES cru do mesmo jeito.
  - *Arcan*: um display server inteiro, com protocolo próprio e Wayland por ponte, mais uma segunda linguagem de script competindo com o Lua do core.
  - *Estender o Stardust*: é XR em primeiro lugar, com modelo de cliente próprio; o modo de tela plana é secundário e o FOCO 1:1 por teclado brigaria com o modelo de objetos dele.
  - *Dentro de uma engine* (Waylandcraft/Minecraft, Simula/Godot): o oposto de leve.
- **Licença.** O niri é GPL-3.0. Ele serve para entender comportamento (a faixa de colunas, presets de largura, throttling de callbacks), **nunca para copiar código** num repo MIT.

### D3 — Renderizador: GLES 3.0 próprio (glow) no contexto do Smithay, em duas camadas

**Decisão.** O `nkw-gl` desenha o mundo com `glow` sobre o contexto EGL do `GlesRenderer`. O contexto glow é criado com `get_proc_address` do EGL do Smithay, no mesmo padrão do smithay-egui.

- **Camada do mundo.** É um passe 3D numa textura offscreen (MSAA, resolvida antes do uso). Recebe como textura as janelas que o Smithay já importou, pelo `tex_id` do `GlesTexture`, inclusive os dmabufs importados como EGLImage.
- **Camada 2D.** O próprio Smithay compõe, com o `OutputDamageTracker`, nesta ordem:
  1. a textura do mundo (`TextureRenderElement`);
  2. por cima, a janela em FOCO e os popups dela, em 1:1 e coordenada inteira;
  3. as sobreposições (HUD, crachás de nome, caixa de diálogo, rótulos de SALTO), rasterizadas na CPU como `MemoryRenderBuffer`;
  4. o cursor, só na sessão própria.

O GL cru roda **fora** do `GlesFrame` (`renderer.with_context`), antes da composição. Ao terminar, devolve o estado conhecido: FBO, viewport, programa 0, VAO 0, blend `ONE, ONE_MINUS_SRC_ALPHA`, sem depth test, `TEXTURE0`. Há um teste para isso.

**Por quê.**
- O caminho maduro de renderização do Smithay é GLES (e Pixman em software). Os buffers dos clientes já viram texturas GL ali, então ficar em GL é **cópia zero**.
- No FOCO, a janela não passa pelo 3D: digitar custa o mesmo que num compositor comum.
- O dano do FOCO é exato, porque o mundo não muda.

**Descartadas.**
- *Bevy*: medimos 5 a 7 ms por quadro, 82 MB de binário e 17 min de build. Ele é dono do laço e do grafo de render, não aceita buffers de cliente sem cópia, e o ECS não paga a conta com menos de 200 objetos.
- *wgpu*: importar dmabuf de cliente exige memória externa do Vulkan e modifiers DRM via `wgpu-hal` com `unsafe`. Seria um segundo contexto de GPU, mais naga na partida e mais megabytes, para um visual que o GLES 3.0 faz com folga.

**Plano B**, se o glow no contexto do Smithay falhar no spike: desenhar o 3D com shaders de textura customizados do próprio `GlesFrame`, com os cantos dos quads projetados na CPU e UV com perspectiva correta (coordenada homogênea). É mais limitado, mas basta para quads de janela.

### D4 — Backend do host: winit agora, cliente SCTK próprio só se precisar

- **Decisão.** Até a fase 5 usamos o backend winit do Smithay.
  - O keymap XKB vem do `world.toml`, com fallback para `XKB_DEFAULT_*` e depois `us`.
  - O cursor é o do host (`set_cursor` ou cursor custom do winit), para mover o mouse não forçar redesenho.
  - A trava de ponteiro (pointer lock) para olhar com o mouse no ANDAR também vem do winit.
- **Na fase 6**, escrevemos um backend de host próprio com smithay-client-toolkit **se** algum destes critérios bater:
  - o keymap configurado divergir do uso real (opções XKB, compose);
  - quisermos `keyboard-shortcuts-inhibit` (para usar `super` aninhado);
  - quisermos o presentation-time do host;
  - quisermos o fractional-scale exato do host.
  Com SCTK dá para **espelhar o keymap do niri** byte a byte.

### D5 — XWayland via `xwayland-satellite`, sob demanda

- **Decisão.** Nada de XWM próprio. O mundo abre os sockets X11 e sobe o `xwayland-satellite` na primeira conexão, como o niri faz.
- O satellite é um processo à parte e não é linkado; o que importa é ele estar instalado. A lista de protocolos que ele exige é conferida na versão usada, na fase 6.

### D6 — O terminal de um gato é uma superfície interna

- **Decisão.** O monitor de um gato mostra uma de duas coisas:
  - o **tail** (`monitor.rs`), quando não há painel aberto;
  - o **terminal do painel**: WebSocket do core, depois `nk_client::Term` (alacritty_terminal), depois o rasterizador fontdue.
- No FOCO, esse buffer é composto em 1:1 e as teclas vão para o `PaneConn` com `encode_key`. É o mesmo caminho do `nkg` e da TUI.
- **Por quê.** É a mesma semântica de painel do core (reusar, anexar, retomar), não depende de um terminal externo e já está testado. "Abrir no foot" pode vir depois como comando.

### D7 — Reuso e crates

- **O `nk-client` é reusado inteiro.**
- **Os módulos puros do `nkg` são extraídos para `nk-cats`:**
  - `model.rs` (estado → pose, paleta);
  - `catpose.rs`;
  - `inbox.rs` (a fila);
  - `monitor.rs` (o feed do tail);
  - `raster.rs` (fontdue);
  - `keys.rs` (o Router).

  Nenhum deles importa Bevy; só `strip.rs` usa `bevy::math`, e ele fica no `nkg`. O `nkg` passa a depender de `nk-cats` **sem mudar de comportamento**: os testes se mudam junto e a screenshot sai igual.
- **`core_io.rs` serve de molde** para o `nkw/src/core_link.rs`, que acorda o `calloop` em vez do winit. As conversões puras (snapshot → `Sess`) vão para `nk-cats`.

### D8 — Direção de arte: Animal Crossing, com o "64" opcional

- **Padrão "aconchego"**:
  - low-poly arredondado com cor por vértice;
  - half-lambert em 3 faixas;
  - ambiente hemisférico (céu/chão);
  - sombras-bolha e névoa quente;
  - paleta pastel;
  - hora do dia;
  - balões e caixas de diálogo arredondados, com texto letra a letra.
- **Preset `64`**: mundo em baixa resolução (~270 a 360 px de altura) com upscale nearest, filtro 3-point nas texturas do mundo e dither Bayer 4×4. Desligado por padrão.
- **Nunca**: warping afim nem snapping de vértice.
- **O FOCO é sempre 1:1**, em qualquer preset.

### D9 — Primeira pessoa, sem avatar

- Você é a câmera, no olho a 1,5 m.
- Patinhas aparecem só segurando uma tela (fase 5, opcional).
- No CONVERSA, o gato se vira para você, como os aldeões do AC.

### D10 — Teclas e scripts no core; preferências e estado do mundo no cliente

- **Os comandos `world.*` são `scope: client` registrados no core.** Assim aparecem em `/v1/commands`, `/v1/keymap`, na ajuda, na paleta e no `nk.keymap.set`.
- **Preferências e estado do mundo** (layout, biblioteca) ficam no `nkw` (§4.7). Não são domínio de agentes.
- **Lua continua só no core** (§8).

### 2.11 Layout dos crates e regras de dependência

```
gui/office/crates/
  nk-client/       (existe) protocolo v1, Term, codificação de teclas
  nk-cats/         (novo, puro) model, catpose, inbox, monitor, raster, keys (extraídos do nkg)
  nkg/             (existe) Bevy; passa a usar nk-cats
  nkw-core/        (novo, puro) mundo, modos, câmera, colisão, colocação, agendador,
                   hints, diálogo, linha de texto, modelo da biblioteca, persistência, lista de desenho
  nkw-gl/          (novo) GLES3 via glow: malhas procedurais, shaders, passes, alvo offscreen
  nkw/             (novo, bin) Smithay: backends (winit, headless, depois udev), protocolos,
                   superfícies→texturas, entrada, core_link (nk-client), FS da biblioteca, launcher
  nkw-testclient/  (novo, dev) cliente Wayland mínimo (wayland-client + SCTK) para testes
```

| crate | pode depender de | não pode |
|---|---|---|
| `nk-cats` | `fontdue` | Bevy, Smithay, GL, rede, threads |
| `nkw-core` | `nk-cats`, `glam`, `serde`, `serde_json`, `toml`, `nucleo-matcher` | Smithay, `glow`, `wayland-*`, `calloop`, `nk-client`, `std::process`, FS direto (o FS entra por trait `FsPort`; o tempo entra como parâmetro `now`) |
| `nkw-gl` | `glow`, `bytemuck`, `nkw-core`, `nk-cats` | Smithay, `nk-client` |
| `nkw` | tudo acima + `smithay`, `calloop`, `nk-client`, `png` | — |

As regras são verificadas por `nkw-core/tests/deps.rs`, que roda `cargo metadata` e checa o fecho de dependências normais. É o equivalente Rust do `internal/archtest`.

O perfil `[profile.world]` (herda `release`) usa `lto = "thin"`, `codegen-units = 1` e `strip = true`. Fica num perfil separado para não alongar o build do `nkg`. A feature do Smithay usada é a mínima: winit, EGL, renderer GL, wayland frontend e desktop; udev/DRM/libinput/libseat só na fase 7.

## 3. Modos e entrada

### 3.1 Princípios

1. **Modal como o vim.** O NAV é o "normal". Teclas cruas vão para uma janela só no FOCO, e para texto só nas linhas de texto (CONVERSA, COMANDOS, SALTO).
2. **`esc` volta**, menos no FOCO, onde o `esc` é da janela. Lá a saída é `ctrl+]`, igual à TUI e ao `nkg`.
3. **Nada rouba o foco.** Gato chegando, confirmação ou pedido de ativação viram selo no HUD. Você decide quando atender.
4. **Ler e digitar é sempre em 1:1.** O 3D é para se orientar, não para ler.
5. **O modo atual está sempre visível** no HUD: `NAVEGAR`, `ANDAR`, `FOCO`, `CONVERSA`, `SALTO`, `COMANDOS`.

### 3.2 A tecla do mundo e o escape

- **`world`** é um modificador abstrato que aparece nos nomes de tecla do keymap (`world+w`). Ele se resolve pelo `world.toml`:
  - `keys.world_mod = "auto"` vira **`super`** na sessão própria (ou com `keyboard-shortcuts-inhibit` no host, fase 6);
  - e vira **`ctrl+alt`** quando aninhado.
- **Por que não `super` aninhado.** O niri consome `Mod` antes de nós: `Mod+H/J/K/L`, `Mod+1..9`, `Mod+Q`, `Mod+Escape` e quase todo `Mod+letra` no config padrão.
- **Por que não `alt`.** O `alt` é o meta dos terminais (readline, emacs). Já `ctrl+alt+letra` quase não é usado por apps e o niri padrão não o liga, fora `ctrl+alt+delete`.
- **Os acordes `world+` são raros por desenho.** No NAV tudo é tecla simples. O acorde só serve para agir **sem sair do FOCO**: pular de tela, chamar a inbox, ANDAR.
- **O escape do FOCO é `ctrl+]`** (`keys.focus_escape`, configurável). É a única tecla interceptada no FOCO além dos acordes `world+`. Quem usa `ctrl+]` no vim pode trocá-la.
- **Por que `world+w` e não `mod+l` para ANDAR.**
  - `world+h/j/k/l` fica com a mesma função do niri (tela vizinha), para não quebrar a memória muscular.
  - O `w` é a tecla de andar para frente, então a mão já está no WASD.
  - No NAV, `w` sozinho também liga o ANDAR. Para não andar sem querer, toda tecla já pressionada ao entrar num modo só passa a contar depois de solta.

### 3.3 Máquina de estados

Há dois **modos de base**: NAV e ANDAR. Por cima deles fica no máximo uma **sobreposição**: FOCO, CONVERSA, SALTO ou COMANDOS. Cada sobreposição guarda para onde voltar: o modo de origem e a pose da câmera. Isso está em `nkw-core/src/mode.rs`, que é puro (ação entra, efeitos saem) e é testado por tabela.

```
            w / world+w                     enter / i (tela)
   ┌──────────────────────────┐     ┌──────────────────────────┐
   ▼                          │     │                          ▼
 ANDAR ◄────────────────────► NAV ──┴─ t / shift+space ──► CONVERSA ── f ──► FOCO (terminal do gato)
   │      esc / world+w        │  ▲                            │
   │ enter (mira)              │  └──────── ctrl+] ───── FOCO ◄┘ (FOCO volta para quem o abriu)
   └────────────► FOCO ────────┘
  f → SALTO (de NAV/ANDAR) · : ctrl+p world+p → COMANDOS (de NAV/ANDAR/FOCO) · esc volta
```

| de | gatilho | para | efeitos |
|---|---|---|---|
| NAV | `enter`/`i` numa tela | FOCO | desliza até a pose 1:1 (≤ 260 ms); ao chegar, o quad vira elemento 2D; foco de teclado e `activated` na janela; o mundo congela |
| NAV | `enter`/`i` na mesa de um gato | FOCO (terminal do gato) | abre ou anexa o painel (`POST /v1/panes`); redimensiona para caber no retângulo de foco |
| NAV | `w` / `world+w` | ANDAR | trava o ponteiro (se `walk.mouse_look`); mostra a mira; teclas pressionadas ignoradas até soltar |
| NAV | `t` | CONVERSA (gato da estação) | o gato vira na cadeira; a câmera enquadra o rosto (≤ 260 ms) |
| NAV / ANDAR | `shift+space` (inbox.summon) | CONVERSA (o mais urgente) | o gato já veio até você ou aparece ao seu lado; nada de viagem de câmera |
| NAV / ANDAR | `f` / `F` | SALTO | rótulos em tudo o que é visível e interativo |
| NAV / ANDAR / FOCO | `:` `ctrl+p` / `world+p` | COMANDOS | a paleta; ao terminar volta, ou vai para o modo que o comando pedir |
| ANDAR | `esc` / `world+w` | NAV | regra de saída (§3.5) |
| ANDAR | `enter` com tela na mira | FOCO | ao sair do FOCO, volta para o ANDAR na mesma pose |
| ANDAR | `enter`/`t` com gato na mira | CONVERSA | `esc` volta para o ANDAR |
| FOCO | `ctrl+]` | modo de origem | `wl_keyboard.leave`; popups fechados (`popup_done`); a câmera recua; o mundo descongela |
| FOCO | `world+h/l/j/k` | FOCO na vizinha | travelling lateral até a pose 1:1 da vizinha; o foco passa junto |
| FOCO | `world+space` | CONVERSA (o mais urgente) | ao terminar a conversa, volta ao FOCO (resposta rápida sem perder o lugar) |
| FOCO | `world+w` | ANDAR | sai do foco e anda dali |
| FOCO | a janela fechou | NAV | a próxima tela da mesa fica selecionada |
| CONVERSA | `i` / `r` | CONVERSA, linha de texto | `enter` envia, `esc` cancela, `shift+enter` quebra a linha |
| CONVERSA | `f` | FOCO (terminal do gato) | ao sair, NAV na mesa do gato |
| CONVERSA | `esc` / `q` | modo de origem | o gato volta a trabalhar |
| SALTO | rótulo | estação (tela/mesa), FOCO (`F`), CONVERSA (gato), sala (porta), abrir (livro) | |
| SALTO / COMANDOS | `esc` | modo de origem | |
| qualquer | gato chegou, `confirm.requested`, xdg-activation | não muda | selo no HUD; com token válido de ativação no NAV, só seleciona a estação |

### 3.4 NAV: estações em primeira pessoa

Uma **estação** é uma pose de câmera com um alvo:

| estação | alvo |
|---|---|
| `Desk {room, desk, slot}` | uma mesa, de pé no corredor entre as mesas, de frente para a pilha de telas; `slot` é a tela selecionada |
| `Air(screen)` | uma tela no ar |
| `Shelf {dir, row}` | uma estante da biblioteca |
| `Door(room)` | a porta de uma sala |
| `Board` | o quadro de avisos da recepção |
| `Free(pose)` | a posição onde você saiu do ANDAR (§3.5) |

Comportamento:

- **`h`/`l`** fazem travelling **lateral**: a câmera desliza de lado sem girar, como a rolagem do niri.
- **`J`/`K`** levam de sala em sala pelo corredor. Se o percurso passa de 8 m, vira corte com fade de 120 ms na cor do céu.
- **A seleção é a tela marcada** na mesa (contorno suave e crachá com o título). No NAV não há foco de teclado: os terminais mostram o cursor vazado, sinal de que você não está digitando ali.
- **Mouse (opcional).** Passar por cima destaca, por raio na CPU e só quando o objeto sob o mouse muda. Clique seleciona; clique na tela já selecionada leva ao FOCO; a rodinha faz `h`/`l`.

Teclas padrão (todas religáveis, §3.12):

| tecla | comando | o quê |
|---|---|---|
| `h` `l` / `←` `→` | `world.left` / `world.right` | estação anterior/próxima na sala |
| `j` `k` / `↓` `↑` | `world.down` / `world.up` | tela de baixo/de cima na pilha da mesa |
| `J` `K` | `world.room_next` / `world.room_prev` | próxima/anterior sala |
| `1`…`9` | `world.room_1`…`world.room_9` | sala N |
| `gg` `G` | `world.first` / `world.last` | primeira/última mesa |
| `enter` `i` | `world.focus` | FOCO na tela selecionada (numa mesa de gato: o terminal dele) |
| `t` | `world.talk` | conversa com o gato desta mesa |
| `w` | `world.walk` | ANDAR |
| `f` `F` | `world.hint` / `world.hint_focus` | SALTO / SALTO direto para o FOCO |
| `o` | `world.overview` | mapa (§3.10) |
| `b` | `world.library` | vai à biblioteca |
| `H` `L` | `world.move_left` / `world.move_right` | leva a tela para a mesa vizinha |
| `,` `.` | `world.consume` / `world.expel` | junta a tela na pilha da mesa à esquerda / tira para uma mesa só dela |
| `-` `=` | `world.narrower` / `world.wider` | largura da mesa (presets 1/3, 1/2, 2/3, 1 da tela) |
| `p` | `world.pin` | prende a tela no ar, à sua frente |
| `M` | `world.send_room` | manda a tela para outra sala (pergunta qual) |
| `T` | `world.terminal` | terminal novo numa mesa nova à direita |
| `Q` | `world.close` | fecha a janela (`xdg_toplevel.close`, sem forçar) |
| `z` | `pane.zoom` | esconde/mostra o HUD (como no `nkg`) |

Comandos do core que valem no NAV por herança de `normal`. Agem sobre o gato da estação, que vira `context.session_id`:

| tecla | comando | efeito no mundo |
|---|---|---|
| `r` | `session.reply` | abre a CONVERSA já na linha de texto |
| `x` | `session.stop` | para o agente; a confirmação vem na caixa de diálogo |
| `n` | `session.rename` | renomeia; o crachá muda |
| `m` | `session.move` | muda o contexto; **o gato anda até a outra sala** |
| `X` | `session.archive` | arquiva; o gato se muda |
| `dd` | `session.remove` | remove |
| `c` | `session.new` | gato novo |
| `a` `A` | `nav.next_attention` / `nav.prev_attention` | vai ao próximo gato que te espera |
| `shift+space` | `inbox.summon` | chama o mais urgente |
| `:` `ctrl+p` | `palette.open` | COMANDOS |
| `?` | `help.open` | ajuda |
| `ctrl+k` | `switcher.open` | SALTO só entre os gatos |

Numa estação sem gato, os comandos de sessão ficam indisponíveis (`409`) e não fazem nada.

**O mundo não liga `app.quit` (`q`).** Fechar o compositor fecha todas as janelas, então sair é `world+shift+e`, com confirmação.

### 3.5 ANDAR: livre em primeira pessoa

Teclas:

| tecla | comando | o quê |
|---|---|---|
| `w` `s` `a` `d` (segurar) | `world.forward` / `back` / `strafe_left` / `strafe_right` | andar |
| `shift` (segurar) | `walk.run_key` no `world.toml` | correr |
| `q` `e` / `←` `→` | `world.turn_left` / `world.turn_right` | girar (suave ao segurar, ou um passo por toque com `walk.turn = "snap"`) |
| `↑` `↓` | `world.look_up` / `world.look_down` | olhar para cima/baixo |
| `c` | `world.recenter` | olhar para o horizonte |
| `enter` `space` | `world.interact` | usa o que está na mira: tela vira FOCO, gato vira CONVERSA, porta leva à sala, estante abre |
| `g` | `world.grab` | pega/solta a tela na mira |
| `[` `]` | `world.nearer` / `world.farther` | com tela na mão: aproxima/afasta (0,8 a 3 m) |
| `-` `=` | `world.narrower` / `world.wider` | com tela na mão: tamanho |
| `f` | `world.hint` | SALTO |
| `esc` / `world+w` | `world.walk_exit` | volta ao NAV |

Parâmetros (`[walk]` e `[camera]` no `world.toml`):

| parâmetro | padrão | faixa |
|---|---|---|
| velocidade andando | 2,8 m/s | 1 a 5 |
| correndo | ×2 | ×1 a ×3 |
| aceleração / frenagem | 10 m/s² (chega a andar em ~0,28 s) / 18 m/s² (para em ~0,16 s) | — |
| giro suave | 110°/s, rampa de 0,12 s | 45 a 240°/s |
| giro em passos | 30° por toque, instantâneo | 15 / 30 / 45 |
| pitch | ±55°, 80°/s | — |
| mouse | 0,12° por contagem, sem aceleração | — |
| altura do olho | 1,5 m | 1,2 a 1,8 |
| FOV vertical | 60° (≈ 91° horizontal em 16:9), **fixo** | 50 a 75 |
| head bob | 0 (desligado) | 0 a 1 |
| raio do jogador / alcance da mira | 0,3 m / 4 m | — |
| passo de simulação | fixo 1/120 s, com subpassos | — |

**Olhar só com o teclado** funciona sempre (`q`/`e`/setas). **Com o mouse** (`walk.mouse_look = true`):

- ao entrar no ANDAR, o ponteiro fica travado (aninhado: `set_cursor_grab(Locked)` do winit; sessão própria: o ponteiro já é nosso) e os movimentos relativos giram a câmera;
- ao sair, ele destrava;
- botão esquerdo faz `interact`, botão direito faz `grab`.

**Colisão.** Fica em `nkw-core/src/collide.rs`, que é puro:

- Cada sala tem uma grade de ocupação de 0,25 m feita das AABBs dos móveis e dos gatos (inflados pelo raio do jogador) e das paredes, com vãos de porta de 1,2 m.
- O movimento é integrado e resolvido **por eixo** (x, depois z). Se a célula de destino está ocupada, zera aquele componente e você escorrega rente à parede.
- A 6 m/s × 8,3 ms, o passo é bem menor que uma célula, então não há tunelamento.
- Telas no ar só bloqueiam se a borda de baixo estiver abaixo de 1,9 m.
- Não há pulo nem andares: o mundo é um plano.

**Carregar uma tela** (`g`):

- A tela solta da mesa e flutua a 1,2 m na sua frente, sempre virada para você.
- `g` de novo solta, escolhendo o primeiro que servir:
  1. **slot de mesa** a menos de 0,6 m: entra na pilha;
  2. **parede** a menos de 0,4 m e alinhada em até 15°: encosta a 5 cm, presa à grade de 0,25 m;
  3. **no ar**, na pose atual, virando estação `Air`.
- Telas nunca têm roll.

**Saída do ANDAR** (`walk.on_exit`, padrão `"auto"`):

- `auto`:
  - se existe uma estação a menos de 1,5 m e com até 40° de diferença de direção, desliza até ela (≤ 200 ms);
  - senão, você fica onde está, numa estação `Free` ("você está aqui").
  A partir de `Free`:
  - `h`/`l` vão à estação mais próxima à esquerda/direita da direção do olhar;
  - `enter` usa o que está no centro da vista;
  - `t` fala com o gato mais próximo à vista.
  A `Free` é esquecida no primeiro movimento de NAV.
- `snap` sempre vai à estação mais próxima (com corte se passar de 8 m).
- `stay` nunca encaixa.

### 3.6 FOCO: a janela em 1:1

- O foco de teclado e o ponteiro vão para a janela.
- O `KeyboardHandle::input` do Smithay recebe um filtro que intercepta só `ctrl+]` e os acordes `world+`. O resto vai para o cliente, com repeat e modificadores do seat.
- Clicar fora do retângulo da janela, no mundo escurecido em volta, sai do FOCO.
- Popups (menus, completions) só existem no FOCO e são desenhados em 2D. Ao sair do FOCO recebem `popup_done`.
- O mundo em volta fica **congelado e escurecido em 20 %** (`focus.freeze_world = true`). O passe do mundo não roda enquanto você digita.
- O HUD encolhe a um selo discreto, que só aparece se alguém te espera.

### 3.7 CONVERSA: com o gato à sua frente

A caixa de diálogo é no estilo AC:

- caixa creme arredondada na base da tela;
- plaquinha com o nome do gato na cor do contexto;
- texto aparecendo letra a letra a ~45 caracteres/s (`space`/`enter` mostra tudo de uma vez);
- um "▼" pulsando quando há mais.

As escolhas aparecem numa caixinha à direita: `j`/`k` ou `1`…`9`, e `enter`. O conteúdo e o mapeamento para o core estão em §6.4. Teclas próprias (`world.talk`):

| tecla | comando | o quê |
|---|---|---|
| `space` `enter` | `world.talk_next` | avança |
| `i` `r` | `world.talk_reply` | linha de texto para responder |
| `f` | `world.talk_sit` | "sentar na mesa": FOCO no terminal do gato |
| `esc` `q` | `world.talk_end` | encerra |

Os comandos do core caem por herança, com o gato como contexto: `x`, `n`, `m`, `X`.

### 3.8 SALTO (hints)

- `f` põe rótulos de 1 ou 2 letras em todo objeto visível com que dá para interagir: telas, gatos, filhotes, portas, estantes e livros à vista, telas no ar.
- O alfabeto é `hint.alphabet = "asdfghjkl"`. Os rótulos não são prefixo uns dos outros, e os mais curtos vão para o que está mais perto do centro da vista.
- No mapa (§3.10), os rótulos cobrem todas as salas e mesas.
- Digitar o rótulo leva ao alvo: seleciona a tela, ou vai direto ao FOCO com `F`; abre a CONVERSA com o gato; passa pela porta; abre o livro.
- `backspace` apaga; `esc` cancela.

### 3.9 COMANDOS (paleta)

Uma linha de texto com lista filtrada por fuzzy (`nucleo-matcher`). A lista junta:

- os comandos do core disponíveis para o contexto (`GET /v1/commands?session_id=`);
- os comandos `world.*`;
- **apps** (arquivos `.desktop` de `XDG_DATA_DIRS`, lidos na primeira abertura e guardados em cache; `world+d` abre a paleta já nos apps);
- "ir para" uma sala, um gato ou uma tela pelo nome (é o `/`, `view.filter`, no NAV);
- `lua <código>`, que vai para `POST /v1/lua/eval`.

Edição: `ctrl+a`/`e`/`w`/`u`, `tab` completa, `↑`/`↓` ou `ctrl+n`/`p` navegam, `enter` executa, `esc` cancela. Argumentos (`name`, `context`, `dir`, `prompt`) são pedidos em sequência na mesma linha, como na TUI.

### 3.10 Mapa (overview)

- `o` faz um fade para uma vista **ortográfica de cima** da sala atual, no estilo do mapa do AC. `o o` mostra o andar inteiro.
- `h`/`j`/`k`/`l` movem um destaque; `enter` vai para lá com fade; `esc` volta.
- O mapa é uma sub-vista do NAV, não um modo. É troca de vista e não um voo de câmera: nada de enjoo.

### 3.11 Conforto e movimento reduzido

- **O FOV é fixo.** Zoom é dolly, nunca animação de FOV.
- **Nunca há roll.** O horizonte é estável; as estações têm pitch 0 ou alinhado à tela.
- **Deslizes curtos**: ≤ 260 ms, ease-in-out cúbico, velocidade máxima limitada. Acima de 8 m, corte com fade de 120 ms.
- **`h`/`l` são travelling lateral**, sem guinada.
- **A câmera anda na taxa do monitor.** Só os gatos animam de 15 a 30 fps; câmera a 30 fps dá enjoo.
- **Head bob desligado.** Vinheta de conforto opcional (`walk.comfort_vignette`), que escurece a periferia ao andar ou girar. Giro em passos disponível.
- **Objetos a menos de 0,6 m da câmera somem com dither**, então a câmera nunca "entra" num gato ou num móvel.
- **Movimento reduzido**: liga com `--reduced-motion`, com `NKW_REDUCED_MOTION=1` ou com a preferência do sistema lida pelo portal Settings (`org.gnome.desktop.interface enable-animations`). Com ele:
  - cortes em vez de deslizes;
  - gatos em pose fixa por estado;
  - carteiros aparecem já parados;
  - texto da conversa inteiro de uma vez;
  - ANDAR continua (quem move é você), com giro em passos por padrão.

### 3.12 Keymap no core e em Lua

Modos e ordem de resolução (feita pelo `nkw`; o Router do `nk-cats` ganha suporte a uma lista de modos):

| modo (keymap) | quando | ordem de resolução |
|---|---|---|
| `world` | acordes globais, em todo modo do mundo, inclusive FOCO | sempre primeiro |
| `normal.world` | NAV | `world` → `normal.world` → `normal` → `*` |
| `world.walk` | ANDAR | `world` → `world.walk` → `*` |
| `world.focus` | FOCO | `world` → `world.focus`; **sem fallback**, o resto vai para a janela |
| `world.talk` | CONVERSA | `world` → `world.talk` → `normal` (só comandos `scope: core`, com o gato como contexto) |
| `world.hint`, `world.palette`, `world.line` | SALTO, COMANDOS, linhas de texto | só o próprio modo; teclas imprimíveis são texto |

O `normal.world` é um sub-modo de `normal`, como o `normal.todos` (`core.ParentMode`). As teclas dele sombreiam as de `normal` e o resto cai em `normal`.

Acordes globais (`world`):

| tecla | comando |
|---|---|
| `world+w` | `world.walk` |
| `world+h` `world+l` `world+k` `world+j` | `world.focus_left` / `world.focus_right` / `world.focus_up` / `world.focus_down` |
| `world+shift+j` `world+shift+k` | `world.room_next` / `world.room_prev` |
| `world+space` | `world.summon` |
| `world+f` | `world.hint` |
| `world+p` | `world.palette` |
| `world+enter` | `world.terminal` |
| `world+d` | `world.launcher` |
| `world+q` | `world.close` |
| `world+o` | `world.overview` |
| `world+shift+e` | `world.quit` (confirma) |

Lua não muda nada: é o `nk.keymap.set` que já existe.

```lua
nk.keymap.set("world.walk", "j", "world.turn_left")     -- girar com j/l em vez de q/e
nk.keymap.set("world.walk", "l", "world.turn_right")
nk.keymap.set("normal.world", "s", "world.walk")        -- s entra no ANDAR
nk.keymap.set("world", "world+a", function()             -- acorde próprio: vai à sala "acme"
  nk.ui.scene("world.goto_room", { room = "acme" })
end)
```

O `keymap.changed` faz o `nkw` reler `/v1/keymap` na hora.

## 4. Mundo e layout

### 4.1 Vocabulário

| termo | o quê | código |
|---|---|---|
| sala | um espaço de trabalho; por padrão, um contexto do core | `Room {id, name, kind: Context(String) / User / Library / Reception}` |
| corredor | liga as salas; as portas alternam dos dois lados | `Hall` |
| mesa | uma coluna do niri: largura em preset e uma pilha de 1 a 3 telas visíveis | `Desk {width, stack, cat: Option<SessionId>}` |
| tela | qualquer coisa exibível | `Screen {source: Window / AgentPane / Tail / Preview, size}` |
| tela no ar | tela presa numa pose livre | `Placement::Air(Pose)` |
| estação | pose de câmera mais um alvo (§3.4) | `Station` |
| biblioteca | os arquivos | `Room::Library` (§7) |

### 4.2 Geometria e escala

- Metros, com y para cima. **1 px lógico de janela = 1 mm**: uma janela de 1600×1000 vira uma tela de 1,6×1,0 m, um monitor gordinho de AC.
- **A pose 1:1 é exata.** Com saída de altura física `H`, escala `s` e FOV vertical `θ`:
  - `f = (H/2)/tan(θ/2)` px físicos;
  - um plano paralelo ao plano da imagem a uma distância `d` aparece com `tamanho_m · f / d` px;
  - igualando a `tamanho_lógico · s` px: `d = 0,001 · f / s`.
  - Exemplos: 1080p, s=1, 60° dá `d ≈ 0,935 m`; 1440p, s=1,25 dá `≈ 1,0 m`.
  - Um ajuste submilimétrico em x/y põe o canto superior esquerdo num pixel inteiro.
  - Isso fica em `nkw-core/src/onetoone.rs`, com teste: cantos projetados a menos de 0,01 px do retângulo 2D.
- **A pose de NAV numa mesa** fica a ~2,4 m: a pilha ocupa ~55 % da altura da vista, e dá para ver o gato e meio vizinho de cada lado.
- **As telas ficam com o centro na altura do olho (1,5 m).** Nas pilhas de 2 ou 3, as telas inclinam para o olho e o FOCO se alinha à normal (pitch de até ~20°, sem roll).
- **O gato senta a ~0,6 m da tela, de costas para você**, como no `nkg`. A cabeça fica abaixo do centro da tela e não tampa a leitura.

### 4.3 Planta

- **Recepção**: onde você começa. Tem a gata recepcionista, o quadro de avisos (contagem da inbox, custo do dia, uso de 5 h) e a porta da biblioteca.
- **Uma sala por contexto do core**, na ordem da árvore (`/v1/view/tree`), com tapete e papel de parede na cor do contexto. Gatos **e** as suas janelas daquele projeto convivem na mesma faixa.
- **"Minha sala"** sempre existe, para janelas abertas fora de uma sala de contexto. Salas livres são criadas pela paleta (`world.room_new`).
- **Cada sala é uma faixa de mesas ao longo da parede do fundo** e cresce para a direita conforme as mesas aumentam, como a faixa infinita do niri.

### 4.4 Colocação automática (a "do niri")

- **Janela nova** entra numa mesa nova **logo à direita da estação atual**, com largura 1/2 (configurável por regra). Se a janela foi aberta por uma intenção registrada, ela vai para lá:
  - a mesa de leitura da biblioteca;
  - a mesa do gato que pediu (§6.1);
  - o lugar lembrado (§4.7).
- **Gatos e mesas de usuário** são reconciliados por uma função pura, `reconcile(strip, sessions_em_ordem)`:
  - um gato novo entra depois da mesa de gato anterior na ordem da árvore, sem empurrar as mesas de usuário;
  - um gato que sai leva só a si: se a mesa tinha janelas suas, ela fica como mesa de usuário ("a mesa fica, o gato vai embora").
- **Tamanho.** A largura é um preset (1/3, 1/2, 2/3 ou 1 da largura útil da saída, menos 16 px de margem). A altura é a altura útil dividida pelo número de telas visíveis na pilha. O `configure` manda esse tamanho, que é exatamente o do FOCO.
- **Mais de 3 telas numa mesa** formam um monte atrás. `j`/`k` percorrem a pilha toda e as 3 visíveis acompanham.

### 4.5 Colocação manual

- **No NAV**: `H`/`L`, `,`/`.`, `-`/`=`, `p` (prende no ar), `M` (outra sala).
- **No ANDAR**: `g` para pegar, mais as regras de soltar de §3.5.
- **Telas no ar** entram na ordem de `h`/`l` da sala pela posição ao longo do eixo da sala, e também aceitam FOCO (alinhado à normal delas).

### 4.6 Casos especiais

| caso | regra |
|---|---|
| popup / menu (`xdg_popup`) | só no FOCO, em 2D, com `PopupManager` e grabs de teclado/ponteiro; fechado ao sair do FOCO |
| diálogo (`set_parent`, `xdg_dialog_v1`) | tela menor 5 cm à frente da mãe, centrada; com a mãe em FOCO, o diálogo vira o alvo 2D centrado sobre ela; modal bloqueia a entrada da mãe |
| tela cheia | no FOCO, ocupa a saída inteira e o passe do mundo não roda; fora do FOCO, a janela continua configurada em tela cheia e vira uma "TV" na mesa ou parede |
| maximizar | largura preset 1 |
| vídeo (mpv, navegador) | no FOCO é 2D e só o dano é composto; na estação atual do NAV, o mundo re-renderiza até 30 Hz; em outros lugares vale o LOD (§5.3) |
| decorações | `xdg-decoration` sempre pede server-side; a "decoração" é o aro do monitor no 3D e nada no FOCO (o título vai no crachá); clientes só com CSD têm a sombra cortada pela window geometry |
| `xdg-activation` | com token recente nosso (launcher ou biblioteca), seleciona a estação; sem token, um gato mensageiro traz um bilhete "quer atenção", que vira item da inbox |
| janela aberta por um agente | o pid do cliente (credenciais do Smithay) sobe a cadeia de `ppid` em `/proc` até o `Session.pid` de um gato; a janela vai para a mesa desse gato (fase 6) |
| portais e apps de instância única (aninhado) | diálogos de portal e apps já abertos no niri (navegador, IDE) podem abrir **fora** do mundo; é limitação documentada do modo aninhado, com variáveis de ambiente por app quando existirem, e se resolve na sessão própria |

### 4.7 Persistência

- **Preferências**: `~/.config/nekuroddo/world.toml` (tecla do mundo, escape, `[walk]`, `[camera]`, `[render]`, `[library]`, `[launch]`, `[[rules]]`). As teclas em si ficam no keymap e no Lua.
- **Estado**: `~/.local/state/nekuroddo/world.json`, com `{"version": 1, …}`. Guarda:
  - salas (ordem, nome, tipo);
  - mesas (largura, ordem, vínculo com `session_id`);
  - telas (fonte, slot ou pose no ar, tamanho);
  - a estação atual;
  - os lugares lembrados.
- **Escrita**: atômica (arquivo temporário e `rename`), 2 s depois da última mudança e na saída. Um arquivo corrompido vira `.bak`, o mundo começa limpo e um toast avisa.
- **Identidade de janela entre reinícios.** O Wayland não tem ID estável, então usamos **memória de lugar** por `app_id`: quando uma janela daquele app abre, ela vai para o slot vazio mais recente que um app igual ocupou. Regras explícitas também valem:

  ```toml
  [[rules]]
  app_id = "firefox"
  room = "pessoal"
  desk = "last"
  width = "2/3"
  ```

- **As mesas dos gatos usam o `session_id`**, que é estável.

## 5. Renderização

### 5.1 O quadro

```
eventos: clientes · teclado · core (SSE) · timers · biblioteca
   │
   ▼
nkw-core: atualiza o mundo → Scheduler decide: passe do mundo? composição? próximo despertar?
   │
   ├─ mundo sujo ─► nkw-gl: passe do mundo (offscreen, MSAA 4×) ─► textura do mundo
   │                   ▲ texturas das janelas: tex_id do Smithay (cópia zero), ou composta
   │                     (subsuperfícies, mipmaps) só quando precisa
   ▼
Smithay OutputDamageTracker:
   [cursor (sessão própria)] [sobreposições 2D] [janela em FOCO + popups, 1:1] [textura do mundo]
   │
   ▼
host (winit no niri) · ou DRM (fase 7)
```

### 5.2 Estilo "aconchego" e preset "64"

**Geometria**:
- malhas procedurais em código (como o `Kit` do `nkg`): caixas e cilindros arredondados, poucas faces, proporções de brinquedo;
- vértice = posição, normal (i8×4) e cor (u8×4);
- instancing por malha (matriz + tom + parâmetros por instância);
- glTF opcional depois (crate `gltf` sem features extras).

**Luz**:
- half-lambert `l = dot(n, L)·0,5 + 0,5`, quantizado por uma rampa de 3 faixas;
- ambiente hemisférico `mix(chão, céu, n.y·0,5 + 0,5)`;
- um "sol" direcional;
- rim sutil.

**Hora do dia** pelo relógio local:
- céu, névoa e sol mudam de cor em 8 faixas;
- re-render só quando a faixa muda (timer de 10 min);
- tema escuro no core = noite no mundo.

**Sombras-bolha**: quads instanciados com gradiente radial no shader, sem textura e sem escrita de profundidade.

**Névoa** linear para a cor do céu. Também serve de distância de corte: 40 m.

**Texturas**: só as das janelas, os atlas de texto e padrões de 32 a 64 px para chão e papel de parede (escolhíveis por sala, como no AC).

**Paleta**: 16 a 24 cores pastel nomeadas (creme, madeira, folha, céu, pêssego, lilás, tinta marrom) em `nkw-core/src/palette.rs`. Os tokens do tema do core (`/v1/theme`) tingem só os acentos: tapetes e crachás por contexto, coleiras por grupo.

**Preset `64`**:
- alvo interno a ~320×240 a 480×270, conforme a proporção;
- filtro 3-point no shader para as texturas do mundo;
- dither Bayer 4×4 em 5 bits por canal;
- upscale nearest para a textura do mundo em resolução nativa.

O upscale é nosso (um passe de tela cheia, ~0,2 ms), para não mexer no filtro global do Smithay.

**Escala de render**: `render.scale = "auto"` usa 1,0 até 2560 px de largura e 0,75 acima.

### 5.3 Telas no mundo e LOD

- **Amostragem direta** do `tex_id` quando a janela é uma superfície só e a tela não aparece muito reduzida.
- **Textura composta** por janela, num FBO próprio desenhado pelo caminho 2D do Smithay, quando a janela:
  - tem subsuperfícies (vídeo no navegador);
  - precisa de mipmaps (reduzida mais de 1,5×);
  - é dmabuf `GL_TEXTURE_EXTERNAL_OES`, que não aceita mipmap. Os shaders têm variante `samplerExternalOES`, como no Smithay.
- **Filtro**: trilinear e anisotrópico 4× quando houver, para telas em ângulo no ANDAR.
- **Throttling** de callbacks de quadro com o `throttle` do `send_frame` do Smithay. Isso poupa o **cliente** também: telas longe desenham menos.

| situação | atualização da textura | callbacks de quadro ao cliente |
|---|---|---|
| em FOCO | todo commit (2D direto) | a cada quadro apresentado |
| tela da estação atual (NAV) | todo commit; o mundo re-renderiza até 30 Hz | a cada quadro |
| visível, a menos de 6 m | ≤ 15 Hz | ≤ 15 Hz |
| visível, a 6 m ou mais | ≤ 4 Hz | ≤ 4 Hz |
| fora da vista ou outra sala | congelada no último quadro | 1 Hz (clientes que dependem do callback não travam) |
| terminal de gato (interno) | por dano de linhas do painel, ≤ 30 Hz fora do FOCO; o tail ≤ 10 Hz de revelação | — |

### 5.4 Texto e legibilidade

1. **Ler e digitar só em 1:1** (FOCO). No 3D, o texto das telas é ambiente.
2. **Todo texto do mundo é sobreposição 2D em resolução nativa e pixel inteiro**: crachás de nome, título da tela selecionada, HUD, caixa de diálogo, rótulos de SALTO. São rasterizados com fontdue (`nk-cats::raster`, que ganha retângulo arredondado com AA) só quando mudam.
3. **Fontes OFL embutidas**: JetBrains Mono (já está no repo) para terminais e uma fonte arredondada para a interface, como M PLUS Rounded 1c ou Nunito, num subconjunto latino com acentos de pt-BR (~150 KB).
4. **Contraste mínimo de 4,5:1** nas sobreposições, com opção de alto contraste. O estado de um gato aparece também por forma (`!`, `zZ`, envelope), não só por cor.

### 5.5 A transição para o 1:1

1. A câmera desliza até a pose 1:1 (§4.2). No caminho, a janela é o quad 3D com a textura dela.
2. No quadro em que a câmera chega, a projeção do quad **é** o retângulo 2D. Os dois são trocados no mesmo quadro, sem salto visível. É o mesmo truque do zoom do `nkg`.
3. Se o cliente ignorar o tamanho do `configure`, a janela fica centrada e é cortada.
4. Ao sair do FOCO, faz-se o inverso: o 2D vira quad e a câmera recua.

### 5.6 Dano, agendamento e apresentação

O `nkw-core/src/schedule.rs` é puro. Ele recebe "vontades" e devolve `{passe_do_mundo, compor, próximo_despertar}`:

| vontade | taxa |
|---|---|
| `Continuous` (câmera deslizando, ANDAR com tecla segurada) | todo quadro do host |
| `Rate` (gatos ativos: 15 fps; carteiros à vista: 30 fps; texto letra a letra: 30 Hz) | a taxa indicada, alinhada à grade de vblank |
| `Once` (HUD, hover, commit de tela, tema) | uma vez |
| `At` (fim de toast, hora do dia) | no instante pedido |

- Tudo o que vence no mesmo quadro é desenhado junto.
- **Sem vontade nenhuma, o `calloop` não arma timer**: só acorda por fd.
- **O passe do mundo e a composição são separados.** Commit do cliente em FOCO e mudança do HUD só recompõem.
- **Aninhado**: os quadros são marcados pelos callbacks do host (`RedrawRequested` do winit). O `wp_presentation` para os clientes usa esse instante como estimativa.
- **Sessão própria**: page-flip do DRM e presentation-time exato.
- **Threads de I/O acordam o laço por `calloop::ping`**: core (SSE), painéis e varredura da biblioteca.

### 5.7 Culling

- Por **sala**: desenha a sala atual, o corredor e as salas visíveis por portas abertas.
- Frustum na CPU por instância e corte na distância da névoa.
- No mapa, tudo vira caixas simples.

### 5.8 Orçamento por quadro (S3/S4, 1080p)

| etapa | CPU | GPU |
|---|---|---|
| atualizar o mundo (poses, câmera, walkers) | ≤ 0,3 ms | — |
| culling e buffers de instância | ≤ 0,2 ms | — |
| submeter o passe do mundo (≤ 80 draws) | ≤ 0,5 ms | ≤ 1,5 ms (MSAA 4×) |
| rasterizar sobreposições (só as que mudaram) | ≤ 0,3 ms | — |
| composição do Smithay | ≤ 0,3 ms | ≤ 0,3 ms |
| **total** | **≤ 1,6 ms** | **≤ 2 ms** |

## 6. Gatos (agentes)

Todo ser vivo do mundo é um gato:

| gato | quem é | de onde vem |
|---|---|---|
| aldeão | uma sessão do Claude Code | snapshot e `/v1/view/tree` |
| filhote | um subagent | `session.subs[]` |
| carteiro | mensagem entre sessões e ✉ de grupo | `peer.message`, `mail.cue` |
| recepcionista | toasts, confirmações vindas de fora da conversa, mensagens do sistema | `toast`, `notice`, `confirm.requested` |
| mensageiro | janela pedindo atenção sem token | `xdg-activation` |
| bibliotecária | resultados de busca na biblioteca | §7 |

### 6.1 Sessão → aldeão

- Uma sessão da aba sessões é **um gato na sala do contexto dela**, numa mesa na ordem da árvore (como as colunas do `nkg`).
- A **aparência é determinística pelo id** (hash): cor do pelo de uma paleta pastel, padrão (malhado, tricolor, smoking, liso), formato da orelha. Cada agente fica reconhecível.
- **Crachá de nome** com o `label` da sessão.
- **Coleira** pela cor do primeiro grupo (como no `nkg`).
- **Custo** num pote de moedinhas na mesa; o custo do dia vai no quadro de avisos.
- Janelas abertas pelo processo do agente vão para a mesa dele (§4.6).

### 6.2 Estado → pose

Reusa `nk-cats::model::pose_for` e `catpose` (parâmetros de junta).

| estado | pose | marcador |
|---|---|---|
| `busy` | patas no teclado, cabeça balançando | — |
| `blocked` / atenção 1 | de pé na cadeira, orelhas em pé | `!` |
| `done` / devolvida (2) | cabeça na mesa | `zZ` |
| ociosa | calma, rabo balançando, olha em volta em "rajadas" | — |
| erro / encerrada | eriça o pelo uma vez, depois senta quieta | — |
| na CONVERSA | vira na cadeira para você e mexe a boca enquanto o texto aparece | — |

A animação ambiente roda a 15 fps (`catpose::AMBIENT_FPS` ajustado). Gatos calmos animam só nas rajadas, e no movimento reduzido ficam em pose fixa.

### 6.3 O monitor do gato

- **Sem painel**: o tail (`GET /v1/sessions/{id}/tail`, atualizado por `session.tail`), colorido por tipo, com as linhas novas entrando uma a uma (`monitor::Feed`).
- **Com painel**: o terminal ao vivo. O FOCO no monitor do gato abre ou anexa o painel, redimensiona (`fit_cells` pelo retângulo de foco e pela célula da fonte) e manda as teclas pelo `encode_key`.
- Só as linhas sujas são re-rasterizadas (o dano do alacritty_terminal).

### 6.4 CONVERSA: o que o gato diz e o que você pode fazer

| estado | fala de abertura | escolhas |
|---|---|---|
| bloqueado com pergunta (`questions[]`) | "{pergunta}" | as opções (1–9) · "Outra resposta…" · "Depois" |
| bloqueado sem pergunta (permissão) | "Preciso de você: {waiting_for / needs}" | "Ver o terminal" · "Responder…" · "Depois" |
| ocupado | "Estou em {last_tool}…" e um trecho do `last_text` | "Mandar mensagem…" · "Parar" · "Ver o terminal" |
| terminou / devolvido | "Terminei! {result ou last_text}"; os `links` viram cartas que o gato te entrega | "Abrir {link}" · "Mandar mais…" · "Arquivar" |
| ocioso | "Oi! Pode mandar." | "Mandar mensagem…" · "Ver o terminal" |
| erro / encerrado | "Ops… {detail}" | "Ver o terminal" · "Remover" |
| filhote | "{tipo}: {descrição}" e o `last_text` dele | "Voltar" (só leitura) |
| carteiro | "Carta de {from} para {to}: {texto}" | "Ir até {to}" · "Ok" |

| ação | chamada ao core |
|---|---|
| escolher opção N / responder | `POST /v1/panes {session_id, cols, rows, reply}` (`open_pane_reply`): o servidor cola o texto com bracketed paste e manda enter, como a TUI. Uma opção numérica é resolvida por `core.ResolveOption` no servidor (§6.9) |
| parar | `POST /v1/commands/session.stop` devolve `pending`; a caixa pergunta "Parar {label}? Sim / Não"; a resposta vai para `POST /v1/confirm/{token}` |
| renomear / mover / arquivar / remover | `session.rename` (`name`) · `session.move` (`context`; o gato anda até a outra sala) · `session.archive` · `session.remove` (confirma) |
| ver o terminal | FOCO no monitor do gato |
| abrir link | `xdg-open` com o `WAYLAND_DISPLAY` do mundo; a janela nova vai para a mesa do gato |
| gato novo | `session.new`: a recepcionista pergunta `dir` e `prompt` na linha da conversa |

Se o gato some durante a conversa (sessão removida), a caixa diz "foi embora" e fecha.

### 6.5 Filhotes (subagents)

- Até 4 filhotes por mesa: na mesa, em volta do monitor, ou no chão.
- Os ocupados digitam em laptops minúsculos. Os que terminaram se enrolam e saem depois de 2 min.
- Acima de 4, aparece uma plaquinha "+N".
- No SALTO, cada filhote tem rótulo e abre a fala dele.

### 6.6 A inbox: os gatos vêm até você

- Quando uma sessão precisa de você (bloqueada, devolvida, mensagem), o gato **levanta e anda até onde você está**.
- **Caminho**: A* na grade de 0,25 m, com as portas como nós, a 1,4 m/s, atualizado a 30 fps.
- **Onde para**: a 1,3 m à sua frente, deslocado 0,6 m para o lado para não tapar a tela, em semicírculo. No máximo 3 perto de você; o resto vira contador "+N" no quadro de avisos e no HUD.
- **A fila** é a do `nk-cats::inbox` (ordem, stagger, dispensa), adaptada para "vir até a câmera" em vez de "corredor da frente".
- **No FOCO não há caminhada**: aparece só um selo 2D no canto, e quando você sai, os gatos "já estão ali".
- **`shift+space` / `world+space`** (`inbox.summon`, evento `inbox.summoned`) abrem a CONVERSA com o mais urgente.

### 6.7 Confirmações e toasts

- **`confirm.requested`**:
  - dentro da CONVERSA, vira escolha Sim/Não na própria caixa;
  - fora dela, a recepcionista vem até você com um "?" (ou um selo, no FOCO);
  - `confirm.resolved` fecha em qualquer cliente.
- **`toast`** vira um balão curto da recepcionista no canto inferior esquerdo, com 3 s para info e 6 s para erro.
- **`notice.meow`** vira um miado, se o som estiver ligado.

### 6.8 Som (opcional, minúsculo)

- Desligado por padrão, atrás da feature `sound` (cpal).
- Bipes sintetizados no estilo "animalese" enquanto o texto aparece, miados curtos para `notice` e passos suaves no ANDAR.
- Tudo gerado em código (sem arquivos de áudio), com volume configurável.

### 6.9 Mudanças no core (Go), todas aditivas e por TDD

1. **`internal/app/world.go`: `RegisterWorldCommands`.** Os comandos `world.*` `scope: client`, nos modos de §3.12, chamado de `RegisterBuiltins`. Testes:
   - `Keymap.Collisions()` vazio para os modos do mundo;
   - toda tecla do modo `world` contém `world+`;
   - títulos em pt-BR.

   A TUI ignora `normal.world` e `world.*`: um teste garante que a ajuda e a barra de status nunca os mostram. Atualizar `docs/comandos.md` e `docs/teclas.md`.
2. **`POST /v1/panes`** passa o `reply` por `core.ResolveOption` com as perguntas da sessão, se ainda não passa. É a mesma regra do `session.reply`, então "2" responde igual em todo cliente. Teste no `internal/rpc`.
3. **Lua `nk.ui.scene(command, args)`** publica o evento `ui.scene {command, args}`. O `nkw` executa o comando de cliente; sem mundo conectado, nada acontece. `nk.ui.has("scene")` passa a ser verdadeiro no `nk serve`. Documentar em `docs/lua.md`.
4. **Fase 6, opcional**: eventos vindos do mundo para o Lua (`world.room_entered`…) por uma rota aditiva com nome reservado `client.*` e limite de taxa. Só se surgir uso real.

Nada muda de versão: o protocolo continua v1. A cobertura do Go segue ≥ 80 % no total e o código novo fica ≥ 90 %.

### 6.10 Mudanças no `nk-client` (leveza)

- **Painel**: hoje faz polling de 8 ms no WebSocket, o que dá **125 wakeups/s por painel aberto**. Passa a usar uma espera bloqueante com `poll(2)` no fd do socket mais um `eventfd` sinalizado pelo `PaneConn::send`: zero wakeups parado, latência zero na tecla.
- **Keeper do core**: hoje faz polling de 50 ms (20 wakeups/s). Passa a usar `wait` bloqueante numa thread e `pidfd` com timeout no encerramento.
- **Teste**: menos de 2 trocas de contexto por segundo nas threads ociosas. O `nkg` ganha junto.

## 7. Biblioteca (arquivos)

### 7.1 Mapeamento

- A biblioteca é uma sala ao lado da recepção.
- Cada **raiz** (`library.roots`) é uma parede de estantes.
- Um **diretório** é uma estante: as prateleiras são as entradas em ordem (pastas primeiro, depois arquivos por nome). Uma placa em cima mostra o caminho relativo à raiz.
- Um **arquivo** é um livro:
  - a cor da lombada vem do tipo (código, texto, imagem, mídia, arquivo compactado, outros);
  - a altura vem do log do tamanho;
  - o nome vai na lombada, como sobreposição 2D ao se aproximar.
- **Entrar numa pasta** (`enter`) desliza o conteúdo da estante para o lado e põe a subpasta no lugar; a placa vira breadcrumb. `backspace`/`h` no começo da estante sobe um nível.

### 7.2 Varredura preguiçosa e limites

- **Só lê `readdir` + `stat`**: nunca conteúdo, fora o preview pedido.
- **Uma pasta é lida ao chegar na estante dela**, numa thread de varredura com resultado via `ping`.
- **No máximo 400 entradas mostradas por estante.** O resto vira uma caixa "+N", aberta pela busca.
- **Pastas ignoradas por padrão**: `.git`, `node_modules`, `target`, `.cache`. Arquivos ocultos ficam escondidos (`show_hidden = false`).
- **Não segue symlink para fora das raízes**, e para em fronteira de montagem (mudança de `st_dev`, como NFS, sshfs, fuse).
- **O índice só existe na memória.** Nenhum índice persistente nem daemon.

### 7.3 Navegação e busca

- **Na estante**: `h`/`l` andam pelos livros, `j`/`k` pelas prateleiras, `enter` abre ou entra.
- **`/` busca por fuzzy** na subárvore atual, com uma thread que tem orçamento: até 100k entradas, profundidade 12 e 2 s.
- **Os resultados** chegam na "mesa de busca": a bibliotecária os traz (com movimento reduzido, aparecem direto).

### 7.4 Abrir

| tecla | o quê |
|---|---|
| `enter` | `xdg-open` com `WAYLAND_DISPLAY` e `DISPLAY` do mundo e um `XDG_ACTIVATION_TOKEN` registrado como **intenção de colocação**: a janela vai para a mesa de leitura (ou para a sala de onde você veio, com `world.toml`) |
| `e` | `$EDITOR` num terminal (`launch.terminal -e $EDITOR <arquivo>`) numa tela nova |
| `p` | preview no "atril": texto UTF-8, até 64 KB, desenhado com o rasterizador; binário não tem preview; imagem abre pelo `xdg-open` |
| `y` | copia o caminho (seleção do seat e, aninhado, a área de transferência do host) |
| `g` (fase 4b) | **entrega o livro a um gato**: escolhe o gato pelo SALTO; o caminho (`@<caminho>`) vai como resposta (`POST /v1/panes … reply`), depois de uma confirmação na caixa de diálogo |

Quando o app não usa o token de ativação, vale a regra de reserva: o próximo toplevel em até 5 s, com pid descendente do processo lançado, ocupa o lugar da intenção.

### 7.5 Segurança e privacidade

- **Lista de exclusão padrão**, nunca mostrada nem buscada: `~/.ssh`, `~/.gnupg`, `~/.password-store`, os chaveiros e `**/.env*`. É configurável, mas a lista padrão só pode **crescer** via config.
- **Nada da biblioteca sai do processo `nkw`.** Não vai para o core nem para os agentes, exceto na entrega explícita de um livro, com confirmação.
- **Screenshots e documentação** usam só o mundo demo, com uma biblioteca de fixture (`nkw --demo` não lê a home). Nunca há caminhos pessoais em docs, PRs ou testes.
- **O estado** (`world.json`, que guarda caminhos de intenções e regras) fica em `~/.local/state/nekuroddo` com permissão 0700.

## 8. Extensibilidade

- **Um runtime Lua só, o do core.** Comandos (`nk.command`), teclas (`nk.keymap.set` nos modos do mundo) e eventos (`nk.on`) já servem a TUI, o `nkg` e o `nkw`.
- **Ações no mundo a partir do Lua**: `nk.ui.scene("world.goto_room", {room = "acme"})`, `nk.ui.scene("world.terminal")`, `nk.ui.scene("world.open", {path = "…"})`.
- **Por que não pôr scripts no compositor.**
  - Um segundo runtime (`mlua`/C) no processo que segura todas as janelas aumenta o risco de crash e o binário.
  - Seriam duas APIs e dois configs.
  - O vocabulário do mundo é pequeno e estável: comandos `world.*` com argumentos.
- **Revisitar** só se surgir um caso que precise de lógica por quadro. Hoje não há nenhum.

## 9. Testes

**TDD em tudo.** O teste vermelho vem antes do código.

| camada | como | meta |
|---|---|---|
| `nk-cats` | os testes atuais do `nkg` mudam junto (35 testes de unidade) e ganham os casos novos (rosto e pose de conversa, retângulo arredondado) | ≥ 90 % de linhas (`cargo llvm-cov`) |
| `nkw-core` | unidade e tabela: máquina de modos (todas as transições de §3.3), Router com modos e `world+`, pose 1:1 (cantos < 0,01 px), deslizes e cortes, colisão (corpus: deslizar na parede, passar por porta, sem tunelamento a 3× a velocidade), regras de saída do ANDAR, colocação e `reconcile`, LOD e agendador (sem timer ocioso; coalescência), rótulos de SALTO (sem prefixo; estáveis), diálogo (letra a letra pelo tempo), linha de texto, biblioteca com `FsPort` falso (limites, exclusão, fronteira de montagem), persistência (ida e volta, migração, arquivo corrompido) | ≥ 90 % de linhas; gate em `make world-cover` |
| `nkw-gl` | unidade nas partes puras (malhas, empacotamento de instâncias); **golden** a 320×180 num contexto EGL surfaceless (Mesa llvmpipe, `khronos-egl` como dev-dependency), com tolerância (delta por pixel e % de pixels diferentes); `UPDATE_GOLDEN=1` regenera | todos os passes cobertos por golden |
| `nkw` (integração) | **backend headless** (`EGLDevice`: render node ou llvmpipe) num `XDG_RUNTIME_DIR` temporário; clientes `nkw-testclient` (toplevel de cor ou xadrez; log JSON de configure, enter, leave, tecla, frame, popup); entrada pelo `--script`; asserções no log do cliente, no dump do estado do mundo (`world.dump()` em JSON) e em screenshots | casos: só o FOCO recebe teclas; `ctrl+]` solta teclas seguradas; popup fecha ao sair; diálogo na mesa da mãe; tamanhos de `configure` por preset; callbacks por LOD (fora da vista ≤ 1,2/s em 3 s); seleção copiada entre dois clientes; tela cheia sem passe do mundo |
| e2e real | se `NKW_E2E=1` e os apps existirem: `foot -e sh -c 'read x; echo $x > arquivo'` digitado pelo `--script` (pt-BR com acento); app GTK4; mpv com dmabuf | rodar antes de fechar cada fase |
| screenshot | `nkw --demo --headless --script "…" --shot x.png` (também `--shot-talk`, `--shot-inbox`, `--shot-library`); o FOCO pixel-exato é conferido (recorte == buffer do xadrez do testclient, 0 pixels diferentes) | toda mudança visual vem com shot no PR |
| desempenho | `make world-perf` (S1–S5, §1.4) com `perf-budget.toml` | rodar no fim de cada fase e antes de PRs que mexem em laço ou render |
| core Go | testes de `world.go`, de `ResolveOption` no `/v1/panes`, do `nk.ui.scene`; `archtest` intacto | gate atual (≥ 80 % no total; novo ≥ 90 %) |

O `make office-test` continua rodando o workspace inteiro. Os alvos novos são:

- `make world-test`: `cargo test -p nk-cats -p nkw-core -p nkw-gl -p nkw`, sem compilar o Bevy;
- `make world-lint`;
- `make world-cover`;
- `make run-world`;
- `make world-shot`;
- `make world-perf`.

## 10. Roteiro por fases

O esforço está em semanas de 1 dev trabalhando com agents. Cada fase:

- é uma branch `mundo/fase-N-<slug>` com PRs menores dentro dela;
- termina com os números em `docs/log.md`;
- é aceita pelo usuário antes da próxima.

### Fase 0 — Spike, com decisão de seguir ou parar (2 semanas)

**Entregas**:
- crates `nkw-core` (NAV e FOCO mínimos, x das mesas, pose 1:1, deslize, agendador), `nkw-gl` (glow no contexto do Smithay, sala-caixa, 3 mesas, quads de janela), `nkw` e `nkw-testclient`;
- protocolos: winit, `calloop`, socket `nkw-0`, xdg-shell, wl_shm, linux-dmabuf, seat (teclado com XKB do `world.toml`, ponteiro), output, data-device mínimo;
- NAV com `h`/`l` e deslize; `enter` leva ao FOCO 1:1; `ctrl+]` volta; mouse no FOCO;
- laço por dano, callbacks por LOD, cursor do host;
- backend headless com `--shot` e `--script`;
- `world-perf.sh` com S1, S2 e S4, e `NKW_STATS`.

**Segue se todos os critérios passarem**:
1. foot, um app GTK4 e um app com dmabuf (Chromium com ozone ou mpv) aparecem nos quads e aceitam digitação no FOCO com o layout configurado, inclusive acentos por tecla morta ou compose.
2. O FOCO é pixel-exato: recorte da screenshot == buffer do testclient, 0 pixels diferentes.
3. S1: ≤ 0,5 % de CPU e ≤ 5 wakeups/s em 60 s.
4. S4: CPU p95 ≤ 2 ms por quadro com 4 telas, sem quadro perdido a 60 Hz.
5. S2: ≤ 2 % de CPU a 15 teclas/s; tecla no cliente em menos de 1 ms.
6. Os elementos 2D do Smithay desenham certos depois do passe 3D (golden) e não há `glGetError` em debug.
7. Binário stripped ≤ 25 MB; build release a frio ≤ 6 min.

**Se não passar**:
- se o problema for glow no contexto do Smithay: plano B de D3 (até +1 semana);
- se o problema for o próprio Smithay aninhado: +1 semana no máximo;
- se o desempenho ficar mais de 2× pior que o orçamento: parar e reavaliar. A alternativa seria o mundo como camada de navegação sobre o niri (as janelas continuam no niri e o mundo só as aponta e foca).

### Fase 1 — Gerenciador de janelas aninhado utilizável (4 semanas)

**Entregas**:
- salas (recepção, "minha sala", salas livres), corredor, mesas com pilhas, presets de largura, telas no ar via `p`, `H`/`L`, `,`/`.`, `-`/`=`, `M`, `Q`;
- popups, diálogos (`set_parent`, xdg-dialog), tela cheia como "TV", maximizar;
- xdg-decoration server-side com o aro do monitor;
- área de transferência: interna e **ponte com o host** (ext/wlr-data-control no lado do niri, numa thread; nunca registra o conteúdo);
- viewporter, fractional-scale, presentation-time (estimado), cursor-shape, xdg-activation;
- launcher (paleta de apps, `world+enter` para terminal);
- persistência (`world.json`, regras, memória de lugar);
- SALTO, COMANDOS, mapa;
- comandos `world.*` no core (§6.9 item 1) e Router do mundo com os modos;
- estilo base: cor por vértice, half-lambert e névoa.

**Aceite**:
- Lista do dia a dia passa: foot, Firefox, Chromium, um app GTK4, um app Qt6, mpv (60 fps no FOCO, ≤ 15 Hz fora da estação), menu de clique direito, um diálogo de arquivo não-portal, F11, copiar e colar nos dois sentidos com apps do niri, texto nítido na escala 1,25.
- Reiniciar o `nkw` devolve salas, mesas e regras.
- `nk.keymap.set("normal.world", "n", "world.right")` funciona.
- S1 ≤ 0,2 % e ≤ 2 wakeups/s; S2 ≤ 1 %; S4 p95 ≤ 2 ms; RSS ≤ 110 MB com 8 janelas.
- `nkw-core` ≥ 90 %.

### Fase 2 — Gatos (3 semanas)

**Entregas**:
- extração do `nk-cats`, com o `nkg` igual (testes verdes, screenshot idêntica);
- correções do `nk-client` (§6.10);
- `core_link`;
- salas por contexto e `reconcile`;
- aldeões procedurais por id e poses;
- monitores (tail e terminal) e FOCO no terminal do gato;
- CONVERSA completa (§6.4) com linha de texto;
- filhotes, inbox vindo até você (A*), carteiros, recepcionista (toasts e confirmações), HUD e quadro de avisos;
- mudanças do core 2 e 3 (§6.9).

**Aceite**:
- `nkw --demo --script talk-gateway` responde a pergunta do `api-gateway` com a opção 1 na CONVERSA, e o tail mostra `> sim, aplicar agora`.
- Parar uma sessão pede confirmação na caixa e funciona.
- S3 ≤ 3 % de CPU; com `--reduced-motion`, ≤ 0,5 %.
- Shots `--shot-talk` e `--shot-inbox` aprovados.
- Wakeups do `nk-client` ocioso ≤ 1/s.

### Fase 3 — ANDAR (2 semanas)

**Entregas**:
- física e colisão (§3.5);
- olhar pelo teclado e pelo mouse (trava de ponteiro do winit);
- mira e `interact`;
- pegar, soltar, encaixar e prender no ar;
- regras de saída;
- opções de conforto e movimento reduzido.

**Aceite**:
- Corpus de colisão e tabela de saída verdes.
- S5: 60 fps, CPU p95 ≤ 2 ms.
- **10 minutos andando com os padrões sem desconforto**, assinado pelo usuário.

### Fase 4 — Biblioteca (2 semanas)

**Entregas**: §7 inteiro, com "entregar o livro ao gato" ao final.

**Aceite** (home com 200k arquivos):
- primeira estante em ≤ 100 ms;
- pasta de 1k entradas lida em ≤ 20 ms;
- busca em 50k entradas em ≤ 1 s dentro do orçamento;
- +30 MB de RSS no máximo com a biblioteca aberta;
- caminhos excluídos nunca aparecem (testes).

### Fase 5 — Aconchego, o passe de arte (2 a 3 semanas)

**Entregas**:
- malhas arredondadas e proporções de brinquedo;
- paleta, rampa toon, sombras-bolha;
- padrões de chão e parede por sala;
- hora do dia;
- caixa de diálogo do AC caprichada (letra a letra, plaquinha do nome, bipes opcionais);
- crachás e fonte arredondada;
- patinhas ao carregar uma tela (opcional);
- preset `64`;
- alto contraste e escala de fonte.

**Aceite**:
- goldens aprovados pelo usuário;
- GPU ≤ 2 ms a 1080p;
- binário ainda ≤ 15 MB;
- tempo de quadro sem regressão acima de 10 %.

### Fase 6 — Endurecimento e extensibilidade (2 a 3 semanas)

**Entregas**:
- `xwayland-satellite` sob demanda;
- janelas de agentes na mesa do gato (`Session.pid`);
- `nk.ui.scene` documentado com exemplos;
- backend de host SCTK, se bater algum critério de D4;
- avaliação de `text-input-v3` e IME aninhado, como limitação documentada se não valer o custo;
- pointer-constraints e relative-pointer para clientes no FOCO;
- `docs/mundo.md` (guia), `make install-world` e o atalho no lançador ("nekuroddo — mundo", que abre aninhado).

**Aceite**:
- xterm e um app X11 funcionam;
- um `xdg-open` de um agente no demo cai na mesa dele (teste);
- todos os orçamentos de §1.3 verdes;
- docs revisados.

**Total até a fase 6: de 17 a 20 semanas.**

### Fase 7 — Sessão própria (opcional, 6 a 10 semanas, só depois do aceite da 6)

**Entregas**:
- backends udev/DRM/KMS (atômico, planos, cursor por hardware, scanout direto da janela em tela cheia no FOCO), libinput e libseat;
- entrada `.desktop` de sessão;
- multimonitor: a primeira saída tem a câmera principal; as outras mostram "vistas fixas" configuráveis, como o mapa da sala dos gatos ou uma estação;
- layer-shell (barra, notificações, launcher);
- `ext-session-lock` (swaylock) e `ext-idle-notify` (swayidle);
- DPMS, gamma-control;
- ext-image-copy-capture ou wlr-screencopy para o `xdg-desktop-portal-wlr` (screencast e screenshot);
- `keyboard-shortcuts-inhibit` (máquinas virtuais);
- notificações D-Bus próprias, viradas em carteiros;
- troca de VT, suspend/resume.

**Riscos próprios**:
- perda do dispositivo DRM;
- GPU híbrida;
- comportamento por driver (NVIDIA);
- portais (o file chooser precisa de um backend);
- **crash derruba todas as janelas da sessão**;
- reconexão de clientes é rara no ecossistema.

**Mitigações**:
- o hook de panic grava o estado;
- serviço de usuário com reinício que cai no greeter (nunca tela preta);
- a sessão niri continua sempre disponível e é o padrão;
- o modo aninhado continua suportado.

**Aceite**:
- entra pelo greeter, 2 monitores, bloqueio, screencast pelo portal, suspend e resume;
- um crash provocado volta ao greeter;
- 1 semana de uso diário sem perda de trabalho.

## 11. Riscos e mitigação

| risco | mitigação |
|---|---|
| texto borrado no 3D | regra do 1:1 no FOCO; texto do mundo como sobreposição nativa; mipmaps e anisotrópico; ninguém lê no 3D (§5.4) |
| compatibilidade de clientes (CSD, GTK sem SSD, Electron, Java) | comportamento do anvil e do niri como referência; matriz de apps no aceite da fase 1; window geometry; xwayland-satellite para X11 |
| popups e menus | só no FOCO, com o `PopupManager` e grabs do Smithay; testes dedicados |
| foco e teclado (teclas presas, foco roubado) | seat único; `leave` ao sair do FOCO; teclas pressionadas só contam depois de soltas ao entrar num modo; ativação sem token não foca; testes de integração |
| keymap aninhado diferente do host | XKB no `world.toml` (copiar o `input` do niri); backend SCTK na fase 6 espelha o keymap |
| portais e apps de instância única abrindo fora do mundo | limitação documentada do aninhado; preferir apps lançados dentro do mundo; resolvido na sessão própria |
| enjoo em primeira pessoa | §3.11; padrões conservadores; giro em passos; vinheta; aceite assinado na fase 3 |
| escopo (é um WM inteiro) | aninhado deixa o difícil com o niri; não-objetivos explícitos; aceite por fase; a fase 7 só depois da 6 |
| drivers e GPU | só GLES 3.0 e import de dmabuf como requisitos; MSAA, anisotropia e timer query opcionais; golden no llvmpipe; testar Mesa iris primeiro |
| mudanças na API do Smithay | versão fixa no `Cargo.lock`; Smithay só no crate `nkw`; atualização em PR próprio |
| licenças | niri GPL só como referência; anvil e Smithay MIT; fontes OFL com o `OFL.txt` |
| crash do `nkw` | aninhado limita o estrago às janelas do mundo; estado salvo com debounce; hook de panic; deixar apps críticos fora até a fase 6 |
| acessibilidade | teclado total, movimento reduzido, alto contraste, escala de fonte, estado por forma e não só por cor; leitor de tela do mundo fora do escopo (documentado) |
| regressão de desempenho | `make world-perf` com orçamento e `NKW_STATS` sempre compilado; números por fase no `docs/log.md` |
| privacidade e repo público | lista de exclusão; biblioteca e shots só do demo; nada de caminhos pessoais; screencopy e data-control nunca expostos a clientes no aninhado; socket do mundo em `XDG_RUNTIME_DIR` (0700) |

### 11.1 Perguntas abertas

1. **Tecla do mundo aninhado**: `ctrl+alt` (proposto) serve? Ou você prefere dedicar uma tecla, por exemplo remapear o Caps Lock para um modificador que o niri não trate como `Mod`?
2. **Salas = contextos do core**, com gatos e janelas do mesmo projeto juntos e uma "minha sala" para o resto: é isso, ou você quer janelas e gatos em salas separadas?
3. **Ordem**: gatos (fase 2) antes do ANDAR (fase 3)?
4. **Raízes da biblioteca**: `~` inteiro com a lista de exclusão, ou só pastas escolhidas (por exemplo `~/dev` e `~/Documentos`)?
5. **Sessão própria (fase 7)**: você pretende um dia entrar direto no mundo pelo greeter, ou o mundo aninhado no niri basta? A resposta decide se a fase 7 fica no plano.

## 12. Primeiras duas semanas (checklist)

**Semana 1**

- [ ] Branch `mundo/fase-0-spike` a partir de `main` (nunca push em `main`); PR em rascunho.
- [ ] Este documento em `docs/plano-mundo.md`, com entrada no `docs/index.md`.
- [ ] Workspace:
  - [ ] crates `nkw-core`, `nkw-gl`, `nkw` e `nkw-testclient`;
  - [ ] perfil `world`;
  - [ ] alvos `world-test`, `world-lint`, `run-world`, `world-shot`, `world-perf` no Makefile.
- [ ] `nkw-core/tests/deps.rs` (regra de dependência via `cargo metadata`), vermelho antes de existir dependência proibida.
- [ ] TDD no `nkw-core`:
  - [ ] `mode.rs`: NAV↔FOCO com retorno ao modo de origem;
  - [ ] `strip.rs`: x das mesas pelos presets;
  - [ ] `onetoone.rs`: pose 1:1 e ajuste ao pixel;
  - [ ] `tween.rs`: deslize ≤ 260 ms e corte acima de 8 m;
  - [ ] `schedule.rs`: nenhum timer sem vontade; coalescência.
- [ ] `nkw`: Smithay winit, `calloop`, socket `nkw-0`, xdg-shell, wl_shm, seat (XKB do `world.toml`/env), output. `nkw -- foot` lança com o `WAYLAND_DISPLAY` do mundo. Primeiro as janelas em 2D lado a lado, para validar os protocolos antes do 3D.
- [ ] `nkw-testclient`: toplevel de cor ou xadrez de 1 px; log JSON de configure, enter, leave, tecla e frame com carimbo de tempo.

**Semana 2**

- [ ] `nkw-gl`:
  - [ ] glow com `get_proc_address` do EGL do Smithay;
  - [ ] textura offscreen do mundo;
  - [ ] chão e parede, 3 mesas;
  - [ ] quads com o `tex_id` das janelas (shader com variante external);
  - [ ] restauração do estado GL, com teste.
- [ ] Composição: `TextureRenderElement` do mundo e a janela em FOCO em 2D por cima, com o `OutputDamageTracker`.
- [ ] NAV `h`/`l` com deslize; `enter` leva ao FOCO 1:1 (troca do quad pelo 2D na chegada); `ctrl+]` volta. O filtro do `KeyboardHandle` intercepta só o escape e os acordes `world+`.
- [ ] Laço por dano sem timers ociosos; `throttle` do `send_frame` por LOD; cursor do host.
- [ ] Backend headless (`EGLDevice`/llvmpipe), `--shot` e `--script`. Primeiro teste de integração: o FOCO recebe as teclas e o NAV não.
- [ ] `world-perf.sh` (S1, S2, S4) e `NKW_STATS`. Medir na máquina alvo, registrar em `docs/log.md` e **escrever no PR a decisão de seguir ou parar** contra os 7 critérios da fase 0.
- [ ] Em paralelo, outro agent: `nk-client` sem o polling de 8 ms do painel e de 50 ms do keeper (§6.10), com teste de wakeups; `make office-test` e o `nkg` verdes.
