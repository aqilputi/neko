---
type: Guide
title: O mundo (nkw)
description: Guia do nkw, o compositor Wayland 3D aconchegante do nekuroddo — suas janelas em mesas e salas, os agentes como gatos aldeões; como instalar e rodar aninhado, os modos e as teclas, o HUD e o telefone, o ANDAR, carregar telas, a recepção e seus objetos, a biblioteca e o cofre, os apps de verdade lá dentro (compatibilidade, X11, portais, IME), o world.toml, Lua, privacidade, desempenho e solução de problemas, com o que ainda está planejado marcado.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/office/crates/nkw
tags: [mundo, nkw, compositor, wayland, smithay, gles, gatos, biblioteca, cofre, telefone, hud, xwayland, guia]
timestamp: 2026-10-23T18:00:00Z
status: draft
---

# O mundo (`nkw`)

O `nkw` é um **compositor Wayland em 3D**, aconchegante, no estilo de um jogo de vila de bichinhos: suas
janelas (terminais, navegador, apps GTK/Qt) viram telas em mesas, as mesas ficam em salas, e cada sessão do
Claude Code é um **gato aldeão** sentado de costas para você, com o trabalho dele rodando no monitor. Você
desliza de mesa em mesa ou anda pelo escritório em primeira pessoa, conversa com os gatos e, para ler e
digitar, entra numa tela em **1:1**: nítida, pixel a pixel, como num compositor comum.

Ele roda **aninhado**: é uma janela dentro da sua sessão (niri, sway, KDE, GNOME…), e os apps que você abre
lá dentro usam o `WAYLAND_DISPLAY` do mundo. Nada de domínio mora no `nkw`: os gatos vêm do mesmo
[core](arquitetura.md) pelo mesmo [protocolo](protocolo.md), e as teclas, do mesmo `/v1/keymap` que a TUI e o
[`nkg`](gui.md) usam — inclusive o que você religou em [Lua](lua.md).

## O que já funciona e o que ainda vem

O mundo é construído em fases ([plano-mundo.md](plano-mundo.md) §10). Este guia descreve o que existe no
código. O que é plano mais adiante está marcado **(planejado)**.

| parte | estado |
|---|---|
| Gerenciador de janelas aninhado: salas, corredor, mesas com pilhas, larguras, telas no ar, tela cheia como "TV", popups, diálogos, área de transferência com o host, escala fracionária, persistência (`world.json`), regras por `app_id`, lançador de apps | funciona |
| Modos NAV, FOCO (1:1), CONVERSA, SALTO, COMANDOS (paleta) e o mapa | funciona |
| Gatos: um por sessão, poses por estado, monitor com o tail ou o terminal, filhotes (subagents), a inbox vindo até você, carteiros, a recepcionista, a bibliotecária, o quadro de avisos; na CONVERSA, responder, escolher, parar e remover | funciona |
| A recepção e seus objetos: vitrola (MPRIS + volume), relógio de parede, pote de óleo da bateria, planta-antena; portas com estado; sons próprios e som por proximidade v1 (só atenuação) | funciona |
| ANDAR pelo teclado e pelo mouse, mira em objetos, telas, gatos e portas, carregar telas, vinheta de conforto, `[walk]`/`[camera]` no `world.toml` | funciona |
| Biblioteca de arquivos (estantes, livros, busca, abrir, prévia no atril) e o cofre com a bibliotecária e o log de auditoria | funciona |
| Teclas Lua ligadas a uma função rodando no mundo (`lua.key:…`) | funciona |
| Passe de arte: teto, janelas com a luz da hora, padrões de chão e parede, gatos andando de quatro | funciona |
| HUD com a faixa de status, o **telefone** (Gatos, Inbox, Mapa, Apps, Janelas, Música, Biblioteca, Cofre, Ajustes, Ajuda), o tour da recepcionista, alto contraste e escala de fonte | funciona |
| Os comandos de sessão da TUI por tecla no NAV (`r`, `x`, `n`, `m`, `X`…) sobre o gato da estação | funciona |
| Apps de verdade: ambiente certo para cada toolkit, X11 pelo `xwayland-satellite`, perfis isolados para navegador, formas de cursor e ponteiro travado no FOCO, seleção primária | funciona |
| A confirmação do `world+shift+e` | (planejado) |
| Método de entrada (IME), imagens de cursor próprias dos apps, arrastar entre janelas, `layer-shell`, janelas de um agente na mesa do gato dele, entregar um livro a um gato, `[sound]` | (planejado, fase 6) |
| Sessão própria (DRM/KMS, `super` como tecla do mundo) | (planejado, fase 7, opcional) |

## Instalar

```sh
make install          # o core (nk): o nkw sobe o próprio `nk serve`, como o nkg
make install-world    # compila o nkw (perfil world: release + LTO) e instala
                      # ~/.local/bin/nkw + o atalho "nekuroddo — mundo" no lançador
```

