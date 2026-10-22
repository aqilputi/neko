---
okf_version: "0.2"
type: Plan
title: Plano — objetos do mundo (nkw): widgets como móveis, FEEL, áudio por proximidade, portas e o cofre
description: Adendo ao plano do mundo. Os widgets de um WM viram objetos da casa (vitrola, relógio, lareira…), a sensação de uso ganha uma especificação (easing, sons, latência, pacing), as portas passam a ter estado e abafam o som das salas fechadas, e as pastas sensíveis vão para um cofre na biblioteca, com destrancamento por frase, auto-trancar e auditoria.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/office
tags: [plano, mundo, nkw, objetos, widgets, vitrola, mpris, feel, áudio, proximidade, portas, pipewire, cofre, biblioteca, privacidade, animal-crossing, gatos]
timestamp: 2026-10-21T23:30:00Z
generated: { by: agent:claude-code/claude-fable-5-1, at: 2026-10-21T23:30:00Z }
status: draft
stale_after: 2027-04-21T00:00:00Z
sources:
  - id: plano-mundo
    resource: plano-mundo.md
    title: Plano — o mundo (nkw); este adendo não o substitui, estende
  - id: pesquisa-widgets
    resource: "pesquisa interna de 2026-10-21 (widgets do desktop como objetos)"
    title: Fontes Linux por widget, crates leves, regra de proximidade, privacidade
  - id: pesquisa-audio-cofre
    resource: "pesquisa interna de 2026-10-21 (áudio por proximidade e cofre)"
    title: pw-dump real, pid→stream, filter-chain, fórmulas de atenuação, opções de destrancamento
  - id: pesquisa-toolchain
    resource: "pesquisa interna de 2026-10-21 (Smithay + glow)"
    title: Rev do Smithay, GlowRenderer, glow 0.16, smallvil, custo de build medido
  - id: mpris
    resource: https://specifications.freedesktop.org/mpris-spec/latest/
    title: MPRIS D-Bus Interface Specification
  - id: upower
    resource: https://upower.freedesktop.org/docs/Device.html
    title: UPower — Device
  - id: networkmanager
    resource: https://networkmanager.dev/docs/api/latest/
    title: NetworkManager D-Bus API
  - id: psi
    resource: https://www.kernel.org/doc/Documentation/accounting/psi.txt
    title: Pressure Stall Information
  - id: pipewire-props
    resource: https://docs.pipewire.org/page_man_pipewire-props_7.html
    title: pipewire-props(7) — application.process.id, pipewire.sec.pid
  - id: pipewire-filter-chain
    resource: https://docs.pipewire.org/page_module_filter_chain.html
    title: libpipewire-module-filter-chain (bq_lowpass)
  - id: pipewire-rs
    resource: https://pipewire.pages.freedesktop.org/pipewire-rs/pipewire/
    title: pipewire-rs
  - id: zbus
    resource: https://docs.rs/zbus/5/zbus/
    title: zbus 5
  - id: rustsni
    resource: https://docs.rs/rustsni/latest/rustsni/
    title: rustsni — StatusNotifierItem/Watcher e DBusMenu em Rust puro
  - id: keepassxc
    resource: https://keepassxc.org/docs/KeePassXC_UserGuide
    title: KeePassXC — regras de auto-lock (prior art do cofre)
  - id: webaudio-panner
    resource: https://www.w3.org/TR/webaudio/#PannerNode
    title: Web Audio PannerNode — modelo "inverse distance clamped"
---

# Plano — objetos do mundo (adendo ao `plano-mundo.md`)

Três pedidos novos, na ordem em que chegaram:

1. **As pastas sensíveis vão para um cofre** na biblioteca, em vez de só sumirem. Com atenção ao desempenho e
   ao **FEEL**.
2. **Os widgets e informações que um WM costuma ter** (bandeja, relógio, bateria, rede, mídia…) viram **objetos
   do mundo real**: uma vitrola para ouvir música, por exemplo.
3. **Proximidade**: se eu fecho a porta de uma sala, não ouço o que está lá dentro.

A direção de arte continua a do plano: Animal Crossing, primeira pessoa, tudo o que é vivo é um gato. Os
orçamentos de leveza de `plano-mundo.md` §1.3 continuam valendo, sem exceção para os objetos.

## 0. Em uma página

- **Cada widget é um móvel.** A informação mora em um objeto numa sala, com um lugar fixo. Você vai até ele,
  olha, e só de perto lê o número exato. De longe só vê o efeito ambiente (o fogo mais alto, a planta murcha).
  Isso é o "glanceable" do AC: você percebe que algo mudou antes de saber o valor.
- **Três raios**, reaproveitando o que o plano já tem: **interagir** (4 m, a mira do ANDAR), **ler** (1,8 m, cone
  de ±50°) e **ambiente** (o resto). O HUD só entra para o que é crítico (bateria baixa, rede caiu, gravação
  de tela, cofre aberto), como selo, sem roubar foco.
- **Vitrola = mídia (MPRIS) + volume.** Pilha de capas (`artUrl`), duas caixas que pulsam, um manípulo de
  volume. É a primeira "coisa viva" sem gato, e entra cedo (fase 2) por ser isolada.
- **Portas viram objetos com estado** (`Closed | Ajar(f) | Open`). Uma única fonte de verdade alimenta quatro
  consequências: colisão, culling, áudio e o throttle dos callbacks de quadro das janelas da sala fechada.
- **Áudio por proximidade**: uma função pura `gains(listener, sources, rooms)` devolve ganho, pan e um corte
  de passa-baixa por fonte. Os streams dos apps são ancorados às janelas (pid → `ppid` → cliente Wayland), a
  música à vitrola. **v1 só atenua** (−24 dB por porta fechada); o filtro de verdade no PipeWire fica mapeado
  para depois.
- **O cofre** é um objeto na biblioteca, guardado pela bibliotecária. Destrancar é digitar uma frase na caixa
  de diálogo dela (zero dependência nova). Mostra **só nomes**, tranca sozinho (20 s, sair da sala, perder o
  foco, entrar no FOCO, captura, screenshot), nunca vai para agentes nem para o core, e deixa um log de
  auditoria `0600`.
- **Leveza**: nenhum objeto faz polling. D-Bus é evento; relógio é um `timerfd` por minuto; CPU/RAM usam PSI
  por evento e só medem o número exato quando você está de frente; animações ambientes só rodam para objetos
  visíveis e perto. Orçamento ocioso continua ≤ 1 wakeup/s (§2.5).
- **Crates novos**: `nkw-objects` (puro: acústica, portas, cofre, objetos, proximidade, mixer) e `nkw-sys`
  (adaptadores: zbus 5, timerfd, pipewire-rs). Nenhum deles toca o Smithay.

## 1. Princípios

