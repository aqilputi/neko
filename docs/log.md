# Log — nekuroddo

Histórico de decisões e progresso. Entradas mais recentes no topo. Até a v0.1.0 o projeto se chamava `neko`,
e as entradas antigas mantêm o nome da época.

## 2026-10-23

- **progresso:** onda 3 do mundo (`nkw`), sete pacotes em paralelo:
  - **arte:** salas com teto de vigas, duas janelas com cortina e luz que segue a hora do dia, papel de
    parede e piso por sala, ON AIR corrigido, pescoço do monitor mais grosso, recepção recomposta.
  - **ANDAR:** mouse-look pelo fork `aqilputi/smithay` (`nkw/winit-relative-pointer`, patch de 60 linhas em
    `gui/office/patches/`), mira, carregar telas e soltar na mesa, na parede ou no ar, teclas seguradas
    religáveis, vinheta de conforto opcional, `[walk]`/`[camera]` no `world.toml`.
  - **gatos:** caixa de diálogo completa (o texto não corta mais), gatos andando em quatro patas, chegam
    bem enquadrados e lado a lado, viram a cabeça a 2,5 m e ronronam a 1,3 m, a bibliotecária.
  - **biblioteca e cofre:** estantes, busca com orçamento, atril, abrir com intenção de lugar; o cofre na
    biblioteca com a bibliotecária, frase digitada, trava automática, auditoria 0600 e `vault_never_leaves`.
  - **core:** teclas do Lua no `/v1/keymap` e `lua.key` remoto; `make install-world` e o atalho
    "nekuroddo — mundo"; guia em [mundo.md](mundo.md).
- **medida** (aninhado no niri, máquina ociosa): S1 0,02 % de CPU; S3 quadro p50 1,08 / p95 1,47 ms (antes
  4,3); S4 p50 1,54 / p95 2,27 ms e 1 quadro perdido (acima do alvo); S5 1 perdido; RSS 97–110 MB (biblioteca
  123 MB); binário 14,1 MB.
- **decisão:** orçamento de RAM na partida de 45 para 90 MB no [plano](plano-mundo.md) (§1.3).
- **pendências:** S4 p50 ≤ 1 ms (passar o dano do overlay à composição); 1 quadro perdido em S4/S5;
  crachás de gatos de outras salas aparecem através das paredes; a câmera fica perto demais das estantes;
  unificar a `Policy` do cofre entre `nkw-objects` e `nkw_core::library`; abrir o PR do patch no Smithay.

## 2026-10-22

- **progresso:** onda 2 do mundo (`nkw`), seis pacotes em paralelo sobre os contratos congelados:
  - **WM:** operações da faixa e presets, pilhas, telas no ar, diálogos na mesa da mãe, tela cheia como TV,
    xdg-activation, `world.json` com regras e memória de lugar, ponte da área de transferência com o host,
    lançador `.desktop`.
  - **interface:** caixa de diálogo no estilo Animal Crossing com texto letra a letra, HUD com selos,
    toasts, SALTO, COMANDOS, crachás e mapa; router com os modos do keymap.
  - **gatos:** aldeões por sessão nas mesas, com o rig do `nkw-gl` e pelagem por id; monitores com tail ou
    painel ao vivo; CONVERSA ligada aos comandos do core; inbox com A\*. Aceite da fase 2 verde contra
    `nk serve --demo` ("1" responde o `api-gateway`).
  - **objetos e áudio:** vitrola, relógio e faixa do dia, bateria, planta, portas com estado (colisão,
    culling, som, auto-fechar), áudio por proximidade v1 e o cofre. Tick do plugin ≈ 6 µs.
  - **modelos:** 41 malhas novas, gatos aldeões, filhotes e os móveis dos objetos.
  - **infra:** dano real da textura do mundo e upload só do que é visível (S4 p95 de 4–6 para 3 ms com a
    máquina carregada); `wp_fractional_scale_v1` (FOCO pixel-exato a 1,25); cenários S3, S3r e S5; patch
    de 60 linhas no backend winit do Smithay para movimento relativo, como proposta.
- **decisão:** orçamento de RAM na partida de 45 para ~90 MB: o driver Mesa sozinho ocupa 48 MB
  (`NKW_MEM=1`); continua ≤ 110 MB com 8 janelas.
