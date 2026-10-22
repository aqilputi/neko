---
type: Guide
title: O mundo (nkw)
description: Guia do nkw, o compositor Wayland 3D aconchegante do nekuroddo — suas janelas em mesas e salas, os agentes como gatos aldeões; como instalar e rodar aninhado, os modos e as teclas, o ANDAR, carregar telas, a recepção e seus objetos, a biblioteca e o cofre, o world.toml, Lua, privacidade, desempenho e solução de problemas, com o que ainda está chegando marcado.
resource: https://github.com/aqilputi/nekuroddo/tree/main/gui/office/crates/nkw
tags: [mundo, nkw, compositor, wayland, smithay, gles, gatos, biblioteca, cofre, guia]
timestamp: 2026-10-22T18:00:00Z
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
código. O que está entrando na fase atual está marcado **(chegando)**: as teclas e os comandos já estão no
core, o comportamento completo vem com a fase. O que é plano mais adiante está marcado **(planejado)**.

| parte | estado |
|---|---|
| Gerenciador de janelas aninhado: salas, corredor, mesas com pilhas, larguras, telas no ar, tela cheia como "TV", popups, diálogos, área de transferência com o host, escala fracionária, persistência (`world.json`), regras por `app_id`, lançador de apps | funciona |
| Modos NAV, FOCO (1:1), CONVERSA, SALTO, COMANDOS (paleta) e o mapa | funciona |
| Gatos: um por sessão, poses por estado, monitor com o tail ou o terminal, filhotes (subagents), a inbox vindo até você, carteiros, a recepcionista, o quadro de avisos; na CONVERSA, responder, escolher, parar e remover | funciona |
| A recepção e seus objetos: vitrola (MPRIS + volume), relógio de parede, pote de óleo da bateria, planta-antena, o cofre; portas com estado; sons próprios e som por proximidade v1 (só atenuação) | funciona |
| ANDAR pelo teclado: andar, correr, girar, olhar, colisão, usar os objetos na mira | funciona |
| ANDAR com o **mouse**, **carregar telas** na mão, mira em telas, gatos e portas, vinheta de conforto, `[walk]`/`[camera]` lidos do `world.toml` | **(chegando)** |
| **Biblioteca** de arquivos (estantes, livros, busca, abrir, prévia) e o **cofre** levado para ela, com a bibliotecária e o log de auditoria | **(chegando)** |
| Teclas Lua ligadas a uma função rodando no mundo (`lua.key:…`) | **(chegando)**: o core já as serve e roda; falta o `nkw` mandar |
| Passe de arte: teto, janelas com a luz da hora, padrões de chão e parede, gatos andando de quatro | **(chegando)** |
| Os comandos de sessão da TUI por tecla no NAV (`r`, `n`, `m`, `X`…) e a confirmação do `world+shift+e` | (planejado) |
| Fonte arredondada, alto contraste, escala de fonte | (planejado, fase 5) |
| XWayland (`xwayland-satellite`), janelas de um agente na mesa do gato dele, entregar um livro a um gato, `pointer-constraints` para as janelas | (planejado, fases 4b e 6) |
| Sessão própria (DRM/KMS, `super` como tecla do mundo) | (planejado, fase 7, opcional) |

## Instalar

```sh
make install          # o core (nk): o nkw sobe o próprio `nk serve`, como o nkg
make install-world    # compila o nkw (perfil world: release + LTO) e instala
                      # ~/.local/bin/nkw + o atalho "nekuroddo — mundo" no lançador
```