1. **Diegético primeiro.** Se uma informação pode ser um objeto, ela é um objeto. O HUD é exceção e é selo.
2. **Nada empurra número para cima de você.** Ler exige aproximar-se e olhar (`plano-mundo.md` §3.1 e D9).
3. **Nada rouba o foco.** Objeto nenhum muda o modo, abre janela ou move a câmera sozinho.
4. **Zero polling.** Evento, ou nada. Onde o Linux não dá evento (CPU/RAM exatos), medimos só quando você está
   de frente, e a ≤ 1 Hz.
5. **Privacidade por padrão.** SSID, nomes de dispositivos, conteúdo de notificação e os nomes do cofre vivem
   só na memória do `nkw`. O modo `--demo` nunca lê a máquina; screenshots só saem do demo.
6. **O FEEL é especificado, não improvisado.** Durações, curvas, sons e latências têm número (§3).

## 2. Catálogo: widget → objeto

### 2.1 Os objetos

Convenção: **Recepção** é a sala de entrada (já tem a recepcionista e o quadro de avisos). "Sala do contexto"
é a sala de cada projeto. "HUD" é o selo 2D. A coluna "wakeups" é o custo ocioso do objeto, por si só.

| widget | objeto | onde | fonte de dado · crate | cadência / wakeups | fase |
|---|---|---|---|---|---|
| mídia (player) | **vitrola** com pilha de capas, duas caixas de som e manípulo de volume | Recepção (som ambiente da casa); `[objects] turntable.room` | MPRIS2 · `zbus` 5, proxy à mão (`nkw-sys/src/dbus/mpris.rs`) | evento (`PropertiesChanged`); capa carregada 1× por faixa; disco gira a 12 fps só se visível e ≤ 6 m | 2 |
| volume (sink padrão) | o **manípulo** da vitrola; mudo = agulha levantada | idem | PipeWire · `pipewire-rs` 0.10 (mesma conexão do áudio por proximidade) | evento | 2 |
| relógio | **relógio de parede** + a **luz da janela** (hora do dia já existe em §5.2 do plano) | Recepção e salas (um por sala, barato: é uma instância) | `timerfd` por minuto (`CLOCK_REALTIME`, `CANCEL_ON_SET`) · `libc` | 1 wakeup/min; a faixa do dia (8) só re-renderiza quando muda | 2 |
| bateria | **pote de óleo de lamparina** com um gato dormindo na almofada ao lado; o nível de óleo é a carga; < 15 % o gato acorda e mia | Recepção, perto da porta de entrada | UPower `DisplayDevice` · `zbus` | evento | 2 |
| rede | **planta-antena** na janela: folhas = barras; sem rede, murcha; cabo = vaso de cerâmica | Recepção | NetworkManager · `zbus` (sem NM: objeto "sem dado", documentado) | evento | 5 |
| notificações | **gato carteiro** (já existe, §6 do plano) | anda até você | `org.freedesktop.Notifications`: best-effort `BecomeMonitor`; "modo completo" = `nkw` dono do nome (§2.7) | evento | 6 (best-effort), 7 (próprio) |
| bandeja (tray) | **prateleira de potes** ao lado da porta, um pote por app registrado, com etiqueta (ícone) | Recepção, porta | StatusNotifierItem/Watcher + DBusMenu · `rustsni` (fd no `calloop`, sem threads) | evento | 6 |
| calendário | **calendário de parede**; eventos do dia como post-its | Recepção | vdir local (`.ics`) · `inotify` + 1 timer/dia | evento + 1/dia | 6, opcional |
| CPU | **lareira**: a altura do fogo é a pressão | sala do contexto (uma por sala) ou Recepção (`[objects] load.scope = "machine"`) | PSI `/proc/pressure/cpu` (evento) · `libc`; número exato por `/proc/stat` só de frente, ≤ 1 Hz | evento; 1 Hz só quando lido | 5 |
| RAM | **aquário**: a água fica turva quando aperta | idem | PSI `memory` + `/proc/meminfo` só de frente | idem | 5 |
| gravação de tela | **placa "ON AIR"** vermelha na parede + selo no HUD sempre que ativo | Recepção + HUD | só na sessão própria (o `nkw` serve a captura); aninhado: não observável, documentado | evento | 7 |
| atualizações | **caixa de encomenda** na porta, com o número | Recepção, porta | comando configurável 1×/dia (`checkupdates`, `apt list --upgradable`…); PackageKit se existir | 1/dia | 6, baixa |
| layout de teclado | **bandeirinha** numa mesinha e o mesmo texto no HUD | HUD + Recepção | estado do próprio seat (XKB) | evento interno | 1 |
| bluetooth | **rádio antigo** de canto; a antena acende quando pareado; bateria do fone num mostrador | Recepção | BlueZ `ObjectManager` · `zbus` | evento | 6, baixa |
| brilho | **lustre/dimmer** na Recepção; o sol do mundo acompanha | Recepção | leitura `sysfs` (`poll` POLLPRI); escrita `login1.Session.SetBrightness` | evento | 6 |
| ocioso / sessão travada | **a luz apaga e a cortina cai** (vira noite) | mundo inteiro | `login1` (`PrepareForSleep`, `LockedHint`) | evento | 7 |
| clima | **biruta** na janela; **desligado por padrão**; exige cidade/coordenada explícita no `world.toml`, nunca IP | Recepção | serviço externo, 1/h | 1/h | 6+, opcional |
| custo do dia e inbox | **quadro de avisos** (já no plano) | Recepção | core | evento | 2 |

Decisões de produto que a pesquisa deixou abertas:

- **Lareira e aquário são dois objetos** (CPU é "trabalho agora", RAM é "quanto está guardado"), não uma roda de
  hamster. Padrão: **uma por sala de contexto**, mostrando a carga da máquina inteira (a carga por projeto não é
  mensurável sem cgroups; fica como ideia para depois). Quem preferir uma só, usa `load.scope = "machine"`.
- **Bandeja, carteiro e encomendas ficam na zona da porta**: tudo o que "chega de fora" entra pelo mesmo lugar.
- **O ON AIR nunca pode estar apagado com captura ativa** (fase 7). É requisito de aceite, não feature.

### 2.2 Interações por modo

Os objetos obedecem às mesmas regras dos outros alvos do mundo (`plano-mundo.md` §3): nada de tecla nova
fora do contexto do objeto, `esc` volta, nada muda o modo sozinho.

