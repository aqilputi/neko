# Changelog

Todas as mudanças relevantes deste projeto ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e o projeto adota o
[Versionamento Semântico](https://semver.org/lang/pt-BR/). Enquanto a versão for `0.x`, mudanças
incompatíveis podem acontecer em versões menores e serão sempre descritas aqui.

## [Não lançado]

### Alterado

- **Início da refatoração de arquitetura.** Plano para separar o domínio de `internal/ui` num core headless
  (`internal/core` + `internal/app`, falando só por portas), com um GUI opcional (gatinhos 3D low-poly,
  terminais reais embutidos) e scripts em Lua, documentado em
  [docs/plano-arquitetura.md](docs/plano-arquitetura.md). Fase 1 (mover regras puras para `internal/core` e
  montar o esqueleto de `internal/app`) em andamento; comportamento, teclas e atalhos continuam os mesmos.
- `internal/ui` fatiada por assunto (sessões, painéis, entrada/disparo, Jira/todos, avisos, navegação) e
  ligada a `internal/core` (árvore, inbox e helpers) e a `internal/app` (um só store e log de eventos, via
  `internal/wire`), sem mudança de comportamento.
- Arquivar, parar, remover, renomear e mover sessões passam a ser casos de uso de `internal/app`
  (comandos `session.archive`, `session.stop`, `session.remove`, `session.rename`, `session.move`,
  `sessions.archive_stale`, `sessions.remove_ended`, com confirmação por token); a TUI só traduz.
- Disparo de agents (`app.Dispatch`, comando `session.new`) no core; a confirmação do bypass vira um token
  respondido por `app.Confirm`/`app.Cancel`, sem mudança de comportamento.
- Avisos em `internal/app` (`app/watch.go`): transições de estado, eventos de hook, notificação no desktop
  (porta `Notifier`) e limites do `/usage`, publicados como `session.state`, `session.notification`,
  `notice` e `usage.updated`; a TUI só mia, toca o sino e mostra o toast.
- Painéis no core (`app.PaneManager`): qual painel é de qual sessão, reaproveitar, retomar histórico,
  resposta pendente e tela cheia (`Result{Exec}`), com eventos `pane.*` e comandos `session.open`,
  `session.preview`, `session.fullscreen`, `pane.close`, `pane.zoom`; a TUI só sobe e desenha o painel.
- Grupos no core (`app/groups.go`): agrupar, tirar, desfazer, avisar o grupo, remetente das mensagens e
  cues de ✉ pelo `Clock`, com eventos `group.changed`, `peer.message`, `mail.cue` e comandos `group.add`,
  `group.remove`, `group.notify`, `group.dissolve`; a TUI só escolhe sessões, lê a paleta e desenha.
- Todos e Jira no core (`app/todo.go`, `app/jira.go`): carregar, start/done, issue → todo, todo → issue com
  confirmação e `LinkJira`, cache do Jira com validade e uma busca por vez, abrir no browser (porta
  `Opener`), com eventos `todos.changed`, `jira.changed` e comandos `todo.start`, `todo.done`,
  `jira.toggle`, `jira.fetch`, `jira.open_url`; a TUI só monta as linhas da aba e traduz.
- Fim da fase 1: todo o estado de domínio vive no `App` (sem espelhos na TUI), todos os comandos nativos
  são registrados na partida (`app.RegisterBuiltins`), o refresh passa por `app.RefreshNotices` (transições
  e nomes dos grupos para qualquer cliente) e `internal/archtest` garante que a TUI só fala com o mundo por
  `internal/app`; sem mudança de comportamento.

## [0.1.0] - 2026-10-16

Primeira versão marcada. Até aqui o projeto se chamava `neko`.

### Adicionado

- **Grupos de agents.** `space` seleciona sessões (como no yazi; numa linha de contexto continua dobrando a
  seção, `esc` limpa); a paleta aceita `group <tag>`, `ungroup [tag]` e `notify #tag <mensagem>`. Membros
  mostram a barra `▍` e o chip `#tag` na cor do grupo, e os pares da sessão sob o cursor ganham `┃`.
- `nk hook-context`, hook **síncrono** (`SessionStart`, `UserPromptSubmit`, `PostToolUse`) que conta a cada
  membro quem são os pares e o combinado do grupo, só quando o grupo muda, e entrega as notas do `:notify`.
- Detecção das mensagens entre sessões (`SendMessage` do Claude Code) no `UserPromptSubmit`: `✉→` no
  remetente, `✉←` nos destinatários (piscando 90 s, apagado até 15 min), o gato mia `✉ A → B` e o painel de
  detalhes lista as últimas mensagens.
- Comando `nk` (link simbólico para o binário `nekuroddo`) e migração automática dos diretórios de dados do
  `neko` (`~/.config`, `~/.local/state`, `~/.cache`) na primeira execução.
- Logo e wordmark em `assets/`.
- `Makefile` com `build`, `install`, `test`, `cover`, `cover-html`, `lint`, `fmt`, `vet` e `ci`;
  configuração do `golangci-lint`; CI no GitHub Actions; Dependabot.
- `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, templates de issue e de pull request.
- Renomear sessão ou seção inteira (`n`) e mover uma sessão para outro contexto (`m`), guardados como
  overrides locais.
- Descrição das issues do Jira no detalhe e no prompt do agent disparado para a issue.
- Bundle de documentação [OKF 0.2](https://okf.md/spec/) em `docs/`, incluindo o guia de
  [grupos](docs/grupos.md); licença MIT.
- Recursos da primeira versão funcional:
  - árvore contexto → sessão → subagents, com estado, modelo e custo, a partir de `claude agents --json` e
    dos transcripts (leitura incremental por offset);
  - inbox de quem está te esperando (bloqueada > voltou > ociosa), com pergunta, opções e resposta sugerida, e
    resposta rápida (`r`);
  - Claude Code embutido num painel ou em tela cheia: PTY + `charmbracelet/x/vt` rodando `claude attach`, com
    mouse, scroll e clipboard via OSC 52;
  - sessões novas em background (`c`) com escolha do modo de permissão;
  - eventos em tempo real via `nk hook` (antes `neko hook`) e polling como rede de segurança;
  - barra de uso da janela de 5 h e da semana (`claude -p /usage`), custo estimado por modelo com preços
    configuráveis e orçamento diário;
  - teclas de vim, paleta fuzzy (`:`), filtro (`/`), 5 temas com escolha claro/escuro automática, ajuda
    gerada das próprias teclas;
  - arquivar (`X`), sessões paradas ocultas (`H`), ordenação (`S`), pular para chats vivos (`ctrl+k`);
  - aba Todos (skill `todo`) com agent disparado no repo do `@contexto`, e Jira sem token via `claude -p` +
    conector Atlassian;
  - o gatinho `ᓚᘏᗢ`, notificação por sino e `notify-send`, título da janela e OSC 7501;
  - `cmd/vtprobe` para diagnosticar o painel embutido.

### Alterado

- **Nome do projeto: `neko` → `nekuroddo`.** Módulo `github.com/aqilputi/nekuroddo`, entrada em
  `cmd/nekuroddo`, binário `nekuroddo` com o atalho `nk`, dados em `~/.config/nekuroddo`,
  `~/.local/state/nekuroddo` e `~/.cache/nekuroddo`. Os hooks no `~/.claude/settings.json` precisam passar
  de `neko hook` para `nk hook`.
- `space` agora seleciona sessões em vez de alternar o nó; numa linha de contexto continua dobrando a seção.
- Layout: a lista ocupa 1/3 da largura e o painel do claude, 2/3. A barra de uso por sessão tem 4 células com
  precisão de 1/8, e o nome do modelo aparece sem a data.
- Nomes contextualizados: a lista nunca mostra um hash como nome de sessão ou subagent.
- README, guias de instalação, teclas, hooks e arquitetura reescritos.

### Corrigido

- `J` num todo não criava a issue no Jira quando o `defaultMode` era `auto`: o classificador negava a escrita
  ("External System Writes") e o erro era atribuído ao hook. O `claude -p` agora roda com
  `--permission-mode default`, a negação mostra o motivo real e a última chamada fica em
  `~/.cache/nekuroddo/jira-last.jsonl`.
- O `.gitignore` ignorava `cmd/neko/` por engano (padrão sem âncora).
- `h` numa linha de subagent derrubava o app (nil pointer) quando havia outro contexto abaixo dela.
- Um `match` inválido em `[[context]]` no `config.toml` derrubava o app; agora vira erro com a regra e o
  nekuroddo segue com os padrões.
- Abaixo de ~118 colunas o cabeçalho quebrava em duas linhas e empurrava a barra de status para fora da tela.
- Perguntas curtas no diálogo de confirmação quebravam a borda da caixa.
- O custo dos subagents saía zerado num segundo leitor do mesmo transcript (offsets num mapa global, sem lock).
- Valores específicos de organização saíram do código e do exemplo de configuração (`approve_hint` sem padrão;
  teste ao vivo do Jira configurado por variável de ambiente).

[Não lançado]: https://github.com/aqilputi/nekuroddo/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/aqilputi/nekuroddo/releases/tag/v0.1.0
