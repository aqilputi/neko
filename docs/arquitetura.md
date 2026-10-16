---
type: Architecture
title: Arquitetura
description: Pacotes Go do nekuroddo, o fluxo dos dados do ~/.claude até a tela, os grupos de agents e o laço de atualização.
resource: https://github.com/aqilputi/nekuroddo
tags: [arquitetura, go, bubbletea, charm, grupos]
timestamp: 2026-10-16T19:00:00Z
generated: { by: agent:claude-code/claude-sonnet-5, at: 2026-10-16T19:00:00Z }
status: stable
---

# Arquitetura

## Princípio

**Embrulhar o `claude` instalado, sem reimplementá-lo.** A lista de sessões vem de `claude agents --json`,
as sessões novas saem de `claude --bg`, e o painel da direita é um terminal virtual rodando `claude attach <id>`
de verdade. Assim, toda atualização do Claude Code chega ao nekuroddo sem mudança aqui. O nekuroddo não chama a
API da Anthropic e não exporta nada.

O mesmo princípio vale para os [grupos de agents](grupos.md): a conversa entre sessões usa o `SendMessage`
nativo do Claude Code; o nekuroddo só apresenta os pares e mostra as mensagens.

## Pacotes

| pacote | responsabilidade |
|---|---|
| `cmd/nekuroddo` | entrada: `nk` (TUI), `nk hook` (registrador de eventos) e `nk hook-context` (grupos); ver [hooks.md](hooks.md) |
| `cmd/vtprobe` | diagnóstico do painel embutido |
| `internal/claude` | fala com o CLI (`agents`, `--bg`, `stop`, `rm`, `-p /usage`) e lê transcripts, estado e uso (ver [fontes-de-dados.md](fontes-de-dados.md)) |
| `internal/store` | junta CLI e transcripts num `Snapshot` imutável: sessões, subagents, atenção, custo, arquivadas, overrides |
| `internal/events` | grava e lê `~/.local/state/nekuroddo/events.jsonl`; reconhece `<cross-session-message>` (`PeerMessage`) |
| `internal/groups` | `groups.json`, a fila de notas do `:notify` e o corpo do `nk hook-context` (ver [grupos.md](grupos.md)) |
| `internal/term` | PTY + `charmbracelet/x/vt`, filtro de OSC e codificação de teclas (ver [painel-embutido.md](painel-embutido.md)) |
| `internal/ui` | modelo Bubble Tea v2: árvore, inbox, todos, seleção e grupos, diálogos, paleta, mouse, render |
| `internal/theme` | 5 temas e a escolha automática claro/escuro |
| `internal/cat` | o gatinho `ᓚᘏᗢ` no canto inferior direito |
| `internal/config` | carga do TOML, padrões (ver [configuracao.md](configuracao.md)) e a migração dos diretórios do `neko` |
| `internal/paths` | nomes dos diretórios de config, estado e cache (XDG); sem imports internos, para o hook ficar barato |
| `internal/todo` | lê e escreve a lista **somente** via CLI da skill `todo` (que cuida de lock e escrita atômica) |
| `internal/jira` | `claude -p` headless com o conector Atlassian (ver [jira.md](jira.md)) |

## Fluxo

```
claude agents --json --all ─┐
~/.claude/sessions/*.json  ─┤
~/.claude/jobs/*/state.json ─┼─► store.Refresh ─► Snapshot ─► ui.Model ─► uv.ScreenBuffer
~/.claude/projects/**.jsonl ─┤   (goroutine própria)       ▲      ▲
events.jsonl (nk hook) ──────┘                             │      │
groups.json ───────────────────────────────────────────────┘      │
                                     term.Pane (PTY: claude attach) ┘ Emulator.Draw célula a célula
```

- **Refresh incremental:** os transcripts são lidos a partir do último offset, e uma linha ainda incompleta
  espera o próximo ciclo. Só entram arquivos modificados nos últimos `show_history_days`, com no máximo
  64 MB por arquivo.
- **Concorrência:** cada `Store` pertence a uma goroutine de cada vez (o comando de refresh). A UI só enxerga
  o `Snapshot` que ele devolve.
- **Eventos:** o `events.Tail` lê o que foi acrescentado ao `events.jsonl` a cada 400 ms. Um evento comum
  antecipa o refresh; um evento `peer` vai para `ui.onPeer` (✉ e miado).
- **Relógio único de animação:** 150 ms quando algo anima (spinner, `◆` piscando, `✉` recente) e 1 s com o gato
  dormindo.
- **Atenção** (menor = mais urgente): bloqueada (pergunta/permissão) > voltou (terminou e você não viu) >
  ociosa no prompt.

## Grupos de agents

```
ui (:group / :ungroup) ──► groups.json ◄── ui.syncGroupNames (renomeações do Claude)
ui (:notify) ────────────► mail/<sid>.jsonl
                                │
nk hook-context ◄───────────────┘  (SessionStart · UserPromptSubmit · PostToolUse)
   └─► additionalContext na sessão: pares + combinado + notas
sessão A ──SendMessage──► sessão B ──UserPromptSubmit──► nk hook ──► events.jsonl (type=peer) ──► ui
```

- `groups.json` é escrito de forma atômica (arquivo temporário + `rename`), porque o hook pode lê-lo a qualquer
  momento, e é relido a cada refresh, então dois nekuroddo abertos enxergam as mudanças um do outro.