| modo | como chegar | o que dá para fazer |
|---|---|---|
| **ANDAR** | mirar (≤ 4 m, cone de 8°) | `enter`/`space` = `world.interact` no objeto; `j`/`k` = `world.adjust` (manípulo, dimmer); `h`/`l` = anterior/próxima (vitrola, calendário) |
| **NAV** | `f` (SALTO): todo objeto visível ganha rótulo; `Pick::Object` leva a câmera ao **ponto de parada** do objeto (`Object.stand`), que vira uma estação `Free(pose)` — não há variante nova de `Station` | as mesmas teclas de ANDAR valem na estação `Free` em frente ao objeto; `h`/`l` saem para a estação vizinha como hoje |
| **CONVERSA** | só o cofre (§5.3): `t` na frente do cofre fala com a bibliotecária | a frase de destrancar é digitada na linha da conversa |
| **FOCO** | nunca | os selos do HUD aparecem (bateria crítica, ON AIR, cofre aberto); o resto espera você sair |
| **COMANDOS** | `media.play_pause`, `media.next`, `media.prev`, `volume.up/down/mute`, `brightness.up/down`, `world.goto_object <nome>` | atalhos para quem não quer andar; agem no mesmo modelo, sem câmera |

Teclas globais opcionais (`world` chord, religáveis em Lua como as outras): `world+m` play/pause,
`world+,`/`world+.` faixa, `world+-`/`world+=` volume. Desligadas por padrão para não ocupar acordes; quem usa
teclas de mídia físicas (`XF86Audio*`) as tem ligadas por padrão no modo `world`.

A vitrola em detalhe (é o exemplo do pedido):

- **Capas**: a pilha ao lado mostra a capa atual (`mpris:artUrl`, `file://` ou `https://`; a `https` é baixada
  numa thread com teto de 512 KB e cache em memória por URL; sem rede ou sem capa, uma capa genérica com o
  título rasterizado). A capa é uma textura de 128×128 no atlas dos objetos.
- **Caixas de som**: duas, que "respiram" com o volume (escala 1,0 → 1,04 a 2 Hz) só enquanto toca e você
  está a ≤ 6 m. Não há análise de espectro: é o volume, não o áudio.
- **Manípulo**: `j`/`k` giram ±18° por passo = ±5 % do volume do sink padrão; o número aparece como rótulo 2D
  (§5.4 do plano) só no raio de leitura.
- **Agulha**: pausado = agulha levantada; mudo = agulha levantada e o disco parado.
- **Som da vitrola no mundo**: os streams do player de música são **ancorados à posição da vitrola** (§4.5).
  Fechar a porta da Recepção abafa a música. É o pedido 3 aplicado ao pedido 2.
- **Sem player** (nenhum nome `org.mpris.MediaPlayer2.*` no barramento): a vitrola existe, com a tampa fechada
  e sem capas; `interact` abre a paleta nos apps de mídia.

### 2.3 Regra de proximidade e raios

| raio | valor | para quê | de onde vem |
|---|---|---|---|
| interagir | 4,0 m, cone de 8° na mira | `interact`/`adjust` em ANDAR | `walk.reach` (§3.5 do plano) |
| ler | 1,8 m, cone de ±50° do olhar | o texto exato vira sobreposição 2D (42 %, "3 atualizações", 14:05) | pesquisa-widgets §4; é menor que o de interagir porque ler pede estar de frente |
| ambiente | além de 1,8 m, dentro da névoa (40 m) | só o efeito (fogo, água, folhas, nível de óleo) | — |
| gatos notam você | 2,5 m (orelhas e cabeça viram); 1,3 m (rabo) | §3.5 | novo |
| porta "segurada" | 2,0 m | o auto-fechar não dispara com você ao lado | pesquisa-audio-cofre A.4 |
| áudio | referência 1 m, platô 12 m | §4.2 | Web Audio |

**Fallback de HUD** (selos, nunca pop-ups, visíveis até no FOCO, no canto do selo da inbox): bateria < 15 % sem
carregar; rede caiu (só se já esteve conectada nesta sessão); ON AIR enquanto ativo (fase 7); **cofre
destrancado** enquanto destrancado; atualizações > 0 (selo pequeno, some ao ler a caixa). Mais nada.

### 2.4 Extensão mínima do `nkw-core`

O `nkw-core` não muda nesta onda. Quando os crates novos estiverem integrados, o integrador aplica só isto:

```rust
// ids.rs: + ObjectId, DoorId, SourceId (hoje definidos em nkw-objects::ids; passam a ser re-exportados de lá)
// mode.rs
pub enum Pick { …, Object(ObjectId) }                 // rótulo de SALTO num objeto
pub enum Action { …, Interact(ObjectId), Adjust(ObjectId, i8), Prev(ObjectId), Next(ObjectId) }
pub enum Effect { …, Object(ObjectId, ObjectAction) /* o nkw chama nkw_objects::World::apply */, GoToStand(Pose) }
// Talk ganha um propósito, para a conversa com a bibliotecária levar a frase ao cofre e não ao core:
pub enum Kind { …, Talk { cat: CatId, line: bool, purpose: TalkPurpose } }
pub enum TalkPurpose { Session, VaultUnlock }
```

`Pick::Object` em NAV produz `Effect::GoToStand(obj.stand)` e a estação vira `Free(pose)`; em ANDAR, `interact`
com objeto na mira vira `Action::Interact`. Nenhum modo novo, nenhuma sobreposição nova.

### 2.5 Orçamento de wakeups (ocioso, NAV parado, todos os objetos presentes)

| fonte | wakeups/s | nota |
|---|---|---|
| relógio (`timerfd`) | 1/60 | re-render só quando a faixa do dia muda (8 por dia) |
| D-Bus (MPRIS, UPower, NM, BlueZ) | 0 em repouso | a thread do zbus dorme no `epoll`; cada evento real acorda o laço 1× |
| PipeWire (registro + volumes) | 0 em repouso | a thread do PipeWire dorme no `epoll`; `set_param` só por mudança > 0,5 dB |
| PSI | 0 em repouso | o kernel só acorda ao cruzar o limiar (janela de 2 s para usuário comum) |
| `/proc/stat`, `/proc/meminfo` | 0 | só a 1 Hz quando você está no raio de leitura da lareira/aquário |
| capa, clima, atualizações | 0 | capa por faixa; clima 1/h se ligado; atualizações 1/dia |
| animações ambientes | 0 | `Want::Rate` registrado só para objetos visíveis e ≤ 12 m; nenhum em NAV parado sem música |
| **total** | **≤ 0,02 + eventos** | dentro do S1 (≤ 1/s) do plano |

Com música tocando e a vitrola à vista: + disco a 12 fps e caixas a 2 Hz (um `Want::Rate(12)` só). Com a
vitrola fora da vista ou da sala: nada.

### 2.6 Dados, adaptadores e o que nunca sai do processo

- **`nkw-sys`** (crate novo) entrega `SysEvent`s decodificados por um canal, mais um `wake()` que o `nkw` liga
  ao `calloop::ping`. Padrão de forma do niri (um arquivo por interface, `zbus::blocking::Connection`), nunca o
  código dele.