Pré-requisitos de compilação em [instalacao.md](instalacao.md#mundo-nkw). `make install-world PREFIX=…` muda o
destino; `make -n install-world` mostra os passos sem rodar nada. O atalho abre o `nkw` aninhado na sua sessão.

## Rodar

```sh
nkw --demo            # o mundo de demonstração: gatos, objetos e lançador de fixture; não lê sua home
nkw                   # suas sessões de verdade
nkw -- foot           # já abre um terminal lá dentro (qualquer comando depois de --)
```

Dentro do mundo, os apps encontram o compositor pelo socket `$XDG_RUNTIME_DIR/nkw-0` (`WAYLAND_DISPLAY=nkw-0`;
sem `XDG_RUNTIME_DIR` o `nkw` não sobe). Tudo o que ele lança recebe esse `WAYLAND_DISPLAY` e nenhum `DISPLAY`
(não há XWayland), e é encerrado quando o mundo fecha. `T` (ou `world+enter`) abre um terminal novo (`foot`,
ou o de `[launch] terminal`) e `world+d` abre a paleta nos apps instalados (os `.desktop` de `XDG_DATA_HOME` e
`XDG_DATA_DIRS`, respeitando `OnlyShowIn`).

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
| `--mouse-look on\|off` | olhar com o mouse no ANDAR; ganha de `[walk] mouse_look` **(chegando)** |
| `--reduced-motion` (ou `NKW_REDUCED_MOTION=1`) | movimento reduzido (veja [Conforto](#conforto-e-movimento-reduzido)) |
| `--preset64` | o visual "64": baixa resolução, filtro de 3 pontos e dither |
| `--state-dir DIR` | onde guardar o `world.json` (e ler o `world.toml`) em vez do padrão |
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
   quadro de avisos. O canto de cima à esquerda diz o modo: `NAVEGAR`.
2. `J`/`K` levam de sala em sala pelo corredor: uma sala por contexto do core, mais "minha sala" para as
   janelas que você abrir fora de um contexto.
3. `h`/`l` deslizam de mesa em mesa. Numa mesa com gato, `t` conversa com ele; `enter` senta no terminal dele.
4. `T` abre um terminal numa mesa nova à direita; `enter` entra nele em 1:1 (**FOCO**) e `ctrl+]` sai.
5. `:` abre os **COMANDOS** (a paleta). `world+shift+e` sai do mundo e fecha as janelas que estão lá dentro
   (hoje sai direto; a confirmação é **planejada**).

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
| mapa | — | `o` / `world+o` (de novo: o andar inteiro) | `esc` |
| estante | — | diante de uma estante da biblioteca **(chegando)** | `esc` |

Nada rouba o foco: um gato que chega, uma confirmação ou um pedido de atenção de um app viram um **selo** no
HUD, e você decide quando atender. Uma tecla que já estava apertada quando você muda de modo só conta depois de
solta (o `w` que entrou no ANDAR não sai andando). Todas as teclas abaixo são comandos do core e podem ser
religadas em Lua; a referência completa está em [teclas.md](teclas.md#mundo-nkw) e
[comandos.md](comandos.md#mundo--worldgo). Enquanto o keymap do core não chega (ou sem core), valem as mesmas
teclas, de uma tabela interna.

### NAV: de estação em estação

Uma **estação** é um lugar onde a câmera para: diante de uma mesa, de uma tela no ar, de uma porta, do quadro
de avisos, de um objeto, ou onde você saiu do ANDAR.

| tecla | o quê |
|---|---|
| `h` `l` (setas) | estação anterior / próxima (travelling lateral, sem girar) |
| `j` `k` (setas) | tela de baixo / de cima na pilha da mesa |
| `J` `K` · `1`…`9` · `gg` `G` | próxima / anterior sala · sala N · primeira / última mesa |
| `enter` `i` · `t` · `w` | FOCO · CONVERSA com o gato · ANDAR |
| `f` · `F` · `o` · `b` | SALTO · SALTO direto para o FOCO · mapa · biblioteca **(chegando)** |
| `H` `L` · `,` `.` · `-` `=` | tela para a mesa ao lado · junta na pilha da esquerda / tira para uma mesa só dela · mesa mais estreita / mais larga |
| `p` · `M` · `T` · `Q` | prende a tela no ar · manda para outra sala · terminal novo · fecha a janela |
| `shift+space` · `:` | chama o gato mais urgente · COMANDOS |

**(planejado)** Os comandos de sessão da TUI no NAV, sobre o gato da estação (`r` responde, `x` para, `n`
renomeia, `m` muda de contexto, `X` arquiva, `dd` remove, `c` cria, `a`/`A` vão ao próximo que espera você): o
keymap já os herda de `normal`, mas o `nkw` ainda não os manda ao core. Hoje, responder, parar e remover se
fazem na CONVERSA (`t`). O mundo nunca liga o `q` da TUI: sair é `world+shift+e`.

### ANDAR: livre, em primeira pessoa

| tecla | o quê |
|---|---|
| `w` `s` `a` `d` (segurar) | andar para a frente / para trás / de lado |
| `shift` (segurar) | correr |
| `q` `e` (e `left` `right`) | girar |
| `up` `down` · `c` | olhar para cima / para baixo · voltar ao horizonte |
| `enter` `space` | usar o que está na mira: um objeto, uma porta; uma tela (FOCO) e um gato (CONVERSA) **(chegando)** |
| `j` `k` · `h` `l` | ajustar o objeto na mira (o volume da vitrola) · anterior / próximo nele (a faixa) |
| `g` · `[` `]` · `-` `=` | pegar / soltar a tela · mais perto / mais longe · tamanho **(chegando)** |
| `f` · `esc` | SALTO · volta ao NAV |

No ANDAR, o botão esquerdo do mouse é `interact` e o direito, `grab`. Os objetos têm a sua mira (até 4 m, num
cone de 8°). A **mira desenhada** no centro da tela — um ponto que vira anel quando há algo à frente — e a mira
em telas, gatos e portas estão **chegando**. Você colide com paredes, móveis, gatos e portas fechadas e
escorrega rente a eles; não há pulo nem escada (o mundo é plano). O ANDAR usa os padrões do plano (2,8 m/s,
correndo ×2, giro de 110°/s, olhar até ±55°, olho a 1,5 m); lê-los do `world.toml` está **chegando**.

**Ao sair**: se há uma estação a menos de 1,5 m e quase na mesma direção, a câmera desliza até ela; senão você
fica onde está, numa estação "você está aqui", e `h`/`l` seguem para a estação mais próxima de cada lado. É a
regra `auto`; `[walk] on_exit = "snap"` (sempre encaixa na mais próxima) e `"stay"` (nunca encaixa) estão
**chegando**.

**O mouse no ANDAR (chegando).** Com `[walk] mouse_look = true` (padrão) ou `nkw --mouse-look on`, entrar no
ANDAR trava e esconde o ponteiro do host e o movimento do mouse gira a câmera (0,12° por contagem, sem
aceleração; `[walk] mouse_deg_per_count`). Sair do ANDAR — ou a janela do mundo perder o foco (um `alt+tab` lá
fora) — devolve o ponteiro. Olhar pelo teclado funciona sempre; se o host não entregar movimento relativo, é o
único jeito. O movimento relativo vem de um ajuste pequeno no backend winit do Smithay, mantido em
`gui/office/patches/`.

**Carregar uma tela (chegando).** `g` com uma tela na mira a solta da mesa: ela flutua 1,2 m à sua frente,
sempre virada para você, e uma dica "g solta · [ ] distância · - = tamanho" aparece embaixo. `[` `]` aproximam
e afastam (0,8 a 3 m, de 20 em 20 cm), `-` `=` mudam a largura. `g` de novo solta, na primeira regra que servir:

1. uma mesa a menos de 0,6 m: a tela entra na pilha dela;
2. uma parede a menos de 0,4 m e com até 15° de desvio: a tela encosta nela (a 5 cm, presa a uma grade de 25 cm);
3. senão, fica no ar onde está, e vira uma estação.

### FOCO: a janela em 1:1

A câmera desliza até a pose exata em que a tela ocupa os pixels dela, e a janela passa a ser desenhada em 2D,
nítida, também em escala fracionária. O teclado vai todo para a janela — só `ctrl+]` (`[keys] focus_escape`) e
os acordes `world+` ficam com o mundo. Em volta, o mundo congela e escurece. Popups e menus só existem no FOCO.
`world+h`/`l`/`j`/`k` passam o FOCO para a tela vizinha sem sair; `world+space` atende o gato mais urgente;
`world+w` sai andando dali.

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

## A recepção e os objetos

A recepção é a sala de entrada. Além da gata recepcionista (que dá os avisos e as confirmações) e do quadro de
avisos (a inbox, o custo do dia, o uso de 5 h), ela tem objetos que são os "widgets" do mundo:

| objeto | o que mostra | o que faz |
|---|---|---|
| vitrola | o player de música (MPRIS) e o volume do sistema | `enter` toca/pausa, `h`/`l` trocam a faixa, `j`/`k` giram o volume; sem player, fica de tampa fechada |
| relógio de parede | a hora; a luz do mundo segue a faixa do dia | — |
| pote de óleo | a bateria: o nível do óleo é a carga; abaixo de 15 % o gato da almofada acorda e mia, e um selo aparece no HUD | — |
| planta-antena | a rede: as folhas são as barras; sem rede, murcha | — |
| cofre | hoje fica aqui (veja [a biblioteca e o cofre](#a-biblioteca-e-o-cofre-chegando)) | `enter` diante dele |
| portas | as portas das salas, no corredor; abrem e fecham sozinhas, e uma porta fechada bloqueia a passagem e abafa o som da sala | `enter` na porta |

O texto exato (42 %, 14:05) aparece como etiqueta 2D quando você chega a menos de 1,8 m e olha para o objeto.
No NAV, o SALTO leva até a frente de um objeto; `world.goto_object` (pela paleta) também. As teclas de mídia
(`XF86AudioPlay`, volume…) funcionam em qualquer modo.

**Som.** O `nkw` toca uns poucos sons próprios pelo PipeWire (passos no ANDAR, portas, um miado, os bipes da
conversa, os cliques do cofre) e faz o **som por proximidade v1**: o volume de cada janela do mundo que toca
áudio (achada pelo processo do app) e o do player da vitrola caem com a distância até a tela dela ou até a
vitrola, com um pouco de pan, e uma porta fechada abafa a sala. Ele só abaixa, nunca aumenta, e devolve todos
os volumes ao sair. Ainda não há um interruptor no `world.toml` (`[sound]` é **planejado**); `--no-sys` (e o
`--demo`) desligam o som e todos os adaptadores de sistema.

## A biblioteca e o cofre (chegando)

Hoje o `b` ainda não leva a lugar nenhum (a sala da biblioteca está sendo construída), e o cofre fica na
recepção: `enter` diante dele abre uma conversa com a recepcionista ("Qual é a frase?"), e ele tranca sozinho
por tempo, ao sair da recepção e antes de qualquer captura — mas ainda não guarda nada, porque não há estantes.
O que segue é a biblioteca como ela chega nesta fase.

`b` leva à **biblioteca**, a sala ao lado da recepção. Cada **raiz** (`[library] roots`, padrão: a sua home) é
uma parede de estantes; uma **pasta** é uma estante, um **arquivo** é um livro (a cor da lombada diz o tipo —
código, texto, imagem, mídia, compactado —, a altura cresce com o tamanho). Só nomes e metadados são lidos
(`readdir` + `stat`), e só quando você chega na estante; nunca o conteúdo, fora a prévia que você pede.

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
`world.open {path}` faz o mesmo. Até 400 livros por estante; o resto vira uma caixa "+N", que a busca abre.
As pastas `.git`, `node_modules`, `target` e `.cache` nunca são abertas; arquivos ocultos ficam escondidos
(`show_hidden`); a varredura não segue um link simbólico para fora das raízes nem atravessa outro sistema de
arquivos montado; nada é indexado em disco.

**O cofre.** Pastas e arquivos sensíveis nunca aparecem nas estantes nem na busca: vão para um **cofre** de
parede, ao lado do balcão da bibliotecária. Por padrão: `.ssh`, `.gnupg`, `.password-store`,
`.local/share/keyrings`, `.config/gcloud`, `.aws`, `.kube`, `.docker/config.json` e todo `.env`, `.env.*`,
`*.pem` e `id_*`. A lista só cresce pelo `world.toml` (`[vault] extra_roots`, `extra_globs`), nunca encolhe.

- **Destrancar:** diante do cofre, `t` (ou `enter`) fala com a bibliotecária, que pede a palavra. A frase
  (`[vault] phrase`, padrão `"destrancar"`) é digitada na linha da conversa e aparece como pontos; não vai para
  o histórico nem para log nenhum. Errou: espera 2 s; depois de 5 erros, 60 s de bloqueio.
- **Aberto:** dentro há cartões com **só os nomes** (tipo, tamanho, data). `enter` entra numa pasta, `y` copia o
  caminho, `e` abre no editor (o editor é outro processo); prévia, busca e entregar a um gato ficam desligados.
  Um selo "cofre aberto" fica no HUD.
- **Tranca sozinho** depois de 20 s sem mexer, ao sair da biblioteca, quando a janela do mundo perde o foco, ao
  entrar em FOCO em qualquer tela, antes de qualquer captura (`--shot`), e com `world.vault_lock` ou `enter` no
  cofre aberto. Ao trancar, a lista de nomes é descartada da memória.
- **Auditoria:** `~/.local/state/nekuroddo/vault.log` (permissão `0600`), uma linha por evento (`unlock`,
  `lock_timeout`, `unlock_denied`…), nunca a frase nem nomes de arquivo; acima de 256 KB vira `vault.log.1`.
  Nunca é escrito no `--demo`.

**O que o cofre protege, com honestidade.** O `nkw` roda como o seu usuário, que já pode ler `~/.ssh`; não há
fronteira de privilégio, e o cofre **não** protege contra alguém (ou algum programa) que já tem a sua sessão.
Ele protege contra **exposição acidental**: um screenshot, uma gravação de tela, um agente que recebe um caminho
sem querer, alguém olhando por cima do ombro, uma pata no teclado. Por isso é atrito de propósito, uma janela
curta de exposição, e nada do cofre sai do processo `nkw`. Uma captura feita **de fora** (o compositor da sua
sessão gravando a tela inteira) não é visível para o mundo aninhado: a defesa é o auto-trancar.

## Configuração: `world.toml`

`~/.config/nekuroddo/world.toml` (ou `$XDG_CONFIG_HOME/nekuroddo/world.toml`; com `--state-dir`, ao lado do
`world.json`). Sob `--demo` ele não é lido. Sem arquivo, valem os padrões; um arquivo quebrado também, com um
toast. Uma tabela que o mundo não conhece é ignorada.

O que o `nkw` lê hoje: `[keys]`, `[launch]` e `[[rules]]`. `[walk]` e `[camera]` (os valores abaixo são os que
já valem), `[library]` e `[vault]` estão **chegando**.

```toml
[keys]
world_mod = "ctrl+alt"      # ou "super" (o --world-mod ganha)
focus_escape = "ctrl+]"     # a saída do FOCO (o --focus-escape ganha)

[launch]
terminal = "foot"           # o terminal do T / world+enter; ou ["foot", "-e", "tmux"]

[[rules]]                   # para onde vai a janela nova de um app
app_id = "firefox"          # exato, ou um prefixo terminado em * ("org.gnome.*")
room = "pessoal"            # a sala, pelo nome
desk = "last"               # "last" ou um índice: uma mesa nova ali
width = "two_thirds"        # third | half | two_thirds | full

[walk]                      # ANDAR (chegando); fora da faixa, o valor é preso à faixa
speed = 2.8                 # m/s, 1 a 5
run_mul = 2.0               # correndo: ×1 a ×3
turn = "smooth"             # ou "snap" (o padrão com movimento reduzido)
turn_rate_dps = 110         # giro suave, 45 a 240 °/s
snap_deg = 30               # giro em passos: 15, 30 ou 45
mouse_look = true           # olhar com o mouse
mouse_deg_per_count = 0.12
comfort_vignette = false    # escurece a periferia ao andar ou girar
on_exit = "auto"            # auto | snap | stay

[camera]                    # (chegando)
eye_height = 1.5            # m, 1,2 a 1,8
pitch_max_deg = 55

[library]                   # (chegando)
roots = ["~"]               # paredes de estantes; um caminho relativo é dentro da home
show_hidden = false
exclude_dirs = []           # somam-se a .git, node_modules, target, .cache
per_shelf = 400             # até 400
search_entries = 100000     # orçamento da busca: entradas (até 100 mil),
search_depth = 12           # profundidade (até 12)
search_ms = 2000            # e tempo (até 2 s)
open_in = "origin"          # ou "library"

[vault]                     # (chegando)
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
`world.toml`: ficam no keymap do core e no seu `init.lua` (abaixo).

**(planejado)** `[render]`, `[sound]`, `[objects]` (quais objetos e onde), `[doors]` e `[notifications]`: estão
no plano ([plano-mundo-objetos.md](plano-mundo-objetos.md)); o mundo ainda não as lê e usa os padrões.

**O estado** fica em `~/.local/state/nekuroddo/world.json` (ou `$XDG_STATE_HOME/nekuroddo/`), escrito de forma
atômica 2 s depois da última mudança e na saída: salas, mesas, telas, a estação atual e a memória de lugar. Um
arquivo corrompido vira `world.json.bak`, o mundo começa limpo e avisa.

## Lua no mundo

O mundo usa o mesmo runtime Lua do core ([lua.md](lua.md)); não há um segundo.

- **`nk.keymap.set` nos modos do mundo** (`normal.world`, `world`, `world.walk`, `world.focus`, `world.talk`,
  `world.hint`, `world.palette`, `world.line`, `world.library`): o `nkw` lê as sobreposições pelo
  `GET /v1/keymap` e relê a cada `keymap.changed`, na hora.
- **Uma função numa tecla** chega ao mundo como `lua.key:<modo>:<tecla>`; ao apertar, o `nkw` pede ao core que
  rode a função (`POST /v1/commands/lua.key:…`, no orçamento de callback do Lua). O core já serve e roda essas
  teclas; o `nkw` mandá-las está **chegando**.
- **`nk.ui.scene(comando, args)`** pede ao mundo que rode um comando dele (`world.goto_room`, `world.terminal`,
  `world.goto_object`…), como se fosse uma tecla.

```lua
-- ~/.config/nekuroddo/init.lua
nk.keymap.set("normal.world", "n", "world.right")         -- n: próxima estação
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
  (`[walk] comfort_vignette`) estão **chegando**.
- **Movimento reduzido** (`--reduced-motion` ou `NKW_REDUCED_MOTION=1`): cortes em vez de deslizes, gatos em
  pose fixa, carteiros já parados, o texto da conversa inteiro de uma vez. O ANDAR continua (quem se move é
  você).

## Privacidade

- **O mundo não manda nada a lugar nenhum.** Fala só com o seu core (um socket local), com o sistema (D-Bus,
  PipeWire) e com o compositor de fora.
- **Nada da biblioteca sai do processo `nkw`**: nem para o core, nem para os agentes, nem para o `world.json`.
  Os caminhos do cofre nunca entram em comando nenhum, em `nk.ui.scene`, em `world.open` (que os recusa), na
  memória de lugar, em logs ou no `dump` de depuração.
- Os eventos do sistema não carregam identificadores: o nome da rede, dos dispositivos e o título da música só
  aparecem na etiqueta 2D, lidos na hora, e nunca são gravados nem saem no `dump`.
- A área de transferência é ligada ao host (pelo `wlr-data-control` do compositor de fora) só passando
  descritores de arquivo: o conteúdo nunca é lido, registrado nem guardado. No demo e sem janela, a ponte fica
  desligada.
- O cofre nunca registra a frase nem nomes de arquivo; hoje as linhas de auditoria ficam só na memória, e o
  arquivo `vault.log` está **chegando**.
- **Capturas e documentação usam só o demo**: `nkw --demo` não lê a home, nem o `world.toml`, nem o
  `world.json`; os objetos, o lançador e o relógio são de fixture. A biblioteca do demo (**chegando**) é uma
  fixture com `projeto/`, `notas.md`, `fotos/` e um cofre de mentira. Uma captura com o cofre aberto é
  impossível: ele tranca antes.

## Desempenho

O `nkw` é feito para ficar parado quando nada muda: o laço não arma timer nenhum quando ocioso, só redesenha
com dano, o mundo congela no FOCO, telas distantes recebem quadros numa taxa menor e o cursor é o do host. Os
orçamentos, medidos por `make world-perf` (arquivo `gui/office/scripts/world-perf-budget.toml`):

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
| `NKW_STATS=1` | uma linha JSON por quadro no stderr (`nkw-stats {…}`: tempos de update, culling, envio e composição, draws, triângulos) e, ao sair, o resumo (quadros, p50/p95, quadros perdidos, primeiro quadro, RSS) |
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
- **Um app abriu fora do mundo:** apps de instância única que já estão abertos na sua sessão (navegador, IDE) e
  diálogos de portal abrem do lado de fora — é limitação do modo aninhado. Feche o app de fora, ou abra um
  perfil separado lá dentro.
- **Um app só X11 não abre:** não há XWayland no mundo ainda **(planejado, fase 6)**.
- **O mouse não gira a câmera no ANDAR:** o olhar pelo mouse está **chegando**; mesmo com ele, o host precisa
  entregar movimento relativo com o ponteiro travado. Sem isso, olhe pelo teclado (`q`/`e`, setas) ou passe
  `--mouse-look off`.
- **Um jogo ou app que trava o ponteiro não trava dentro do mundo:** o mundo ainda não oferece
  `pointer-constraints` às janelas **(planejado, fase 6)**. Também não há `layer-shell` (barras e lançadores
  de fora), `cursor-shape`, `presentation-time` nem método de entrada (IME).
- **Texto borrado em escala fracionária (1,25, 1,5):** no FOCO a janela é desenhada em 1:1 na escala da saída
  (`fractional-scale` + `viewporter`); um app que não fala `fractional-scale` sai na escala inteira acima e é
  reduzido. O HUD do mundo desenhado nos pixels físicos está **chegando**.
- **Medir ou relatar um problema de desempenho:** `NKW_STATS=1 nkw --demo` e o resumo ao sair; `make
  world-perf` roda os cenários.

## Desenvolvimento

```sh
make world-test       # cargo test dos crates do mundo + o teste do world-perf.sh
make world-lint       # rustfmt + clippy -D warnings
make run-world        # nkw --demo direto do código
make world-shot       # as capturas de verificação (sem janela, só o demo)
make world-perf       # os cenários S1–S5 contra os orçamentos (precisa de GPU)
```

`nkw --headless --demo --script cena.nks --shot x.png` desenha sem janela, roteirizado — `key`, `text`,
`spawn`, `wait`, `shot`, `expect mode|focus|json`, `wait json`, `inject <plugin> <json>` —, e salva a captura.
Os roteiros ficam em `gui/office/scripts/scenarios/` (`S*.nks` para as medidas, `shot-*.nks` para as
capturas). O desenho por dentro está em [plano-mundo.md](plano-mundo.md) e
[plano-mundo-objetos.md](plano-mundo-objetos.md).

## Ver também

- [teclas.md](teclas.md#mundo-nkw) e [comandos.md](comandos.md#mundo--worldgo): a referência das teclas e dos
  comandos do mundo
- [lua.md](lua.md) e [protocolo.md](protocolo.md): o Lua e o protocolo que o mundo usa
- [gui.md](gui.md): o `nkg`, o escritório em Bevy (sem gerenciar janelas)
- [plano-mundo.md](plano-mundo.md) e [plano-mundo-objetos.md](plano-mundo-objetos.md): o plano e o adendo