Pré-requisitos de compilação em [instalacao.md](instalacao.md#mundo-nkw), com o `xwayland-satellite` opcional
(para apps X11). `make install-world PREFIX=…` muda o destino; `make -n install-world` mostra os passos sem
rodar nada. O atalho abre o `nkw` aninhado na sua sessão.

## Rodar

```sh
nkw --demo            # o mundo de demonstração: gatos, objetos e lançador de fixture; não lê sua home
nkw                   # suas sessões de verdade
nkw -- foot           # já abre um terminal lá dentro (qualquer comando depois de --)
```

Dentro do mundo, os apps encontram o compositor pelo socket `$XDG_RUNTIME_DIR/nkw-0` (`WAYLAND_DISPLAY=nkw-0`;
sem `XDG_RUNTIME_DIR` o `nkw` não sobe). Tudo o que ele lança recebe esse `WAYLAND_DISPLAY`, as variáveis que
fazem cada toolkit preferir Wayland ([o ambiente do lançador](#o-ambiente-do-lançador)) e um `DISPLAY` só
quando o X11 do mundo está de pé ([X11](#x11-pelo-xwayland-satellite)); tudo é encerrado quando o mundo fecha.
`T` (ou `world+enter`) abre um terminal novo (`foot`, ou o de `[launch] terminal`); `world+d` abre a paleta nos
apps instalados (os `.desktop` de `XDG_DATA_HOME` e `XDG_DATA_DIRS`, respeitando `OnlyShowIn`), e o app **Apps**
do telefone mostra os mesmos.

Sem `--core`, o `nkw` sobe o **próprio** `nk serve` (o `nk` do `PATH`, ou `~/.local/bin/nk`) com um diretório
de runtime privado, e o encerra ao sair — como o `nkg`. Ele não se pendura num `nk serve` que já esteja
rodando, a menos que você passe o endereço. Sem core (não há `nk`, ou ele caiu), o mundo continua sem gatos e
avisa com um toast.

| flag | o quê |
|---|---|
| `--demo` | o mundo de demonstração: gatos e objetos de fixture, nenhum estado salvo, nenhuma leitura da home, do `world.toml` ou do `world.json` |
| `--core auto\|none\|demo\|URL` | de onde vêm os gatos: `auto` (padrão) sobe um `nk serve` próprio; `demo` sobe `nk serve --demo`; `none`, nenhum core (é o padrão do `--demo`, que traz os gatos de fixture); `tcp://127.0.0.1:PORTA` (token de `$NEKO_TOKEN` ou do arquivo de token do core) ou `unix:///caminho` usa um core que já está rodando |
| `--world-mod ctrl+alt\|super` | o que é a tecla do mundo (`world+`); ganha do `world.toml` |
| `--focus-escape TECLA` | a saída do FOCO (padrão `ctrl+]`); ganha do `world.toml` |
| `--mouse-look on\|off` | olhar com o mouse no ANDAR; ganha de `[walk] mouse_look` |
| `--xwayland auto\|on\|off` | o X11 pelo `xwayland-satellite`; ganha de `[launch] xwayland` (veja [X11](#x11-pelo-xwayland-satellite)) |
| `--text-input` | oferece o `text-input-v3` aos apps, só para avaliação: não há IME ligado (veja [IME](#método-de-entrada-ime)) |
| `--reduced-motion` (ou `NKW_REDUCED_MOTION=1`) | movimento reduzido (veja [Conforto](#conforto-e-movimento-reduzido)) |
| `--preset64` | o visual "64": baixa resolução, filtro de 3 pontos e dither |
| `--state-dir DIR` | onde guardar o `world.json`, o `ui.toml` e os perfis isolados (e ler o `world.toml`) em vez do padrão |
| `--socket NOME` | o nome do socket Wayland (padrão `nkw-0`) |
| `--no-sys` | sem os adaptadores de sistema (D-Bus, PipeWire): sem som e com os objetos parados |
| `--size LxA` | o tamanho da janela (padrão 1280×800 aninhado, 1280×720 sem janela) |
| `--scale S` | a escala da saída, de 0,5 a 4 (padrão: a do host; 1 sem janela) |
| `--seconds N` | sai depois de N segundos |
| `--headless`, `--script ARQ\|-`, `--shot ARQ.png`, `--gl-debug` | para testes e verificação: sem janela, entrada roteirizada, captura, erros de GL contados (veja [Desenvolvimento](#desenvolvimento)) |

## A primeira vez, aninhado

O mundo abre como uma janela comum da sua sessão. Dê a ela a tela inteira (no niri, `Mod+F` maximiza a
coluna; no sway, `Mod+f`; no KDE ou no GNOME, maximize a janela) e:

1. Você começa na **recepção**, de pé diante da mesa da gata recepcionista, com o relógio, a vitrola e o
   quadro de avisos. O canto de cima à esquerda diz o modo: `NAVEGAR`; o de cima à direita, a hora.
2. Na primeira vez, a recepcionista faz um **tour** de seis balões: andar (`h` `l` `J` `K`), o FOCO (`enter` e
   `ctrl+]`), conversar (`t`), o ANDAR (`w`), o telefone (`tab`) e os comandos (`:`). Cada balão tem "próximo" e
   "pular"; no fim, o mundo anota que você já viu (no `world.json`) e não repete. `world.tour`, pela paleta,
   faz de novo: a câmera volta à recepção e ela recomeça. O `--demo` nunca abre o tour sozinho.
3. `J`/`K` levam de sala em sala pelo corredor: uma sala por contexto do core, mais "minha sala" para as
   janelas que você abrir fora de um contexto.
4. `h`/`l` deslizam de mesa em mesa. Numa mesa com gato, `t` conversa com ele; `enter` senta no terminal dele.
5. `T` abre um terminal numa mesa nova à direita; `enter` entra nele em 1:1 (**FOCO**) e `ctrl+]` sai.
6. `tab` abre o **telefone**; `:` abre os **COMANDOS** (a paleta). `world+shift+e` sai do mundo e fecha as
   janelas que estão lá dentro (hoje sai direto; a confirmação é **planejada**).

**A tecla do mundo (`world`).** Aninhado, `world` é **`ctrl+alt`**: o compositor de fora consome quase todo
`super`+letra antes de o `nkw` ver, e `alt` sozinho é o meta dos terminais. Todo acorde `world+…` é global —
funciona em todo modo, inclusive no FOCO. Se a sua sessão usa algum `ctrl+alt+letra` (alguns ambientes ligam
`ctrl+alt+t` ao terminal), religue o de fora ou passe `--world-mod super` quando o compositor de fora deixar
`super` chegar às janelas.

O layout do teclado vem das variáveis `XKB_DEFAULT_LAYOUT`, `XKB_DEFAULT_VARIANT` e `XKB_DEFAULT_OPTIONS` do
ambiente em que o `nkw` sobe (repetição: 200 ms, 25 por segundo).

## Modos

| modo | rótulo no HUD | como entra | como sai |
|---|---|---|---|
| **NAV** | `NAVEGAR` | é o modo de base | — |
| **ANDAR** | `ANDAR` | `w` / `world+w` | `esc` / `world+w` |
| **FOCO** | `FOCO` | `enter` / `i` numa tela (na mesa de um gato: o terminal dele) | `ctrl+]`, ou um clique no mundo escurecido em volta |
| **CONVERSA** | `CONVERSA` | `t` (o gato da estação), `shift+space` / `world+space` (o mais urgente) | `esc` / `q` |
| **SALTO** | `SALTO` | `f` / `world+f` (`F`: direto para o FOCO) | digitar o rótulo, ou `esc` |
| **COMANDOS** | `COMANDOS` | `:` / `ctrl+p` / `world+p` (`world+d`: nos apps) | `enter` / `esc` |
| **telefone** | `TELEFONE` | `tab` (NAV, ANDAR), `world+n` (qualquer modo, inclusive o FOCO), `?` (no app Ajuda) | `esc` / `tab`; volta ao modo de onde veio |
| mapa | — | `o` / `world+o` (de novo: o andar inteiro) | `esc` |
| estante | — | diante de uma estante da biblioteca | `esc` |

Nada rouba o foco: um gato que chega, uma confirmação ou um pedido de atenção de um app viram um **selo** no
HUD, e você decide quando atender. Uma tecla que já estava apertada quando você muda de modo só conta depois de
solta (o `w` que entrou no ANDAR não sai andando). Todas as teclas abaixo são comandos do core e podem ser
religadas em Lua; a referência completa está em [teclas.md](teclas.md#mundo-nkw) e
[comandos.md](comandos.md#mundo--worldgo). Enquanto o keymap do core não chega (ou sem core), valem as mesmas
teclas, de uma tabela interna. A lista viva, com o que você religou, está no app **Ajuda** do telefone (`?`).

### NAV: de estação em estação

Uma **estação** é um lugar onde a câmera para: diante de uma mesa, de uma tela no ar, de uma porta, do quadro
de avisos, de um objeto, ou onde você saiu do ANDAR.

| tecla | o quê |
|---|---|
| `h` `l` (setas) | estação anterior / próxima (travelling lateral, sem girar) |
| `j` `k` (setas) | tela de baixo / de cima na pilha da mesa |
| `J` `K` · `1`…`9` · `gg` `G` | próxima / anterior sala · sala N · primeira / última mesa |
| `enter` `i` · `t` · `w` | FOCO · CONVERSA com o gato · ANDAR |
| `f` · `F` · `o` · `b` | SALTO · SALTO direto para o FOCO · mapa · biblioteca |
| `H` `L` · `,` `.` · `-` `=` | tela para a mesa ao lado · junta na pilha da esquerda / tira para uma mesa só dela · mesa mais estreita / mais larga |
| `p` · `M` · `T` · `Q` | prende a tela no ar · manda para outra sala · terminal novo · fecha a janela |
| `shift+space` · `:` | chama o gato mais urgente · COMANDOS |
| `tab` · `?` · `z` | telefone · ajuda · esconde/mostra o HUD |

**Os comandos de sessão sobre o gato da estação.** O NAV herda as teclas de sessão da TUI e as manda ao core
com o gato da mesa em que você está: `r` responde, `x` para, `n` renomeia, `m` muda de contexto, `X` arquiva,
`dd` remove, `c` cria uma sessão nova, `a`/`A` vão ao próximo / anterior que espera você e `ctrl+k` abre o
telefone no app Gatos. Um comando que pede um texto (o nome novo, o contexto) abre os COMANDOS já na pergunta.
Numa mesa sem gato, um toast avisa "nenhum gato nesta mesa". O mundo nunca liga o `q` da TUI: sair é
`world+shift+e`.

### ANDAR: livre, em primeira pessoa

| tecla | o quê |
|---|---|
| `w` `s` `a` `d` (segurar) | andar para a frente / para trás / de lado |
| `shift` (segurar) | correr |
| `q` `e` (e `left` `right`) | girar |
| `up` `down` · `c` | olhar para cima / para baixo · voltar ao horizonte |
| `enter` `space` | usar o que está na mira: um objeto, uma porta, uma tela (FOCO), um gato (CONVERSA) |
| `j` `k` · `h` `l` | ajustar o objeto na mira (o volume da vitrola) · anterior / próximo nele (a faixa) |
| `g` · `[` `]` · `-` `=` | pegar / soltar a tela · mais perto / mais longe · tamanho |
| `f` · `tab` · `esc` | SALTO · telefone · volta ao NAV |

No ANDAR, o botão esquerdo do mouse é `interact` e o direito, `grab`. A **mira** é um ponto no centro da tela
que vira anel quando há algo à frente (até 4 m, num cone de 8°). Você colide com paredes, móveis, gatos e
portas fechadas e escorrega rente a eles; não há pulo nem escada (o mundo é plano). Os padrões (2,8 m/s,
correndo ×2, giro de 110°/s, olhar até ±55°, olho a 1,5 m) mudam no `[walk]`/`[camera]` do `world.toml`, e
algumas opções também na hora, pelo app Ajustes do telefone (sem salvar).

**Ao sair**: se há uma estação a menos de 1,5 m e quase na mesma direção, a câmera desliza até ela; senão você
fica onde está, numa estação "você está aqui", e `h`/`l` seguem para a estação mais próxima de cada lado. É a
regra `auto`; `[walk] on_exit = "snap"` sempre encaixa na mais próxima e `"stay"` nunca encaixa.

**O mouse no ANDAR.** Com `[walk] mouse_look = true` (padrão) ou `nkw --mouse-look on`, entrar no ANDAR trava e
esconde o ponteiro do host e o movimento do mouse gira a câmera (0,12° por contagem, sem aceleração;
`[walk] mouse_deg_per_count`). Sair do ANDAR — ou a janela do mundo perder o foco (um `alt+tab` lá fora) —
devolve o ponteiro. Olhar pelo teclado funciona sempre; se o host não entregar movimento relativo, é o único
jeito. O movimento relativo vem de um ajuste pequeno no backend winit do Smithay, mantido em
`gui/office/patches/`.

**Carregar uma tela.** `g` com uma tela na mira a solta da mesa: ela flutua 1,2 m à sua frente, sempre virada
para você, e uma dica "g solta · [ ] distância · - = tamanho" aparece embaixo. `[` `]` aproximam e afastam
(0,8 a 3 m, de 20 em 20 cm), `-` `=` mudam a largura. `g` de novo solta, na primeira regra que servir:

1. uma mesa a menos de 0,6 m: a tela entra na pilha dela;
2. uma parede a menos de 0,4 m e com até 15° de desvio: a tela encosta nela (a 5 cm, presa a uma grade de 25 cm);
3. senão, fica no ar onde está, e vira uma estação.

### FOCO: a janela em 1:1

A câmera desliza até a pose exata em que a tela ocupa os pixels dela, e a janela passa a ser desenhada em 2D,
nítida, também em escala fracionária. O teclado vai todo para a janela — só `ctrl+]` (`[keys] focus_escape`) e
os acordes `world+` ficam com o mundo. Em volta, o mundo congela e escurece. Popups e menus só existem no FOCO.
`world+h`/`l`/`j`/`k` passam o FOCO para a tela vizinha sem sair; `world+space` atende o gato mais urgente;
`world+n` abre o telefone por cima (e `esc` volta ao FOCO); `world+w` sai andando dali.

No FOCO, o **cursor do host segue o app**: a mão num link, o I num campo de texto, as setas de redimensionar
(o app pede pelo `cursor-shape`; um app que desenha a própria imagem de cursor fica com a seta padrão, por ora).
Um jogo ou app que **trava o ponteiro** (`pointer-constraints` + `relative-pointer`) o recebe travado e
escondido, com o movimento relativo do mouse; `ctrl+]`, a perda de foco ou a troca de modo devolvem o ponteiro
(aninhado, "confinar" vale como "travar"). Fora do FOCO, o cursor volta ao padrão.

### CONVERSA: com o gato à sua frente

O gato vira na cadeira e uma caixa creme aparece embaixo, com a plaquinha do nome e o texto letra a letra
(`space`/`enter` mostra tudo de uma vez; quando há mais, um ▼ pisca). Quando a sessão fez uma pergunta, as
opções aparecem numa caixinha à direita: `j`/`k` ou `1`…`9`, e `enter`. `i`/`r` abrem a linha de texto para
responder (`enter` envia, `shift+enter` quebra a linha, `esc` cancela); `f` senta no terminal do gato;
`esc`/`q` encerram. Parar e remover o gato saem pelas escolhas da conversa, e as confirmações do core chegam
como uma pergunta na caixa.

### SALTO, COMANDOS e o mapa

- **SALTO** (`f`): rótulos de uma ou duas letras (`asdfghjkl`) em tudo o que dá para usar à vista — telas,
  gatos, filhotes, portas, objetos. Digitar o rótulo leva até lá; `F` vai direto ao FOCO; `backspace` apaga,
  `esc` cancela.
- **COMANDOS** (`:`): uma linha com lista fuzzy que junta os comandos do mundo, os apps instalados e "ir para"
  uma sala, um gato, uma tela ou um objeto (`world.goto_object`). `tab` completa, `ctrl+n`/`ctrl+p` (ou as setas)
  navegam, `ctrl+a`/`e`/`w`/`u` editam.
- **Mapa** (`o`): uma vista de cima da sala, sem voo de câmera; `o` de novo mostra o andar inteiro. `esc` volta.

## O HUD

O mundo mostra o que importa nos próprios objetos (o relógio, o pote de óleo, a planta, o quadro de avisos) e
deixa o HUD como selo, pequeno, nos cantos:

- **o rótulo do modo**, em cima à esquerda (`NAVEGAR`, `ANDAR`, `FOCO`, `TELEFONE`…), com os **selos** embaixo
  dele: a inbox com o número de gatos esperando, a bateria baixa, o cofre aberto, um app pedindo atenção;
- **a faixa de status**, em cima à direita, uma pílula creme da mesma altura do rótulo;
- **os toasts**, com um ícone pelo tipo (aviso ou erro), até três empilhados.

A faixa tem três jeitos (`[hud] strip` no `world.toml`, ou o app Ajustes):

| `strip` | o que mostra |
|---|---|
| `"auto"` (padrão) | a hora; o envelope com o número da inbox, se alguém espera você; o custo do dia (`$ x,yy`), se houver; uma nota musical, se a vitrola toca; a bateria, só quando está descarregando e abaixo de 30 % |
| `"always"` | tudo o que o mundo sabe: hora, inbox, gatos ativos e bloqueados, custo, uso de 5 h, bateria, rede, música, cofre, X11 |
| `"never"` | nada (o rótulo e os selos continuam) |

`z` (no NAV) esconde e mostra de uma vez o rótulo, a faixa e os selos, para um mundo sem HUD nenhum. Os
crachás com o nome dos gatos ficam acima da cabeça de cada um, nunca atravessam paredes (um gato na sala ao lado
não aparece pela parede) e nunca se sobrepõem: o mais perto fica no lugar, e o mundo mostra no máximo 12. Um só
visual de seleção — fundo creme e um anel — marca o que está escolhido em toda parte: a tela na mira, o livro, o
item do telefone, a linha da paleta, o mapa.

## O telefone

`tab` (no NAV e no ANDAR) ou `world+n` (em qualquer modo, inclusive no FOCO) abre o **telefone**: um painel no
meio da tela, por cima do mundo, com o status no topo (a hora grande, a bateria, a rede, a inbox e o custo do
dia) e uma grade de apps. Ele se abre por cima dos COMANDOS, então volta exatamente ao modo de onde você veio.

| tecla | o quê |
|---|---|
| `h` `l` `j` `k` (e setas) | anda pela grade (três colunas) e pelas listas |
| `enter` `space` | abre o app ou o item |
| `backspace` | volta: da sub-página ao app, do app ao início |
| `esc` `tab` | fecha o telefone |
| `/` | vai para a linha de busca do app (quando ele tem uma) |
| `1`…`9` | abre o app N do início, na ordem da grade |

Um item pode ter teclas próprias (escritas no rodapé do app); elas valem antes das de cima. As teclas do
telefone são o modo `world.phone` do keymap e podem ser religadas em Lua como qualquer outra.

| app | o que tem |
|---|---|
| **Gatos** | um item por gato: nome, cor do contexto, "estado · sala" e o selo `!` (bloqueado) ou `zZ`; busca ao digitar. `enter` abre a página do gato: **ir até**, **conversar**, **sentar no terminal**, **responder**, **parar**, **renomear**, **mover de contexto**, **arquivar**. O ícone mostra quantos estão bloqueados |
| **Inbox** | os gatos que esperam você, em ordem; `enter` conversa com ele. `shift+space` (no NAV) chama o mais urgente |
| **Mapa** | abre o mapa (`o`) e fecha o telefone |
| **Apps** | os apps instalados (os mesmos do `world+d`); `enter` abre, `i` **abre isolado** (navegadores, veja [instância única](#apps-de-instância-única-e-abrir-isolado)); um app já aberto fora do mundo diz "aberto lá fora" |
| **Janelas** | todas as telas do mundo, com sala, mesa e `app_id`; `enter` vai até ela, `Q` fecha, `M` manda para outra sala |
| **Música** | o que a vitrola toca (lido na hora, nunca guardado), tocar/pausar, anterior/próxima, volume −/+ e mudo. Sem player MPRIS na sessão, só diz isso |
| **Biblioteca** | a busca da biblioteca: digite e `enter`; os achados (até 60) abrem com `enter` (fecha o telefone), `e` abre no editor, `y` copia o caminho. O que é do cofre nunca aparece |
| **Cofre** | o estado ("trancado" ou "aberto há N s") e dois itens: **trancar** e **ir até o cofre**. Nunca mostra um nome |
| **Ajustes** | as preferências, abaixo |
| **Ajuda** | as teclas de cada modo, lidas do keymap de verdade (com o que você religou em Lua), o modo atual primeiro; busca ao digitar. `?` no NAV abre o telefone direto aqui |

**Ajustes**, e onde cada um fica:

| item | o quê | onde fica salvo |
|---|---|---|
| movimento reduzido | corta as animações do HUD e do telefone | `ui.toml`; para o mundo inteiro (câmera, gatos), `--reduced-motion` |
| alto contraste | tinta escura sobre creme claro, contraste ≥ 7:1 | `ui.toml` |
| escala da fonte | de 80 % a 160 % (`h`/`l` no item) | `ui.toml` |
| faixa de status | `auto` → `sempre` → `nunca` | `ui.toml` |
| giro em passos, olhar com o mouse, vinheta de conforto | o ANDAR, na hora | não salva: para fixar, `[walk]` no `world.toml` |
| tecla do mundo, saída do FOCO | só leitura (`ctrl+alt`, `ctrl+]`) | `[keys]` no `world.toml` |

O `ui.toml` fica ao lado do `world.json` (`~/.local/state/nekuroddo/ui.toml`, ou no `--state-dir`), com uma
tabela `[prefs]` (`reduced_motion`, `contrast`, `font_scale`, `strip`); sob `--demo` ele nunca é escrito.

## A recepção e os objetos

A recepção é a sala de entrada. Além da gata recepcionista (que dá os avisos, as confirmações e o tour) e do
quadro de avisos (a inbox, o custo do dia, o uso de 5 h), ela tem objetos que são os "widgets" do mundo:

| objeto | o que mostra | o que faz |
|---|---|---|
| vitrola | o player de música (MPRIS) e o volume do sistema | `enter` toca/pausa, `h`/`l` trocam a faixa, `j`/`k` giram o volume; sem player, fica de tampa fechada |
| relógio de parede | a hora; a luz do mundo segue a faixa do dia | — |
| pote de óleo | a bateria: o nível do óleo é a carga; abaixo de 15 % o gato da almofada acorda e mia, e um selo aparece no HUD | — |
| planta-antena | a rede: as folhas são as barras; sem rede, murcha | — |
| portas | as portas das salas, no corredor; abrem e fecham sozinhas, e uma porta fechada bloqueia a passagem e abafa o som da sala | `enter` na porta |

O texto exato (42 %, 14:05) aparece como etiqueta 2D quando você chega a menos de 1,8 m e olha para o objeto.
No NAV, o SALTO leva até a frente de um objeto; `world.goto_object` (pela paleta) também. As teclas de mídia
(`XF86AudioPlay`, volume…) funcionam em qualquer modo. A mesma informação está no topo do telefone.

**Som.** O `nkw` toca uns poucos sons próprios pelo PipeWire (passos no ANDAR, portas, um miado, os bipes da
conversa, os cliques do cofre) e faz o **som por proximidade v1**: o volume de cada janela do mundo que toca
áudio (achada pelo processo do app) e o do player da vitrola caem com a distância até a tela dela ou até a
vitrola, com um pouco de pan, e uma porta fechada abafa a sala. Ele só abaixa, nunca aumenta, e devolve todos
os volumes ao sair. Ainda não há um interruptor no `world.toml` (`[sound]` é **planejado**); `--no-sys` (e o
`--demo`) desligam o som e todos os adaptadores de sistema.

## A biblioteca e o cofre

`b` leva à **biblioteca**, a sala ao lado da recepção, até o balcão da bibliotecária. Cada **raiz**
(`[library] roots`, padrão: a sua home) é uma parede de estantes; uma **pasta** é uma estante, um **arquivo** é
um livro (a cor da lombada diz o tipo — código, texto, imagem, mídia, compactado —, a altura cresce com o
tamanho). Só nomes e metadados são lidos (`readdir` + `stat`), e só quando você chega na estante; nunca o
conteúdo, fora a prévia que você pede. Diante de uma estante, a câmera fica a uns 2,4 m, para a estante inteira
caber na tela.

Diante de uma estante (modo `world.library`; o que ele não liga é o NAV):

| tecla | o quê |
|---|---|
| `h` `l` · `j` `k` (setas) | livro anterior / próximo · prateleira de baixo / de cima |
| `enter` · `backspace` | abre o livro (ou entra na pasta) · sobe um nível |
| `/` | busca fuzzy na subárvore (até 100 mil entradas, 12 níveis, 2 s); a bibliotecária traz os achados numa mesa |
| `e` · `p` · `y` | abre no `$EDITOR` num terminal novo · prévia no atril (texto até 64 KB) · copia o caminho |
| `esc` | sai da estante |

Abrir um livro usa o `xdg-open` do mundo, e a janela que nasce vai para a sala de onde você veio (`[library]
open_in = "origin"`) ou para a mesa de leitura da biblioteca (`"library"`). Pela paleta ou pelo Lua,
`world.open {path}` faz o mesmo; o app Biblioteca do telefone busca sem ir até lá. Até 400 livros por estante;
o resto vira uma caixa "+N", que a busca abre. As pastas `.git`, `node_modules`, `target` e `.cache` nunca são
abertas; arquivos ocultos ficam escondidos (`show_hidden`); a varredura não segue um link simbólico para fora
das raízes nem atravessa outro sistema de arquivos montado; nada é indexado em disco.

**O cofre.** Pastas e arquivos sensíveis nunca aparecem nas estantes nem na busca: vão para um **cofre** de
parede, ao lado do balcão da bibliotecária. Por padrão: `.ssh`, `.gnupg`, `.password-store`,
`.local/share/keyrings`, `.config/gcloud`, `.aws`, `.kube`, `.docker/config.json` e todo `.env`, `.env.*`,
`*.pem` e `id_*`. A lista só cresce pelo `world.toml` (`[vault] extra_roots`, `extra_globs`), nunca encolhe.

- **Destrancar:** diante do cofre, `t` (ou `enter`) fala com a bibliotecária, que pede a palavra. A frase
  (`[vault] phrase`, padrão `"destrancar"`) é digitada na linha da conversa e aparece como pontos; não vai para
  o histórico nem para log nenhum. Errou: espera 2 s; depois de 5 erros, 60 s de bloqueio.
- **Aberto:** dentro há cartões com **só os nomes** (tipo, tamanho, data). `enter` entra numa pasta, `y` copia o
  caminho, `e` abre no editor (o editor é outro processo); prévia, busca e entregar a um gato ficam desligados.
  Um selo "cofre aberto" fica no HUD, e o app Cofre do telefone diz há quanto tempo (nunca o que há dentro).
- **Tranca sozinho** depois de 20 s sem mexer, ao sair da biblioteca, quando a janela do mundo perde o foco, ao
  entrar em FOCO em qualquer tela, antes de qualquer captura (`--shot`), e com `world.vault_lock`, `enter` no
  cofre aberto ou "trancar" no telefone. Ao trancar, a lista de nomes é descartada da memória.
- **Auditoria:** `~/.local/state/nekuroddo/vault.log` (permissão `0600`), uma linha por evento (`unlock`,
  `lock_timeout`, `unlock_denied`…), nunca a frase nem nomes de arquivo; acima de 256 KB vira `vault.log.1`.
  Nunca é escrito no `--demo`.

**O que o cofre protege, com honestidade.** O `nkw` roda como o seu usuário, que já pode ler `~/.ssh`; não há
fronteira de privilégio, e o cofre **não** protege contra alguém (ou algum programa) que já tem a sua sessão.
Ele protege contra **exposição acidental**: um screenshot, uma gravação de tela, um agente que recebe um caminho
sem querer, alguém olhando por cima do ombro, uma pata no teclado. Por isso é atrito de propósito, uma janela
curta de exposição, e nada do cofre sai do processo `nkw`. Uma captura feita **de fora** (o compositor da sua
sessão gravando a tela inteira) não é visível para o mundo aninhado: a defesa é o auto-trancar.

## Apps dentro do mundo

Um app aberto lá dentro é um cliente Wayland comum do `nkw`: ele não sabe que está numa mesa 3D. O que segue é
o que esperar dele e como o lançador ajuda.

### Compatibilidade

Auditoria de 2026-10-24, aninhada no niri (Intel Iris Xe), cada app com perfil descartável e sessão D-Bus
própria, nunca com os seus perfis. A coluna "cursor" mostra a mudança desta onda: antes o mundo ignorava o
cursor de todo app.

| app (`app_id`) | abre | teclas no FOCO | acentos | menus / fecham com `ctrl+]` | cursor | escala 1,25 | 2ª vez |
|---|---|---|---|---|---|---|---|
| `foot`, `kitty` | ✓ | ✓ | ✓ | — | ✓ forma do app | ✓ fracionária | — |
| `weston-terminal` | ✓ | ✓ | ✓ | — | ✗ desenha o próprio | ◐ escala 1, esticado | — |
| `org.gtk.WidgetFactory4`, nautilus | ✓ | ✓ | ◐ não conferido | ✓ / ✓ | ✓ | ✓ (só o widget-factory) | ✓ janela nova no mundo |
| `firefox`, `zen` | ✓ | ✓ | ◐ não conferido | firefox ✓ / ✓; zen sem menu em about:blank | ✓ | ✓ (só o firefox) | ✓ janela nova |
| `chromium` (com ou sem ozone) | ✓ | ✓ | ◐ não conferido | ✓ / ✓ | ✓ | ✓ | ◐ aba nova na mesma janela |
| `Chromium` pelo X11 (`xwayland-satellite`) | ✓ | ✓ | ◐ não conferido | ✓ / ◐ | ✓ protocolo ligado | — | ◐ aba nova |
| `mpv` (dmabuf; `--fs` em tela cheia) | ✓ | ✓ | ◐ não conferido | — | ✓ | ✓ | — |
| discord | ✗ não abre em 20 s com perfil novo (baixa o app antes) | | | | | | |

Também desta onda: colar com o botão do meio (seleção primária) entre apps do mundo, travar o ponteiro para
o app em FOCO que pedir (jogos, ferramentas 3D), e o mpv se marca como vídeo. Sem erros de protocolo em nenhum
app que abriu. Ainda não testado: copiar de um app para fora do mundo, arrastar e soltar dentro de uma janela,
seletor de arquivo e IME (não há nenhum nesta máquina).

`✓` funciona, `◐` funciona com ressalva, `✗` não funciona, `—` não se aplica. `make world-compat` roda a parte
sem janela (o `nkw-testclient` exercita cada protocolo, e os apps instalados entram quando presentes);
`make world-compat-nested` refaz a matriz com os apps de verdade numa janela aninhada, por até dois minutos.

O que o mundo oferece aos apps, além do básico (`xdg-shell`, `shm`, `dmabuf`, `viewporter`,
`fractional-scale`, `xdg-decoration` sempre do lado do servidor, `xdg-activation`, `xdg-dialog`, área de
transferência):

| protocolo | o que dá |
|---|---|
| `cursor-shape` | o cursor do host muda com o app no FOCO |
| `primary-selection` | selecionar e colar com o botão do meio, entre os apps do mundo |
| `presentation-time` | o app sabe quando o quadro dele foi mostrado (players e jogos ficam lisos) |
| `single-pixel-buffer` | fundos de uma cor só sem gastar memória |
| `content-type` | o app diz se é foto, vídeo ou jogo; o mundo anota |
| `xdg-foreign` | um diálogo de outro app pode se prender à janela certa |
| `idle-inhibit` | um player pede para a tela não apagar; o mundo registra o pedido (repassá-lo ao host ainda não é garantido) |
| `pointer-constraints`, `relative-pointer` | ponteiro travado para jogos e apps 3D no FOCO |
| `text-input-v3` | só com `--text-input`, para avaliação (veja [IME](#método-de-entrada-ime)) |

### O ambiente do lançador

Tudo o que o mundo abre — pelo `world+d`, pelo telefone, pelo `T`, por `nkw -- cmd` ou pela biblioteca —
recebe, depois do `WAYLAND_DISPLAY` do mundo e de um `XDG_ACTIVATION_TOKEN` novo:

| variável | valor | para quê |
|---|---|---|
| `GDK_BACKEND` | `wayland,x11` | GTK: Wayland primeiro |
| `QT_QPA_PLATFORM` | `wayland;xcb` | Qt: Wayland primeiro |
| `QT_WAYLAND_DISABLE_WINDOWDECORATION` | `1` | Qt sem barra de título própria (o mundo é a moldura) |
| `SDL_VIDEODRIVER` | `wayland` | jogos e apps SDL |
| `CLUTTER_BACKEND` | `wayland` | apps Clutter |
| `MOZ_ENABLE_WAYLAND` | `1` | Firefox e derivados (versões antigas) |
| `ELECTRON_OZONE_PLATFORM_HINT` | `auto` | Electron (Discord, VS Code…) escolhe Wayland |
| `_JAVA_AWT_WM_NONREPARENTING` | `1` | apps Java desenham certo |
| `GTK_USE_PORTAL` | `0` | GTK abre o próprio seletor de arquivo, dentro do mundo |
| `DISPLAY` | `:N` | só quando o X11 do mundo está de pé |

O mundo **nunca** muda `XDG_CURRENT_DESKTOP` (os portais escolhem o backend por ela) nem nada de `DBUS_*`.
O terminal do `T` também recebe tudo isso, então o que você roda dentro dele herda. `[launch] env` no
`world.toml` acrescenta ou troca variáveis (o arquivo ganha):

```toml
[launch]
env = { QT_QPA_PLATFORM = "xcb", MEU_APP_TEMA = "claro" }
```

### X11 pelo `xwayland-satellite`

O mundo não tem um servidor X próprio: ele usa o [`xwayland-satellite`](instalacao.md#mundo-nkw), que roda
o `Xwayland` como um cliente Wayland comum e entrega cada janela X11 ao mundo como uma janela normal (o
`app_id` é o `WM_CLASS`, então os `[[rules]]` valem igual).

- Com `[launch] xwayland = "auto"` (padrão) e o `xwayland-satellite` no `PATH`, o mundo o sobe **sob
  demanda**: na primeira vez que você abre um app ou um terminal, no primeiro display livre de `:7` a `:17`.
  Um app aberto enquanto ele ainda sobe espera até 2 s pelo socket; dali em diante todo processo novo recebe
  `DISPLAY`. Sem o binário, `auto` fica quieto e só apps Wayland abrem.
- `"on"` faz o mesmo e avisa (um toast) se o binário não existe; `"off"` nunca o sobe. `--xwayland
  auto|on|off` ganha do arquivo; `world.xwayland {on}`, pela paleta, liga ou desliga na hora.
- Se ele não subir em 2 s, um toast diz "xwayland-satellite não subiu" e os apps seguem sem `DISPLAY`.
- Com a faixa de status em `always`, um ícone mostra que o X11 está de pé.

Não há gerenciador de janelas X11 no mundo: posição e tamanho são do mundo, como para qualquer janela.

### Apps de instância única e "abrir isolado"

Navegadores, IDEs e chats costumam ter **uma instância só por usuário**: se o app já está aberto na sua
sessão, abrir de novo de dentro do mundo só pede à instância de fora uma janela nova — que abre **lá fora**.
O mundo avisa: ao abrir um app da lista `[launch] isolate` (padrão: `firefox`, `zen-browser`, `chromium`)
enquanto ele roda fora do mundo, um toast diz "o *app* já está aberto fora do mundo: a janela vai abrir lá fora
· telefone › Apps › abrir isolado", e o lançamento acontece mesmo assim (foi o que você pediu).

**Abrir isolado** (`i` no app Apps do telefone, ou `world.launch_isolated {id}`) abre o app com um **perfil só
do mundo**, em `~/.local/state/nekuroddo/profiles/<id>/` (permissão `0700`; com `--state-dir`, dentro dele):

- `firefox`, `zen-browser`: `--new-instance --profile <pasta>`;
- `chromium`: `--user-data-dir=<pasta> --no-first-run`.

É um perfil separado de verdade: sem os seus favoritos, logins e extensões de fora, até você fazê-los lá. O
mundo nunca mexe na instância de fora nem a encerra. No `--demo` não há perfis isolados.

### Seletores de arquivo e portais

- Apps **GTK** abrem o próprio seletor de arquivo, **dentro** do mundo (`GTK_USE_PORTAL=0`).
- Apps que só usam o **portal** (`xdg-desktop-portal`) — os de flatpak e snap, e alguns Electron e Qt —
  pedem o diálogo ao portal da sua sessão, que o abre **lá fora**, na sua área de trabalho. É limitação do modo
  aninhado: o portal não sabe do mundo. Escolha o arquivo lá e volte; o resultado chega ao app normalmente.
- O mesmo vale para outros portais (captura de tela, "abrir com"): eles aparecem do lado de fora.

### Método de entrada (IME)

Teclas mortas e composição (`´` + `a` = `á`) funcionam no FOCO, pelo xkb. Um **método de entrada** (IBus,
Fcitx: chinês, japonês, coreano, emoji por pinyin) **não funciona** dentro do mundo ainda: o `nkw` não oferece
o `text-input-v3` por padrão. `--text-input` o oferece para avaliação — o app vê o protocolo e o liga —, mas
nada é ligado ao IME da sua sessão. O caminho (fase 6) é o mundo receber do host a pré-edição e o texto
confirmado (pela janela aninhada) e repassá-los ao app no FOCO pelo `text-input-v3`, com a posição do cursor
de volta para a janela de candidatos.

## Configuração: `world.toml`

`~/.config/nekuroddo/world.toml` (ou `$XDG_CONFIG_HOME/nekuroddo/world.toml`; com `--state-dir`, ao lado do
`world.json`). Sob `--demo` ele não é lido. Sem arquivo, valem os padrões; um arquivo quebrado também, com um
toast. Uma tabela ou chave que o mundo não conhece é ignorada; um valor fora da faixa é preso a ela, com um
aviso no log.

```toml
[keys]
world_mod = "ctrl+alt"      # ou "super" (o --world-mod ganha)
focus_escape = "ctrl+]"     # a saída do FOCO (o --focus-escape ganha)

[launch]
terminal = "foot"           # o terminal do T / world+enter; ou ["foot", "-e", "tmux"]
isolate = ["firefox", "zen-browser", "chromium"]   # os que ganham "abrir isolado"
xwayland = "auto"           # auto | on | off (o --xwayland ganha)
env = { }                   # variáveis a mais para todo app (ganham das do mundo)

[hud]
strip = "auto"              # auto | always | never

[[rules]]                   # para onde vai a janela nova de um app
app_id = "firefox"          # exato, ou um prefixo terminado em * ("org.gnome.*")
room = "pessoal"            # a sala, pelo nome
desk = "last"               # "last" ou um índice: uma mesa nova ali
width = "two_thirds"        # third | half | two_thirds | full

[walk]                      # ANDAR
speed = 2.8                 # m/s, 1 a 5
run_mul = 2.0               # correndo: ×1 a ×3
turn = "smooth"             # ou "snap" (o padrão com movimento reduzido)
turn_rate_dps = 110         # giro suave, 45 a 240 °/s
snap_deg = 30               # giro em passos: 15, 30 ou 45
mouse_look = true           # olhar com o mouse
mouse_deg_per_count = 0.12
comfort_vignette = false    # escurece a periferia ao andar ou girar
on_exit = "auto"            # auto | snap | stay

[camera]
eye_height = 1.5            # m, 1,2 a 1,8
pitch_max_deg = 55

[library]
roots = ["~"]               # paredes de estantes; um caminho relativo é dentro da home
show_hidden = false
exclude_dirs = []           # somam-se a .git, node_modules, target, .cache
per_shelf = 400             # até 400
search_entries = 100000     # orçamento da busca: entradas (até 100 mil),
search_depth = 12           # profundidade (até 12)
search_ms = 2000            # e tempo (até 2 s)
open_in = "origin"          # ou "library"

[vault]
phrase = "destrancar"       # mínimo 6 caracteres
auto_lock_after_s = 20      # 1 a 600
cooldown_s = 2
max_fails = 5               # 1 a 20
lockout_s = 60
extra_roots = []            # somam-se à lista padrão (que nunca encolhe)
extra_globs = []
```

Sem regra, uma janela nova entra numa mesa nova logo à direita de onde você está, e o mundo lembra o último
lugar de cada `app_id` (a "memória de lugar"); uma regra explícita ganha da memória. As **teclas** não ficam no
`world.toml`: ficam no keymap do core e no seu `init.lua` (abaixo). As preferências do HUD que você muda no
telefone ficam no `ui.toml` e ganham do `[hud]`.

**(planejado)** `[render]`, `[sound]`, `[objects]` (quais objetos e onde), `[doors]` e `[notifications]`: estão
no plano ([plano-mundo-objetos.md](plano-mundo-objetos.md)); o mundo ainda não as lê e usa os padrões.

**O estado** fica em `~/.local/state/nekuroddo/` (ou `$XDG_STATE_HOME/nekuroddo/`, ou o `--state-dir`):
`world.json`, escrito de forma atômica 2 s depois da última mudança e na saída (salas, mesas, telas, a estação
atual, a memória de lugar e se o tour já foi visto); `ui.toml` (as preferências do telefone); `profiles/` (os
perfis isolados); `vault.log`. Um `world.json` corrompido vira `world.json.bak`, o mundo começa limpo e avisa.

## Lua no mundo

O mundo usa o mesmo runtime Lua do core ([lua.md](lua.md)); não há um segundo.

- **`nk.keymap.set` nos modos do mundo** (`normal.world`, `world`, `world.walk`, `world.focus`, `world.talk`,
  `world.hint`, `world.palette`, `world.line`, `world.library`, `world.phone`): o `nkw` lê as sobreposições
  pelo `GET /v1/keymap` e relê a cada `keymap.changed`, na hora. O app Ajuda do telefone mostra o resultado.
- **Uma função numa tecla** chega ao mundo como `lua.key:<modo>:<tecla>`; ao apertar, o `nkw` pede ao core que
  rode a função (`POST /v1/commands/lua.key:…`, no orçamento de callback do Lua).
- **`nk.ui.scene(comando, args)`** pede ao mundo que rode um comando dele (`world.goto_room`, `world.terminal`,
  `world.goto_object`…), como se fosse uma tecla.

```lua
-- ~/.config/nekuroddo/init.lua
nk.keymap.set("normal.world", "n", "world.phone")         -- n abre o telefone (no lugar de renomear o gato)
nk.keymap.set("world.walk", "j", "world.turn_left")       -- girar com j/l em vez de q/e
nk.keymap.set("world.walk", "l", "world.turn_right")
nk.keymap.set("world", "world+a", function()              -- um acorde seu: vai à sala "acme"
  nk.ui.scene("world.goto_room", { room = "acme" })
end)
nk.keymap.set("world.library", "o", "world.book_open")    -- na estante, o também abre
```

As teclas do mundo não aparecem na ajuda da TUI nem do `nkg`.

## Conforto e movimento reduzido

- O campo de visão é fixo (60° na vertical); aproximar é mover a câmera, nunca mudar o zoom. Nunca há "roll":
  o horizonte é estável.
- Deslizes curtos (até 260 ms); percursos longos viram corte com um fade curto. `h`/`l` andam de lado, sem girar.
- Sem balanço de cabeça. Giro em passos (`[walk] turn = "snap"`) e vinheta de conforto
  (`[walk] comfort_vignette`), também no app Ajustes.
- **Movimento reduzido** (`--reduced-motion` ou `NKW_REDUCED_MOTION=1`): cortes em vez de deslizes, gatos em
  pose fixa, carteiros já parados, o texto da conversa e do tour inteiro de uma vez. O ANDAR continua (quem se
  move é você). O item do app Ajustes vale para o HUD e o telefone.
- **Alto contraste** e **escala da fonte** (80 % a 160 %) no app Ajustes, para o HUD e o telefone.

## Privacidade

- **O mundo não manda nada a lugar nenhum.** Fala só com o seu core (um socket local), com o sistema (D-Bus,
  PipeWire) e com o compositor de fora.
- **Nada da biblioteca sai do processo `nkw`**: nem para o core, nem para os agentes, nem para o `world.json`.
  Os caminhos do cofre nunca entram em comando nenhum, em `nk.ui.scene`, em `world.open` (que os recusa), na
  memória de lugar, no telefone, na faixa de status, em logs ou no `dump` de depuração.
- Os eventos do sistema não carregam identificadores: o nome da rede, dos dispositivos e o título da música só
  aparecem na etiqueta 2D e no app Música, lidos na hora, e nunca são gravados nem saem no `dump`.
- A área de transferência é ligada ao host (pelo `wlr-data-control` do compositor de fora) só passando
  descritores de arquivo: o conteúdo nunca é lido, registrado nem guardado. No demo e sem janela, a ponte fica
  desligada.
- O cofre nunca registra a frase nem nomes de arquivo (veja a [auditoria](#a-biblioteca-e-o-cofre)).
- Os perfis isolados são pastas do mundo; ele nunca lê nem escreve os perfis, o histórico ou as sessões dos
  seus apps de fora.
- **Capturas e documentação usam só o demo**: `nkw --demo` não lê a home, nem o `world.toml`, nem o
  `world.json`; os objetos, o lançador e o relógio são de fixture, e a biblioteca do demo é uma fixture com
  `projeto/`, `notas.md`, `fotos/` e um cofre de mentira. Uma captura com o cofre aberto é impossível: ele
  tranca antes.

## Desempenho

O `nkw` é feito para ficar parado quando nada muda: o laço não arma timer nenhum quando ocioso, só redesenha
com dano, o mundo congela no FOCO, telas distantes recebem quadros numa taxa menor, o telefone e o HUD só são
redesenhados quando o que mostram muda, e o cursor é o do host. Os orçamentos, medidos por `make world-perf`
(arquivo `gui/office/scripts/world-perf-budget.toml`):

| métrica | cenário | alvo | falha acima de |
|---|---|---|---|
| CPU ocioso | S1: NAV parado, 3 janelas | ≤ 0,1 % de um núcleo | 0,5 % |
| wakeups ociosos | S1 | ≤ 1/s | 5/s |
| digitando no FOCO | S2: 15 teclas/s | ≤ 1 % | 3 % |
| latência de tecla | S2 | ≤ 1 ms (a tecla sai na mesma volta do laço) | 1 quadro |
| mundo animado | S3: 5 gatos ativos e um carteiro | ≤ 3 % (≤ 0,5 % com movimento reduzido) | 6 % |
| CPU por quadro do mundo | S3/S4 | p50 ≤ 1 ms, p95 ≤ 2 ms | p95 3 ms |
| GPU por quadro | S4 a 1080p | ≤ 2 ms | 4 ms |
| câmera em movimento | S4/S5 | 60 fps, nenhum quadro perdido | 2 seguidos |
| primeiro quadro | partida | ≤ 250 ms | 500 ms |
| RAM | com até 8 janelas | ≤ 110 MB | 150 MB |
| binário | perfil `world` | ≤ 15 MB | 25 MB |
| sala típica | S4 | ≤ 80 draw calls, ≤ 100 mil triângulos | 200 draws |

Variáveis de ambiente para medir e depurar:

| variável | o quê |
|---|---|
| `NKW_STATS=1` | uma linha JSON por quadro no stderr (`nkw-stats {…}`: tempos de update, culling, importação das janelas, envio e composição, draws, triângulos) e, ao sair, o resumo (quadros, p50/p95, quadros perdidos, primeiro quadro, RSS) |
| `NKW_STATS=gpu` | o mesmo, mais o tempo de GPU por quadro |
| `NKW_MEM=1` | a memória por parte, no primeiro quadro e na saída |
| `NKW_LOG=…` | o filtro de log no stderr (padrão `warn`; por exemplo `NKW_LOG=nkw=debug`); o `RUST_LOG` não é lido |
| `NKW_FULL_DAMAGE=1` | desliga o dano parcial (para comparar) |

Os números de cada fase ficam em [log.md](log.md).

## Solução de problemas

- **Não sobe, erro de socket:** o `nkw` precisa de `XDG_RUNTIME_DIR` (toda sessão gráfica tem) e de um nome de
  socket livre; se outro mundo já usa `nkw-0`, passe `--socket nkw-1`.
- **Sem gatos, toast "sem core":** instale o core (`make install`) ou aponte para um que já roda
  (`--core unix:///…`). `--core none` abre o mundo sem gatos; `--demo`, com os de fixture.
- **Teclado no layout errado:** o mundo usa `XKB_DEFAULT_LAYOUT` (e `_VARIANT`, `_OPTIONS`) do ambiente em que
  sobe.
- **Um `ctrl+alt+…` faz outra coisa:** o compositor de fora o pegou antes; religue o de fora ou use
  `--world-mod super`.
- **Um app abriu fora do mundo:** se veio o toast "já está aberto fora do mundo", é um app de instância única
  aberto na sua sessão: use **abrir isolado** (telefone › Apps › `i`) ou feche o de fora. Um seletor de arquivo
  ou outro diálogo de **portal** sempre abre fora (veja [portais](#seletores-de-arquivo-e-portais)).
- **Um app só X11 não abre:** instale o `xwayland-satellite` ([instalacao.md](instalacao.md#mundo-nkw)) e
  confira que `[launch] xwayland` (ou `--xwayland`) não está `off`. Com a faixa de status em `always`, o ícone
  do X11 diz se ele subiu; o toast "xwayland-satellite não subiu" diz que não.
- **Um app abre como X11 em vez de Wayland:** o app ignorou as variáveis do lançador; troque a dele em
  `[launch] env` (por exemplo `ELECTRON_OZONE_PLATFORM_HINT = "wayland"`).
- **O mouse não gira a câmera no ANDAR:** o host precisa entregar movimento relativo com o ponteiro travado.
  Sem isso, olhe pelo teclado (`q`/`e`, setas) ou passe `--mouse-look off`.
- **Um jogo não trava o ponteiro:** o ponteiro só trava no FOCO, na janela que pediu; `ctrl+]` solta. Um app
  que pede "confinar" recebe o ponteiro travado (aninhado não há como confinar à janela).
- **O cursor fica a seta padrão num app:** ele desenha a própria imagem de cursor em vez de pedir uma forma
  (`cursor-shape`); imagens próprias ainda não passam ao host **(planejado)**.
- **Chinês, japonês, coreano não entram:** não há método de entrada (IME) no mundo ainda (veja
  [IME](#método-de-entrada-ime)).
- **Texto borrado em escala fracionária (1,25, 1,5):** no FOCO a janela é desenhada em 1:1 na escala da saída
  (`fractional-scale` + `viewporter`), e o HUD, nos pixels físicos; um app que não fala `fractional-scale` sai
  na escala inteira acima e é reduzido.
- **O HUD sumiu:** `z` no NAV o esconde e mostra; a faixa de status pode estar em `never` (app Ajustes).
- **Medir ou relatar um problema de desempenho:** `NKW_STATS=1 nkw --demo` e o resumo ao sair; `make
  world-perf` roda os cenários.

## Desenvolvimento

```sh
make world-test           # cargo test dos crates do mundo + o teste do world-perf.sh
make world-lint           # rustfmt + clippy -D warnings
make run-world            # nkw --demo direto do código
make world-shot           # as capturas de verificação (sem janela, só o demo)
make world-perf           # os cenários de desempenho contra os orçamentos (precisa de GPU)
make world-compat         # a suíte de compatibilidade sem janela (testclient + apps instalados)
make world-compat-nested  # a matriz com apps reais, aninhada, por até 2 min (perfis descartáveis)
```

`nkw --headless --demo --script cena.nks --shot x.png` desenha sem janela, roteirizado — `key`, `text`,
`spawn` (com `env K=V` antes), `wait`, `shot`, `expect mode|focus|json`, `wait json`, `inject <plugin> <json>`
—, e salva a captura. Os roteiros ficam em `gui/office/scripts/scenarios/` (`S*.nks` para as medidas,
`shot-*.nks` para as capturas). O desenho por dentro está em [plano-mundo.md](plano-mundo.md) e
[plano-mundo-objetos.md](plano-mundo-objetos.md).

## Ver também

- [teclas.md](teclas.md#mundo-nkw) e [comandos.md](comandos.md#mundo--worldgo): a referência das teclas e dos
  comandos do mundo
- [lua.md](lua.md) e [protocolo.md](protocolo.md): o Lua e o protocolo que o mundo usa
- [instalacao.md](instalacao.md#mundo-nkw): compilar, instalar e o `xwayland-satellite`
- [gui.md](gui.md): o `nkg`, o escritório em Bevy (sem gerenciar janelas)
- [plano-mundo.md](plano-mundo.md) e [plano-mundo-objetos.md](plano-mundo-objetos.md): o plano e o adendo