- **Os eventos não carregam identificadores pessoais**: `NetState` tem `strength` e `wifi`, **não** o SSID; o
  SSID aparece só no rótulo 2D, lido do proxy na hora, e nunca entra em `world.json`, logs, `events.jsonl`
  ou `world.dump()`.
- **`--demo`** usa `nkw_sys::demo::fixture()`: bateria 73 % descarregando, Wi-Fi com 3 barras, "Lo-fi for cats
  — acme radio", relógio de `now`. É o único conjunto que aparece em screenshots e docs.

### 2.7 Notificações

`org.freedesktop.Notifications` tem um dono só. Com mako/dunst rodando:

1. **best-effort** (fase 6): `BecomeMonitor` no barramento de sessão; se a política permitir, o carteiro traz a
   notificação inteira; se não, log claro e nada mais.
2. **modo completo** (fase 7 ou `[notifications] own = true`): o `nkw` é o dono do nome; documentado como "desligue
   o mako".
3. Conteúdo de notificação **só na memória**, pelo tempo da entrega; nunca em disco, nunca no core.

## 3. FEEL

O que faz o mundo parecer um lugar e não um menu. Todas as durações são as do `nkw-core::tween` quando já
existem; as novas seguem a mesma família de curvas.

### 3.1 Curvas e durações

| o quê | duração | curva | nota |
|---|---|---|---|
| deslize de câmera (estações, FOCO) | 120–260 ms | ease-in-out cúbico | já em `tween.rs`; corte com fade de 120 ms acima de 8 m |
| ir ao ponto de parada de um objeto (SALTO) | ≤ 260 ms | idem | a câmera para a ~1,2 m do objeto, de frente |
| manípulo girando | 120 ms | ease-out quadrático | sem overshoot: é um knob, não uma mola |
| agulha sobe/desce | 180 ms | ease-out cúbico | um "tac" no fim (§3.4) |
| porta abrindo/fechando | 320 ms | ease-out cúbico, sem bounce | §3.3 |
| tampa do cofre | 420 ms | ease-out cúbico + 60 ms de assentamento | §5.10 |
| rótulo 2D aparecendo (raio de leitura) | 80 ms fade | linear | some em 120 ms ao sair do raio |
| contorno de seleção / hover | 80 ms | linear | nunca pisca |
| caixa de diálogo | 140 ms (sobe 12 px + fade) | ease-out | texto a 45 c/s, como no plano |
| toast da recepcionista | 140 ms in, 200 ms out | ease-out / ease-in | 3 s info, 6 s erro |
| capa trocando na vitrola | 220 ms (desliza e troca) | ease-in-out | não anima com movimento reduzido |
| gato virando para você | 260 ms | ease-in-out | já no plano (§3.3) |
| movimento reduzido | **todas viram 0 ms** (corte), exceto o deslize de câmera que vira corte com fade | — | `--reduced-motion` |

### 3.2 Passos

- Sintetizados (sem arquivos), um "toc" curto filtrado, timbre por piso: madeira (grave, curto), tapete
  (abafado, mais longo), azulejo (agudo, com um pouco de reverb curto). O piso vem do padrão da sala
  (`plano-mundo.md` §5.2).
- Cadência pela distância percorrida: **um passo a cada 0,7 m** (2,8 m/s ⇒ 4 passos/s andando, 8 correndo),
  alternando pan −0,15/+0,15. Volume 0,25 do máximo; correndo 0,35. Parado, nenhum passo e nenhum timer.
- Desligados por padrão como todo o som (`[sound] enabled = false`); com som ligado, `[sound] footsteps`
  desliga só eles.

### 3.3 Portas: animação e som

- Abrir: a folha gira 90° em 320 ms (ease-out cúbico); um ranger curto (ruído filtrado com varredura de
  400→900 Hz, 250 ms) no começo. Fechar: gira em 320 ms, um "thunk" grave (60 ms) ao encostar.
- Auto-fechar (8 s, `doors.auto_close_after_s`, `0` desliga) fecha devagar (480 ms) e sem ranger, só o thunk
  mais baixo: é ambiente, não evento.
- A porta **nunca fecha com você no vão** (a célula ocupada pelo jogador bloqueia o fechamento; ela espera).
- Com movimento reduzido: a porta troca de estado num corte; o som fica.

### 3.4 Bipes de diálogo e cliques

- "Animalese": um bipe por caractere visível (não por espaço), 25–40 ms, onda triangular com envelope curto,
  altura por gato (hash do id → uma de 7 alturas) e uma variação ±4 % por caractere. Vogais um pouco mais
  longas. Volume 0,2. A bibliotecária e a recepcionista têm vozes fixas.
- Cliques de interface (manípulo, agulha, seleção, cofre): 10–20 ms, volume 0,15. Um só por ação.
- Miado: `notice.meow` e bateria crítica (uma vez, com histerese de 3 %).

### 3.5 Gatos reagindo a você

- A **2,5 m** e dentro do seu cone de leitura: orelhas e cabeça viram para você (rajada de 15 fps, 400 ms), e
  voltam ao trabalho. Não repete por 20 s para o mesmo gato.
- A **1,3 m**: rabo balança enquanto você fica; gatos bloqueados fazem um "mrrp" (se o som estiver ligado).
- Filhotes olham, mas não param de digitar.
- Nunca andam até você por causa de proximidade: isso é a inbox (§6.6 do plano).

### 3.6 Hover, mira e seleção

- A mira do ANDAR é um ponto; vira um anel de 6 px quando há algo interagível na mira (80 ms).
- O objeto na mira ganha contorno suave (o mesmo da tela selecionada) e, dentro do raio de leitura, o rótulo
  2D com o nome e o valor.
- Com mouse em NAV, hover só destaca (raio na CPU quando o alvo muda); clique interage.

### 3.7 Latências alvo

| de | até | alvo | como se mede |
|---|---|---|---|
| tecla no FOCO | cliente recebe | < 1 ms, mesma iteração do laço | `NKW_STATS` `inject` × log do testclient |
| tecla de modo (`h`, `enter`, `interact`) | primeiro quadro da resposta | próximo vblank (≤ 16,7 ms a 60 Hz) | idem |
| `interact` na vitrola | comando D-Bus enviado | ≤ 2 ms | timestamp no `nkw-sys` |
| andar / fechar porta | ganho de áudio aplicado | ≤ 50 ms (20 Hz) + suavização (τ 120 ms) | `nkw-sys` |
| evento D-Bus (bateria, faixa) | objeto atualizado no quadro | próximo quadro | — |
| som próprio (passo, clique) | áudio sai | ≤ 1 quantum do PipeWire (5–20 ms) | — |

### 3.8 Frame pacing