- **medida:** `world-perf.sh --headless --seconds 30` com a máquina ociosa (perfil `world`): binário 13,6 MB;
  S1 0 % de CPU e 0,13 wakeups/s; S2 0,17 % e tecla com p95 de 0,25 ms; S3 0,37 % (quadro p50 0,94 ms, p95
  4,3 ms, acima dos 2 ms); S3r 0,03 %; S4 p50 1,19 ms, p95 1,9 ms, 0 quadros perdidos; S5 0 perdidos; RSS de
  95 a 107 MB.
- **pendências:** S3 p95 e S4 p50 acima do alvo; tick ocioso com p95 de 1,9 ms; aplicar o patch
  de movimento relativo (mouse-look); "N" espelhado no ON AIR; a sala não tem teto; gatos andando usam o rig
  sentado; overrides de tecla do Lua ainda não chegam ao `/v1/keymap`.

- **progresso:** fase 0 do mundo (`nkw`, [plano-mundo.md](plano-mundo.md)) fechada. Medida aninhada no niri
  (Iris Xe), `world-perf.sh` com janelas de 60 s, binário `--profile world`:
  1. foot, um app GTK4 e o mpv (dmabuf) aparecem nos quads e digitam no FOCO, com tecla morta ✓;
  2. FOCO pixel-exato, 0 px diferentes (headless e aninhado) ✓;
  3. S1 ocioso: 0 % de CPU, 0 wakeups/s ✓;
  4. S4: p95 de 1,87 ms por quadro (p50 1,14), 0 quadros perdidos, 12 draws, GPU 2,1 ms ✓;
  5. S2: 0,18 % de CPU a 15 teclas/s, tecla no cliente com p95 de 0,25 ms ✓;
  6. 2D sobre o passe 3D conferido por shot, sem `glGetError` com `--gl-debug` ✓;
  7. binário de 7,4 MB, build a frio de 2 min 43 s ✓.

  Decisão: seguir. Pendências: RSS de 75 MB na partida (orçamento 45 MB), mouse-look no ANDAR depende de
  movimento relativo no backend winit, p50 acima de 1 ms por reenviar o buffer shm inteiro, escala
  fracionária ainda não exata.
- **progresso:** crates do mundo: `nkw-core` (cérebro puro), `nkw-gl` (renderizador), `nkw` (compositor
  Smithay), `nkw-testclient`, `nkw-objects` (acústica, portas, cofre, objetos), `nkw-sys` (D-Bus, relógio,
  PipeWire) e `nk-cats` (módulos puros dos gatos, extraídos do `nkg`). O `nk-client` não acorda mais quando
  ocioso (painel e keeper de ~130 para 0 wakeups/s).
- **decisão:** o GUI nativo deixa o navegador: `nkg` em Rust + Bevy ([gui.md](gui.md)); o web vira legado.

## 2026-10-16