- O hook guarda um hash do contexto por sessão (`ctx/<sid>`) para só repetir o texto quando o grupo muda.
- As notas são reivindicadas com `rename` antes da leitura, para que uma nota escrita no meio não se perca.
- Os dois hooks nunca falham para o Claude Code: em qualquer erro, saem com 0 sem imprimir nada.

## Diretórios e migração

`internal/paths` concentra os nomes: `~/.config/nekuroddo`, `~/.local/state/nekuroddo` e
`~/.cache/nekuroddo` (ou os equivalentes XDG). `config.MigrateLegacy` roda no início de toda invocação,
hooks incluídos, e renomeia os diretórios antigos do `neko` quando o novo ainda não existe. No caso comum, custa
três `os.Lstat`.

## Influências

Do estudo do [crush](https://github.com/charmbracelet/crush) vieram: tela composta num `uv.ScreenBuffer`,
`key.Binding` como fonte única de teclas, ajuda e paleta, diálogos em overlay, paleta fuzzy, toasts com `seq`,
OSC 7501 e título da janela, notificação só quando a janela está sem foco, debounce do resize do PTY e
banner de saída. A seleção com `space` segue o [yazi](https://github.com/sxyazi/yazi).

## Arquitetura alvo (planejado; fase 1 em andamento)

O plano em [plano-arquitetura.md](plano-arquitetura.md) separa o domínio hoje preso em `internal/ui` do
resto: regras puras (árvore, inbox, cor do grupo, resolução de opção) em `internal/core`, sem I/O; casos de
uso com efeitos (arquivar, disparar, grupos, Jira, todos, notificação) em `internal/app`, que fala com o mundo
só por **portas** implementadas pelos pacotes acima (`claude`, `store`, `term`, `groups`, `events`, `jira`,
`todo`). Um registro de comandos e um barramento de eventos em `app` passam a ser a face comum da TUI, de um
GUI futuro (três.js + xterm.js num shell Wails v3, com gatinhos 3D low-poly por cima da calçada) e de scripts
em Lua (`gopher-lua`). Um servidor local (`internal/rpc`, HTTP+SSE+WS sobre Unix socket) fica **opcional**:
o `nk` de hoje continua um processo só, sem socket.

**Nada disto existe em código mergeado ainda.** A fase 1 (em andamento) só move regras puras para
`internal/core` e monta o esqueleto de `internal/app`/`internal/wire`, sem mudar o comportamento da TUI atual;
GUI, Lua e o servidor são fases 2 a 4 do plano.

Árvore de pacotes alvo (resumida; tudo marcado "futuro" ainda não existe):

```
cmd/nekuroddo/      nk: TUI, nk hook, nk hook-context (hoje); futuro: nk serve, nk --listen, nk run <cmd>
cmd/nekuroddo-gui/  futuro — nkg: shell Wails v3 com o frontend em gui/web/
internal/core/      futuro — domínio puro: tipos, árvore, inbox, comando/evento como dados; sem I/O
internal/app/       futuro — casos de uso + registro de comandos + pubsub; fala só com portas
internal/wire/      futuro — raiz de composição: liga claude/store/term/... às portas de internal/app
internal/tui/       futuro — a internal/ui de hoje, afinada: teclas → comandos, eventos → redraw
internal/rpc/       futuro — servidor opcional: HTTP+SSE+WS sobre Unix socket
internal/lua/       futuro — runtime gopher-lua, API nk.* (comandos e eventos para a TUI e o GUI)
internal/{claude,store,term,groups,events,jira,todo,config,paths,theme,cat}   como hoje: adaptadores por papel
internal/archtest/  futuro — teste que falha se a regra de dependência abaixo for violada
```

Regra de dependência alvo: `internal/core` nunca importa Bubble Tea, PTY ou rede, só tipos de dados dos
pacotes acima; `internal/app` só fala com o mundo por portas; os "shells" (`tui`, `rpc`, `lua`, `guihost`) não
chamam `claude`/`store`/`groups`/`jira`/`todo` diretamente — só por `internal/app`.

```
     cmd/nekuroddo   cmd/nekuroddo-gui
           │                │
  internal/tui  internal/rpc  internal/lua  internal/guihost     ← shells (teclas/HTTP/Lua → comandos)
           │                │              │
           └───────┬────────┴──────────────┘
                    ▼
            internal/app ──► portas ◄── internal/wire ──► claude, store, term, groups, events, jira, todo
                    │
                    ▼
            internal/core   (só stdlib + tipos de dados; nunca chama funções dos pacotes acima)
```

Decisões completas, o protocolo do core, a cena do GUI, a API Lua e os pacotes de trabalho por fase (com dono
de arquivos para agents trabalharem em paralelo) estão em [plano-arquitetura.md](plano-arquitetura.md).

## Riscos

- Os formatos de `~/.claude` e o `<cross-session-message>` são internos e mudam entre versões. Por isso o
  parsing fica isolado em `internal/claude` e `internal/events`, com testes.
- `claude --bg --resume <sid>` numa sessão que ainda está rodando cria uma **cópia**. O nekuroddo só faz isso
  com sessões do histórico.
- Transcripts podem conter dados sensíveis. O nekuroddo só os lê, localmente.