- A câmera anda na taxa do host (como no plano). Gatos a 15 fps; carteiros a 30; texto a 30 Hz.
- Objetos: disco da vitrola 12 fps, caixas 2 Hz, fogo 8 fps, água 6 fps, folhas 4 fps, mostrador do rádio
  2 fps — **só se visíveis, dentro da névoa e ≤ 12 m**, e todos compartilham a grade de vblank do agendador
  (`Want::Rate`), então 12 + 8 + 6 fps viram no máximo 12 quadros/s, não 26.
- Teto: no máximo 4 `Want::Rate` de objetos ao mesmo tempo; o resto congela até entrar no topo da lista por
  distância.
- Durante o FOCO nada disso roda (o mundo congela), inclusive o disco.

### 3.9 Nada rouba o foco

- Objetos nunca mudam o modo, nunca abrem janela, nunca movem a câmera.
- A vitrola não abre o player; `interact` manda `PlayPause`. "Abrir o player" é uma escolha na paleta.
- A caixa de encomendas mostra a lista; aplicar a atualização abre um terminal **com o comando digitado, sem
  executar** (nunca `sudo` automático).
- Carteiros e a recepcionista continuam como no plano: selo no FOCO, gato ao seu lado fora dele.

### 3.10 Guardrails de desempenho (gates, não desejos)

| regra | como se garante |
|---|---|
| nenhum timer ocioso por objeto | `nkw_objects::World::want` devolve `None` para tudo fora da vista; teste de tabela |
| ≤ 4 `Rate` de objetos simultâneos | teste no agendamento do `nkw` |
| rótulos 2D re-rasterizados só quando o texto muda | cache por `(ObjectId, String)` |
| `set_param` de volume só com mudança > 0,5 dB ou pan > 0,02, ≤ 20 Hz | `Smoothed::step` devolve `false` sem mudança (teste) |
| eventos D-Bus coalescidos por quadro | o laço drena o canal inteiro antes de planejar |
| capa ≤ 512 KB, 128×128 no atlas | teto no download e no decode |
| `/proc/stat` só no raio de leitura e ≤ 1 Hz | `want` da lareira em `Readable` = `Rate(1)`, senão `None` |
| S1 do plano continua ≤ 0,1 % / ≤ 1 wakeup/s com todos os objetos | `make world-perf` com o cenário S1 ganhando os objetos do demo |
| +2 MB de binário no máximo para `nkw-sys` (zbus + pipewire) | medido na fase 2 |

## 4. Áudio por proximidade e portas

### 4.1 O modelo puro

Tudo em `nkw-objects::acoustic`, sem tempo e sem estado:

```rust
pub struct Listener { pos: Vec2, yaw: f32, room: RoomId }
pub struct Source   { id: SourceId, pos: Vec2, room: RoomId, own_volume: f32 }
pub struct Portal   { id: DoorId, rooms: (RoomId, RoomId), pos: Vec2, openness: f32 }  // 0 fechada … 1 aberta
pub struct SourceGain { id: SourceId, gain: f32, pan: f32, lowpass_hz: Option<f32> }
pub fn gains(p: &Params, l: &Listener, sources: &[Source], g: &RoomGraph) -> Vec<SourceGain>;
pub fn pan_to_stereo(gain: f32, pan: f32, mono: bool) -> (f32, f32);
pub fn smooth_towards(cur: f32, target: f32, dt_s: f32, tau_s: f32) -> f32;
```

Mesmo input, mesmo output: dá para testar por tabela. A suavização (que tem estado) fica fora, em `Smoothed`.

### 4.2 Fórmulas e parâmetros

**Distância (mesma sala)** — "inverse distance clamped", o do `PannerNode` do Web Audio:

```
d        = max(|src − lis|, ref)        ref = 1,0 m
dc       = min(d, max)                  max = 12 m   (platô, não corte: sem degrau ao sair do alcance)
gain_d   = ref / (ref + rolloff · (dc − ref))        rolloff = 1,0
```

Invariantes: `gain_d(ref) = 1`; decrescente; `gain_d(d ≥ max) = gain_d(max)`.

**Pan (mesma sala)** — o vetor fonte−ouvinte nos eixos do ouvinte:

```
local = rot(−yaw) · (src − lis)
pan   = clamp(local.x / (|local| + 1e-4), −1, 1)
(l, r) = (gain · cos(θ), gain · sin(θ)),  θ = (pan + 1) · π/4      // potência constante: l² + r² = gain²
mono: l = r = gain / √2
```

**Porta (sala vizinha)** — o som sai pela porta: a distância é pelo portal e o pan aponta para a porta:

```
d_path     = |src − door| + |door − lis|
extra_db   = lerp(−24, 0, openness)            // fechada −24 dB … aberta 0 dB, linear em dB
lowpass_hz = lerp(800, 20000, openness)        // 20 kHz = "sem filtro"; uma fórmula só, sem casos
gain       = gain_d(d_path) · 10^(extra_db/20) · own_volume
```

**Mais de uma porta**: menor caminho no grafo de salas (BFS); os `extra_db` somam (duas fechadas = −48 dB), o
`lowpass` é o mínimo; acima de `max_hops = 2`, `gain = 0`. **Sem caminho**: `gain = 0`.

**Parâmetros** (`[audio]` no `world.toml`): `ref_distance`, `max_distance`, `rolloff`, `closed_db`,
`closed_lowpass_hz`, `max_hops`, `mono`.

### 4.3 Suavização (anti-zipper)

Por fonte, por campo, a cada tick do mundo: `cur += (target − cur) · (1 − e^(−dt/τ))`.

| campo | τ | por quê |
|---|---|---|
| gain | 120 ms | responde andando, não estala |
| pan | 80 ms | pan salta menos que volume |
| lowpass | 150 ms | cortes rápidos "chiam" |

`Smoothed::step` devolve `true` só quando a mudança passa de 0,5 dB ou 0,02 de pan: é isso que limita as
chamadas ao PipeWire.

### 4.4 Portas como objetos com estado

Hoje a porta é um vão de 1,2 m sempre aberto na grade de colisão. Passa a ser `nkw_objects::door::Door`:

| estado | colisão | culling | áudio | callbacks de quadro das janelas da sala atrás |
|---|---|---|---|---|
| `Open` | vão livre | a sala atrás entra no conjunto visível (§5.7 do plano) | 0 dB extra | pela tabela de LOD normal |
| `Ajar(f)` | passável se `f ≥ 0,5` | visível se `f > 0` | lerp em dB | idem |
| `Closed` | a folha ocupa as células do vão | a sala atrás some do conjunto | −24 dB (+ filtro, quando houver) | **1 Hz** ("fora da vista") mesmo que a sala fosse visível pela geometria |