- **documentação:** [gui.md](gui.md) sai de "planejado/em construção" para **implementado** — a Fase 3 do GUI
  fechou enquanto o guia ainda descrevia o esqueleto. Reescrito contra o código real desta vez (não o plano):
  `cmd/nekuroddo-gui` existe e `make run-gui-browser` abre de verdade (`vite build` + `nkg --browser --demo
  --dev`, `xdg-open`, link de uso único); `internal/guihost` serve o frontend e faz proxy de `/v1`
  (embutido, ou `--connect unix://…`/`tcp://…` para um `nk serve`/`nk --listen`, ou `--listen` para o
  caminho contrário); `gui/web/src/{scene,inbox,windows,chrome,keymap}` estão todos montados em `App.tsx`,
  não só em módulos soltos com teste. Bundle medido com `make gui-build`: 291 KB gzip de JS + 4,7 KB de CSS
  (meta do plano era 1 MB). Achado que corrige o guia anterior: `cmd/nekuroddo-gui/host.go` agora declara
  `guiCaps` com `"scene"` para o core que o próprio `nkg` embute — `nk.ui.has("scene")` finalmente responde
  `true`, mas só nesse caso; via `--connect` a um `nk serve` (`serveCaps`) ou a uma TUI com `--listen`
  (`tuiCaps`), nenhum dos dois ganhou `"scene"`, então continua `false` ali. A janela nativa (Wails v3
  beta.30 + WebKitGTK 6.0) está implementada e compila sob a tag `wails`, mas **não foi verificada em
  hardware** — nenhuma máquina usada até agora tinha o WebKitGTK 6.0 instalado; o guia diz isso sem
  rodeio em vez de presumir que funciona. Seção do GUI em [instalacao.md](instalacao.md#gui-nkg) atualizada
  junto (o pacote do Debian/Ubuntu ganhou `libgtk-4-dev`, que faltava).
- **documentação:** fase 3 (WP-6.6): [gui.md](gui.md), o guia de uso do GUI (`nkg`) — como rodar hoje (o
  frontend num navegador comum, contra `nk serve --demo`, já que `cmd/nekuroddo-gui` ainda não existe) e como
  está planejado (`nkg --browser`/`--demo`/`--connect`, WebKitGTK por distro); a cena, o gato da inbox (com a
  receita de atalho do compositor para `nk run inbox.summon`, Hyprland e KDE), janelas, teclado, Lua
  (`nk.ui.has("scene")`), acessibilidade, orçamento de desempenho e solução de problemas de WebGL. Seção nova
  em [instalacao.md](instalacao.md#gui-nkg), linha em [index.md](index.md) e roadmap do README atualizados.
  Escrito contra o código real das outras frentes do GUI que rodam em paralelo nesta fase: `gui/web/src/
  {client,state}` (WP-6.1) prontos; `inbox` (WP-6.4) já exporta `InboxWalk`/`installInboxKeys` pelo contrato;
  `scene` e `keymap` têm peças internas em código e teste (modelo de cena, resolução de tecla, navegação) mas
  ainda sem o `index.ts` do contrato; `chrome`/`windows` só o esqueleto; `internal/guihost` (host HTTP, proxy
  de `/v1`, login por link de uso único) já funcional como pacote; `cmd/nekuroddo-gui` (o binário `nkg`) ainda
  não existe. Cada seção do guia marca o estado real da peça que descreve, em vez de um GUI que ainda não roda.
- **documentação:** onda 3 (WP-3.5): [protocolo.md](protocolo.md) (HTTP+JSON, SSE e WebSocket de
  `internal/rpc`, erros e `nk serve`/`--listen`/`run`), [lua.md](lua.md) (API `nk.*` de `internal/lua`,
  eventos, orçamentos, sandbox, reload) e [comandos.md](comandos.md) (todo comando do registro, com escopo,
  teclas e argumentos), escritos a partir do código das ondas 2 e 3 (`internal/rpc`, `internal/lua`,
  `internal/app`), não só do plano. Achados: `internal/rpc` e `internal/lua` já estão ligados de verdade em
  `cmd/nekuroddo` (`nk serve`, `nk --listen`, `nk run`, `lua.Load()` no arranque) — mais adiantado que a fase 1
  do plano previa —, mas nenhum host ainda declara `Options.Caps` do Lua (toda `nk.ui.*` continua sem efeito) e
  `POST /v1/lua/eval` segue `501` (só `:lua` local na TUI). `proto.CodeConflict` está declarado e nunca emitido
  (`ErrUnavailable` vira `409` com `code: "unavailable"`, não `"conflict"`).
- **pesquisa + plano:** arquitetura alvo para separar o domínio de `internal/ui` em `internal/core` (regras
  puras, sem I/O) e `internal/app` (casos de uso, fala só por portas), com um registro de comandos e um
  barramento de eventos comuns à TUI, a um GUI novo (three.js + xterm.js num shell Wails v3, gatinhos 3D
  low-poly) e a scripts em Lua (`gopher-lua`); servidor local (`internal/rpc`) opcional. Trabalho dividido em
  fases e pacotes de trabalho (WP) com dono de arquivos, para um time de agents rodar em paralelo sem
  conflito, guiado por TDD e por um teste de arquitetura (`internal/archtest`) que trava a regra de
  dependência. Plano completo em [plano-arquitetura.md](plano-arquitetura.md); resumo e diagramas em
  [arquitetura.md](arquitetura.md#arquitetura-alvo-planejado-fase-1-em-andamento). Fase 1, onda 1
  (`internal/core`, `internal/app`, `internal/term` sem Bubble Tea e esta documentação) em andamento; nada do
  GUI ou do Lua existe em código ainda.
- **lançamento:** v0.1.0, a primeira versão marcada ([notas](releases/v0.1.0.md), `CHANGELOG.md` no formato
  Keep a Changelog).
- **decisão:** o projeto passa a se chamar **nekuroddo**, com o comando curto `nk` (link simbólico para o
  binário `nekuroddo`). Módulo `github.com/aqilputi/nekuroddo`, entrada em `cmd/nekuroddo`. Os diretórios
  `~/.config/neko`, `~/.local/state/neko` e `~/.cache/neko` migram sozinhos na primeira execução, hooks
  incluídos, sem sobrescrever nada ([instalacao.md](instalacao.md#vindo-do-neko)). O gato `ᓚᘏᗢ` continua.
- **progresso:** [grupos de agents](grupos.md). `space` seleciona sessões (como no yazi), `:group`/`:ungroup`/
  `:notify` na paleta, barra `▍` e chip `#tag` na cor do grupo. Os membros ficam sabendo dos pares pelo hook
  síncrono `nk hook-context` e se avisam com o `SendMessage` nativo do Claude Code; o `nk hook` reconhece a
  `<cross-session-message>` no `UserPromptSubmit` e o nekuroddo mostra `✉→`/`✉←` e o miado.
- **decisão:** o nekuroddo não transporta as mensagens entre sessões. Usar o `SendMessage` do Claude Code
  mantém o princípio de embrulhar o `claude` e deixa as permissões da sessão valendo; o combinado do grupo diz
  que avisos de pares informam, mas não substituem as instruções do usuário.
- **progresso:** logo (`assets/`), `Makefile` (build, install, test, cover, lint), `golangci-lint` e CI no
  GitHub Actions; documentação reescrita (README, [hooks.md](hooks.md) com `hook-context`, `CONTRIBUTING.md`,
  `SECURITY.md`, templates de issue e PR).

## 2026-10-15

- **correção:** `J` num todo não criava a issue. Com `defaultMode: auto`, o classificador negava o
  `createJiraIssue` ("External System Writes") e o neko atribuía a negação ao guard. O `claude -p` agora roda com
  `--permission-mode default`, a negação mostra o motivo real e a última chamada fica em
  `~/.cache/neko/jira-last.jsonl` ([jira.md](jira.md#permissões-e-escrita-negada)).
- **progresso:** renomear sessão/seção (`n`) e mudar sessão de contexto (`m`); nomes nunca mostram hash; layout
  1/3 lista · 2/3 painel; descrições das issues do Jira no detalhe e no prompt do agent.
- **publicação:** repositório público em github.com/aqilputi/neko, documentação reorganizada como
  bundle OKF 0.2 em `docs/`, licença MIT. Módulo renomeado para `github.com/aqilputi/neko`.
  O `.gitignore` ignorava `cmd/neko/` por engano (padrão `neko` sem âncora) e foi corrigido para `/neko`.
  Valores específicos de organização saíram do código e do exemplo: `approve_hint` agora não tem padrão e o
  teste ao vivo do Jira lê `NEKO_JIRA_SITE`.
- **progresso:** hooks em tempo real (`neko hook` → `events.jsonl`), [Jira](jira.md) sem token via
  `claude -p` + conector, paleta `:`, diálogos em overlay, mouse, arquivar (`X`), sessões paradas ocultas (`H`).
- **decisão:** painel embutido = PTY + `x/vt` rodando `claude attach`, sem reimplementar o claude. Bugs do
  `x/vt` v0.1.0 (OSC não-ASCII, teclas com modificadores) contornados em `internal/term`
  ([painel-embutido.md](painel-embutido.md)).
- **decisão:** embrulhar o `claude` instalado em vez de usar o SDK, para receber as atualizações do Claude Code
  sem mudança no neko ([arquitetura.md](arquitetura.md)).
- **pesquisa:** [mapa de requisitos](requisitos.md) levantado de 30 dias de uso real.
