# Log — nekuroddo

Histórico de decisões e progresso. Entradas mais recentes no topo. Até a v0.1.0 o projeto se chamava `neko`,
e as entradas antigas mantêm o nome da época.

## 2026-10-16

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