- **Tecla**: `interact` com a porta na mira alterna. No NAV, o rótulo de SALTO da porta leva até ela
  (`Door(room)` já existe em `Pick`); `enter` na porta abre e atravessa, como hoje.
- **Auto-fechar**: 8 s depois da última passagem, só com você a mais de 2 m; a porta espera se a célula do
  vão está ocupada.
- **Portas abertas por gatos**: um carteiro atravessando uma porta fechada a abre (e ela fecha sozinha depois).
  O A* dos gatos (§6.6 do plano) trata porta fechada como custo +2 m, não como parede.
- **Estado persistido** em `world.json` (`doors: [{room_a, room_b, state}]`).
- Uma fonte de verdade (`Door::openness(now)`) alimenta as quatro colunas. Nenhuma duplicação.

### 4.5 Onde cada som está

| fonte | âncora (posição e sala) | como |
|---|---|---|
| player de música (o que a vitrola mostra) | a vitrola | o nome MPRIS do player → `application.name`/`binary` do stream, ou pid do player (`org.mpris.MediaPlayer2.<x>` → `GetConnectionUnixProcessID`) |
| app com janela no mundo | a tela da janela (centro) | `application.process.id` do stream → sobe `ppid` (≤ 6 saltos) até um pid de cliente Wayland (`SO_PEERCRED` do Smithay); fallback: `binary`/`app_id` ↔ `app_id` do toplevel; flatpak: `pipewire.access.portal.app_id` |
| vários streams do mesmo app (voz, campainha…) | a mesma janela | cada stream é uma fonte própria; convergem para os mesmos ganhos |
| stream sem âncora (app fora do mundo, no niri) | nenhuma: `gain = own_volume`, sem pan, sem filtro | o mundo não toca no que não é dele |
| sons próprios (passos, portas, miados, bipes) | onde acontecem (o gato, a porta, você) | o mixer aplica as mesmas regras: um miado atrás de uma porta fechada também é abafado |

`pipewire.sec.pid` **não serve** de âncora: para tudo o que passa pelo `pipewire-pulse` (a maioria dos apps) ele
aponta para o daemon. É o que o `pw-dump` real mostrou (pesquisa-audio-cofre A.1.1).

### 4.6 O adaptador (`nkw-sys`, pipewire-rs 0.10)

- Uma conexão persistente numa thread (`MainLoop` + `Context` + `Core`), registro de `Node`s de
  `Stream/Output/Audio`, `set_param(Props { channelVolumes: [l, r] })` por fonte. Pan e distância na mesma
  chamada. Nada de `wpctl`/`pactl` (fork por chamada) nem `libpulse` (um salto de protocolo a mais).
- Taxas: ≤ 20 Hz com você ou uma fonte em movimento; **0 Hz** parado; 1 chamada imediata quando uma porta muda.
- O volume do sink padrão (o manípulo) é a mesma conexão.
- **Restauração**: ao sair, e ao perder uma fonte, o `nkw` devolve `channelVolumes = [1, 1]` ao que mexeu. Os
  valores originais ficam numa tabela em memória; nunca em disco.
- Com o PipeWire ausente (ou a feature desligada): o mundo fica mudo e sem controle, sem erro além de um log.

### 4.7 O mixer de sons próprios

- Vozes `{cue, t, gain, pan, lowpass}` em `nkw-objects::sound::Mixer`, tudo gerado em código (sem assets).
- `render()` no callback `process` do nosso stream (`F32LE`, estéreo, 48 kHz); one-pole por voz para o abafado.
- **Zero CPU em silêncio**: 200 ms sem voz ⇒ o stream é desconectado; reconecta na próxima deixa.
- Volume geral `[sound] volume`, padrão 0,5; tudo desligado por padrão (`[sound] enabled = false`), como no plano.

### 4.8 v1 só atenuação; o filtro depois

- **v1 (fase 3)**: só `channelVolumes`. Os −24 dB já dão "do outro lado da parede" ao ouvido; zero topologia
  nova no PipeWire.
- **Depois, se o uso pedir** (fase 6, opcional): um `bq_lowpass` único no `filter-chain`
  (`~/.config/pipewire/filter-chain.conf.d/nkw-muffle.conf`, Freq 800 Hz, Q 0,707), com links dinâmicos
  fonte→filtro→sink enquanto houver porta fechada no caminho. O `lowpass_hz` do modelo já sai pronto para isso.

### 4.9 Conforto e acessibilidade

- `mono = true` (`--reduced-motion` também liga) tira o pan.
- `[audio] proximity = false` desliga o modelo inteiro (os streams ficam como estão).
- Ganhos nunca passam de `own_volume`: o mundo só abaixa, nunca aumenta o volume de ninguém.

## 5. O cofre

### 5.1 O objeto

Um cofre de parede, de ferro pastel, na biblioteca, ao lado do balcão da **bibliotecária** (é ela quem guarda a
chave). Tem um mostrador giratório, uma alavanca e uma plaquinha "cofre". Trancado, é só decoração: os rótulos
de SALTO o mostram, o raio de leitura diz "trancado · 7 pastas". Destrancado, a porta abre e dentro há cartões
com **nomes** (uma estante pequena), e um selo "cofre aberto" aparece no HUD até trancar.

O que vai para o cofre (`nkw_objects::vault::Policy`), relativo às raízes da biblioteca; a lista padrão **só
cresce** via config, nunca encolhe:

- pastas: `.ssh`, `.gnupg`, `.password-store`, `.local/share/keyrings`, `.config/gcloud`, `.aws`, `.kube`,
  `.docker/config.json`;
- padrões: `**/.env`, `**/.env.*`, `**/*.pem`, `**/id_*` (chaves fora do `.ssh`).

`partition(policy, entries)` separa estante e cofre de forma disjunta e completa: uma entrada nunca está nos dois,
nunca em nenhum.

### 5.2 Modelo de ameaça, com honestidade

O `nkw` roda como o **mesmo usuário Unix** que já lê `~/.ssh`. Não há fronteira de privilégio: o cofre não
protege contra um atacante local com a sua sessão (nada no mesmo processo protegeria). Ele protege contra
**exposição acidental**: um screenshot, uma gravação de tela, um agente que recebe um caminho sem querer, uma
estante que fica aberta enquanto alguém olha por cima do ombro, uma pata de gato no teclado. Por isso o
mecanismo é **atrito deliberado + janela curta de exposição + nunca sair do processo**, e não PAM/polkit (que
checariam identidade, que já é sua, ao custo de um helper privilegiado ou de um agent polkit na sessão).

`pkcheck`/PAM ficam como **tier opcional** documentado (`[vault] reauth = "polkit"`) para máquinas
compartilhadas, fora do padrão e fora desta onda.

### 5.3 Destrancar: a frase, pela bibliotecária

- Em frente ao cofre, `t` (ou `interact`) abre a **CONVERSA com a bibliotecária** (`Kind::Talk { purpose:
  VaultUnlock }`): "Esse é o cofre. Se é você mesmo, diga a palavra." A linha de texto é a mesma da conversa;
  `enter` envia a frase ao `Vault::apply(Attempt)`, **não ao core**.
- Frase padrão `"destrancar"` (`[vault] phrase`, mínimo 6 caracteres; a comparação é por igualdade exata, sem
  trim de acentos). O texto digitado **não é ecoado** na caixa (aparecem pontos), não vai para o histórico da
  linha, não vai para logs.
- Errou: "Hmm, não foi isso." Cooldown de 2 s entre tentativas; 5 erros ⇒ lockout de 60 s ("Volte daqui a
  pouco."). Cada erro vira linha de auditoria.
- Acertou: `VaultDial` (mostrador gira, 420 ms) + `VaultUnlock` (clique grave), a porta abre, a bibliotecária
  diz "Pronto. Eu fecho daqui a pouco." e a conversa termina sozinha (volta ao modo de origem).

### 5.4 Auto-trancar (`LockReason`)

| gatilho | razão | nota |
|---|---|---|
| 20 s sem interação dentro do cofre (`[vault] auto_lock_after_s`) | `Timeout` | `Touch` renova (ler um cartão, `y`) |
| sair da sala da biblioteca | `LeftRoom` | o mesmo evento que já troca o culling |
| o `nkw` perde o foco de teclado do host (`alt+tab` no niri) | `FocusLost` | padrão KeePassXC "lock on minimize" |
| entrar em FOCO em qualquer tela | `EnteredFocus` | uma tela pode ser compartilhada; o cofre não fica aberto atrás dela |
| pedido de captura servido pelo `nkw` (fase 7) | `Capture` | tranca **antes** de responder o quadro |
| `--shot` / `shot` de script | `Shot` | screenshots nunca mostram o cofre aberto, nem no demo |
| `world.vault_lock` (paleta) ou `interact` no cofre aberto | `Manual` | — |

Trancar: a porta fecha (320 ms) + `VaultLock` (thunk); o índice do cofre é **descartado da memória** (a próxima
abertura lê de novo: `readdir` + `stat`, nunca conteúdo). Captura "de fora" (a tela do niri inteira sendo
compartilhada) **não é detectável** pelo `nkw` aninhado: a defesa é a janela curta, documentada como limitação.

### 5.5 Só nomes

Dentro do cofre, um cartão por entrada: nome, tipo (pasta/arquivo), tamanho, data. **Nunca** conteúdo:

| tecla | na estante comum | no cofre |
|---|---|---|
| `enter` | abre (`xdg-open`) / entra na pasta | entra na pasta (só nomes); em arquivo: nada ("é só a lista") |
| `e` | `$EDITOR` numa tela nova | igual: o editor é outro processo, o `nkw` não lê os bytes |
| `p` | preview no atril | **desabilitado** |
| `y` | copia o caminho | copia o caminho (é a única "saída", e é um caminho, não conteúdo) |
| `g` | entrega o livro a um gato | **desabilitado, fixo** |
| `/` | busca fuzzy | **o cofre nunca entra na busca** |

### 5.6 Nunca para agentes nem para o core

Regra fixa, não configurável, mais forte que o resto da biblioteca (`plano-mundo.md` §7.5):

- caminhos do cofre **nunca** entram em `POST /v1/panes … reply`, em `nk.ui.scene`, em `world.open`, em
  intenções de colocação, em `world.json` (memória de lugar e regras), em `world.dump()`, em `events.jsonl`;
- `world.open {path}` vindo do Lua **recusa** caminhos do cofre (defesa em profundidade: o `init.lua` é do
  usuário, mas um agente pode sugerir código);
- a paleta "ir para" não lista o cofre nem o seu conteúdo;
- o teste `vault_never_leaves` passa um `FsPort` falso com entradas do cofre por todos os caminhos de saída e
  verifica que nenhum string do cofre aparece em nenhuma mensagem, arquivo ou dump.

### 5.7 Log de auditoria

`~/.local/state/nekuroddo/vault.log`, criado `0600` (o diretório já é `0700`), texto simples, uma linha por
evento, `append` + `fsync`:

```
2026-10-21T20:14:03-03:00 unlock
2026-10-21T20:14:25-03:00 lock_timeout
2026-10-21T20:30:11-03:00 unlock_denied wrong_phrase fails=1
2026-10-21T20:31:40-03:00 lock_focus_lost
```

Nunca a frase digitada, nunca nomes de arquivo. A máquina pura emite `AuditLine`s; o `nkw` só escreve. Rotação:
acima de 256 KB vira `vault.log.1` (um só).

### 5.8 Configuração

```toml
[vault]
phrase = "destrancar"      # mínimo 6 caracteres
auto_lock_after_s = 20
cooldown_s = 2
max_fails = 5
lockout_s = 60
reauth = "phrase"          # "phrase" | "polkit" (opcional, fase 6+)
extra_roots = [".config/acme-secrets"]   # só cresce
extra_globs = ["**/*.key"]
```

### 5.9 Testes

- **Máquina pura** (`nkw-objects::vault`, tabela): frase errada/certa, cooldown, lockout, cada `LockReason`,
  `Touch` renova, `next_wake` = instante do timeout, lock quando já trancado é no-op sem auditoria duplicada.
- **Política**: raízes e globs padrão; `grow` só adiciona; `partition` disjunta e completa; paths normalizados
  (`a/../.ssh` conta como `.ssh`).
- **Integração no `nkw`** (`FsPort` falso): entradas do cofre nunca aparecem na estante, na busca, no dump, no
  `world.json`, em nenhuma chamada ao core; `--shot` com o cofre aberto produz o cofre fechado; `--demo` tem um
  cofre com 3 cartões de ficção (`id_demo`, `config`, `known_hosts`) e nunca lê a home.
- **Auditoria**: arquivo `0600`, uma linha por evento, sem a frase, rotação.
- **FEEL**: `VaultDial` dura 420 ms e emite um cue só; com movimento reduzido vira corte e o som fica.

### 5.10 FEEL do cofre

Abrir um cofre tem que ser gostoso e raro: o mostrador gira com um "tic-tic-tic" (3 cliques), a alavanca
baixa, a porta abre em 420 ms com um assentamento de 60 ms, os cartões entram em cascata (40 ms de atraso cada,
máximo 8 animados). Trancar é seco: 320 ms e um thunk. Sem partículas, sem brilho. Com movimento reduzido:
cortes, e os sons ficam. Desempenho: o `readdir` do cofre só acontece ao destrancar (≤ 20 ms para 1k entradas,
como a biblioteca); nada é mantido na memória depois de trancar.

## 6. Fases e achados de toolchain adotados

### 6.1 Atualização do roteiro (`plano-mundo.md` §10)

| fase | ganha | continua |
|---|---|---|
| **0 — spike** | os crates `nkw-objects` e `nkw-sys` nascem em paralelo com o spike (são puros/isolados e não dependem do Smithay); `nkw-gl` fica atrás de uma API fechada (`WorldRenderer` + `DrawList`) | os 7 critérios de seguir/parar não mudam |
| **1 — WM aninhado** | bandeirinha do layout de teclado (HUD) | tudo |
| **2 — gatos** | **vitrola** (MPRIS + volume), relógio de parede + luz, bateria, quadro de avisos com custo; o `nkw-sys` entra no laço; `--demo` com fixture | tudo |
| **3 — ANDAR** | **portas com estado** (colisão, culling, throttle), **áudio por proximidade v1** (só atenuação), passos, mixer de sons próprios, gatos reagindo | corpus de colisão ganha "porta fechada bloqueia", "porta espera se você está no vão" |
| **4 — biblioteca** | **o cofre** inteiro (§5), a bibliotecária como guardiã | §7 do plano |
| **5 — aconchego** | lareira, aquário, planta-antena, as animações ambientes com os tetos de §3.8 | — |
| **6 — endurecimento** | bandeja (`rustsni`), caixa de encomendas, rádio bluetooth, dimmer, calendário, notificações best-effort, filtro passa-baixa de verdade (opcional), `reauth = "polkit"` (opcional) | — |
| **7 — sessão própria** | ON AIR de verdade, ocioso/travado vira noite, notificações próprias | — |

Aceites novos: fase 2 — S1 com a vitrola tocando e fora da vista ≤ 0,2 % e ≤ 2 wakeups/s; fase 3 — fechar uma
porta com música na outra sala baixa 24 dB em ≤ 170 ms (50 ms + τ) e a sala some do conjunto de culling no
mesmo quadro; fase 4 — `vault_never_leaves` verde e screenshot com cofre aberto impossível.

### 6.2 Achados de toolchain adotados (de `pesquisa-toolchain`)

| achado | decisão |
|---|---|
| crates.io `smithay 0.7.0` está atrás do master (breaking changes "Unreleased") | **pinar rev de git** (`24647fc8…`, 2026-10-19); re-pinar só em PR próprio |
| `GlowRenderer` + `winit::init::<GlowRenderer>()` + `with_context` dão o `Arc<glow::Context>` de fábrica | **D3 simplifica**: nenhum glow manual; o plano B do D3 sai do caminho crítico |
| `glow` do Smithay é `0.16`; crates 0.x não unificam entre minors | `nkw-gl` usa `glow = "0.16"` e o re-exporta para o `nkw` |
| `smallvil` é o exemplo do escopo da fase 0 (winit + wayland_frontend + desktop) | o `nkw` nasce da forma do `smallvil`, não do `anvil` |
| incremental sob `lto = "thin"` mediu 54,5 s; perfil `dev` 1,7 s | `[profile.world]` só para release/CI/perf; o loop de dev usa `dev` |
| build release a frio do mínimo: 5 min 03 s; binário 6,7 MiB | dentro dos critérios da fase 0, com folga para `nkw-core`/`nkw-gl`/`nkw-sys` |
| `EGL_MESA_platform_surfaceless` disponível | backend headless e goldens do `nkw-gl` por EGL surfaceless, sem `/dev/dri` |
| `wayland-protocols` (.pc) faltava; foot/mpv/chromium/gtk4-demo/wev faltavam | instalar antes da fase 0 (`sudo`, pelo usuário) |
| `zbus` 6 trocou o executor e removeu o módulo `blocking` | **`zbus = "5"` + `async-io 2.6`**, como o niri; revisitar quando o zruntime amadurecer |
| `pipewire.sec.pid` aponta para o `pipewire-pulse` na maioria dos apps | âncora por `application.process.id` + subida de `ppid`; fallback por `app_id` |
| `cpal`/`rodio` seriam uma segunda pilha de áudio | sons próprios pelo stream do próprio `pipewire-rs` |
| `rustsni` é Rust puro e dirigido pelo chamador | bandeja por `rustsni` (fase 6), fd direto no `calloop` |
| PSI é o único sinal de CPU/RAM por evento | lareira/aquário por PSI; número exato só de frente, ≤ 1 Hz |

Correção à pesquisa de widgets: ela sugeria `libpulse-binding` para o volume; a pesquisa de áudio, feita
depois com dados reais, mostrou que a **mesma conexão `pipewire-rs`** serve volume do sink, volume por stream e
os sons próprios. Fica uma pilha de áudio só.

## 7. Riscos

| risco | mitigação |
|---|---|
| D-Bus ausente (sessão mínima) ou sem NM/UPower | cada adaptador falha sozinho (`SysEvent::Error`), o objeto fica "sem dado", o resto roda |
| `pipewire-rs` precisa de `libpipewire` + `libclang` no build | feature `pipewire` (padrão ligada) e build sem ela continua verde |
| ancorar stream à janela errada (sandbox, proxies distintos) | fallback por `app_id`; se não achar, o stream fica sem âncora (não mexe) |
| abaixar o volume de um app e não devolver (crash) | tabela de restauração + hook de panic devolve `[1, 1]`; no pior caso o usuário mexe no `wpctl` |
| o cofre dar falsa sensação de segurança | §5.2 escrito na doc do usuário; o selo "cofre aberto" sempre visível; janela curta |
| objetos virando enfeite caro (animação sempre rodando) | `want` por tier com teste; teto de 4 `Rate`; S1 medido com os objetos |
| SSID/nomes de dispositivos vazando em logs/screens | eventos sem esses campos por tipo; fixture no demo; regra de review |
| notificações: dois donos do nome | best-effort + "modo completo" documentado; nunca `REPLACE_EXISTING` |
| mais um crate com D-Bus (`rustbus` do `rustsni`) | aceito na fase 6; se pesar, portar para `zbus` usando o `rustsni` (MIT) como referência |

## 8. Perguntas abertas

1. **Onde fica a vitrola**: Recepção (som da casa) ou "minha sala"? Padrão proposto: Recepção.
2. **Lareira/aquário por sala ou um só**: proposto um por sala, com a carga da máquina; vale a pena?
3. **Frase do cofre**: uma frase fixa configurável basta, ou você quer o tier polkit já na fase 4?
4. **Auto-trancar ao entrar no FOCO**: é o mais seguro, mas pode irritar quem abre o cofre para copiar um
   caminho e colar num terminal. Alternativa: trancar só se a tela em FOCO tiver `app_id` de navegador/chat.
5. **Passos ligados por padrão** quando o som estiver ligado, ou opt-in separado?
