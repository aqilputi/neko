# neko 🐈‍⬛

Orquestrador TUI das suas sessões do Claude Code, em Go com Charm (Bubble Tea v2, Lip Gloss v2, Bubbles).
Um gatinho preto (`ᓚᘏᗢ`) dorme no canto inferior direito e acorda para miar quando um agent volta.

**Embrulha o `claude` instalado, não reimplementa**: a lista vem de `claude agents --json`, sessões novas
saem de `claude --bg`, e o painel da direita é um terminal virtual (PTY + `charmbracelet/x/vt`) rodando
`claude attach <id>` de verdade. Toda atualização do Claude Code chega no neko sem mudar nada aqui.

```fish
neko                      # ~/.local/bin/neko
# build: cd ~/dev/neko; ~/.local/sdk/go/bin/go build -o ~/.local/bin/neko ./cmd/neko
```

## Teclas (vim)

| tecla | ação |
|---|---|
| `j` `k` `gg` `G` `ctrl+d/u` | navegação |
| `h` `l` `space` | fecha / abre / alterna nó (contexto → sessão → subagents) |
| `a` `A` | pula para a próxima / anterior sessão te esperando |
| `1` `2` `3` / `tab` | Sessões · Inbox · Todos |
| `/` | filtro fuzzy (esc limpa) |
| `H` | mostra/esconde sessões paradas (> `stale_days`) e arquivadas |
| `S` | ordem: atividade → custo → nome |
| `X` | arquiva / desarquiva a sessão (some do inbox e da árvore) |
| `enter` | abre a sessão no painel e foca (INSERT) |
| `o` / `O` | abre sem focar / tela cheia (`←` dentro do claude volta) |
| `i` / `ctrl+]` | entra / sai do INSERT (as teclas vão pro claude) |
| `r` | resposta rápida — já vem com o `suggestedReply`; um número sozinho escolhe a opção da pergunta |
| `c` | nova sessão em background (diretório → prompt) |
| `tab` (ao disparar) | modo de permissão do agent: padrão → acceptEdits → auto → plan → ⚠ bypass (`--permission-mode`; bypass pede confirmação e volta ao padrão depois) |
| `x` / `dd` | `claude stop` / `claude rm` (pede confirmação) |
| `w` / `z` | fecha o painel / zoom |
| Todos: `enter` | dispara um agent para a tarefa (no repo do `@contexto`), marca `start` e anota o id |
| Todos: `s` / `D` | start / done |
| Todos: `J` | todo → cria issue no Jira (só quando você pede, com confirmação) · issue → vira todo |
| Todos: `F` / `o` | busca suas issues agora (~$0,05–0,12) / abre a issue no browser |
| `:` / `ctrl+p` | paleta de comandos fuzzy: qualquer ação, pular para sessão/tarefa, trocar tema |
| `T` `R` `?` `q` | tema · atualizar · ajuda · sair (pede confirmação; `ctrl+c` duas vezes sai direto) |

**Mouse:** clique seleciona, clique de novo abre; a roda rola a lista. Sobre o painel do claude, roda, clique e
arraste vão direto para ele (o renderizador fullscreen do Claude Code rola o chat e seleciona texto sozinho; o que
ele copia chega ao seu clipboard via OSC 52). Clicar no painel também entra no INSERT, e o cursor do claude vira o
cursor real. No renderizador clássico (sem mouse), a roda e `shift+pgup/pgdown` rolam o scrollback.

**Painel embutido — por que funciona assim:** o `x/vt` v0.1.0 aborta um OSC no primeiro byte não-ASCII e imprime o
resto como texto; o título que o Claude define (`✳ <resumo da tarefa>`) aparecia como texto fantasma no prompt.
O neko filtra os OSC antes do emulador (`internal/term/osc.go`): o título vira o subtítulo do painel, OSC 52 vai
para o clipboard, OSC ASCII passa. `go run ./cmd/vtprobe -- claude --model haiku` reproduz uma sessão real no
painel e despeja tela, cursor, título e clipboard a cada passo.

## Eventos em tempo real (hooks)

`neko hook` está instalado em `~/.claude/settings.json` (async, ~3 ms, não imprime nada, sempre exit 0) nos
eventos Notification, Stop, SubagentStop, UserPromptSubmit, SessionStart e SessionEnd. Cada evento vira uma linha em
`~/.local/state/neko/events.jsonl`; o neko lê o arquivo a cada 400 ms e atualiza na hora. O gato mia com a mensagem
do Claude ("Claude needs your permission…"), e o `↯` na barra de status indica que os hooks estão chegando. Para desligar,
remova as entradas `neko hook` em `/hooks` (o backup é `~/.claude/settings.json.bak-neko-*`).

## Jira

O neko **não guarda token**. Leitura e escrita vão por um `claude -p` headless que usa o conector
Atlassian do claude.ai (`ENABLE_CLAUDEAI_MCP_SERVERS=true` + `MCP_CONNECTOR_PREWAIT_MS`), com uma única
ferramenta liberada. O neko lê o **resultado cru da ferramenta** no `stream-json`, sem confiar no texto do modelo:
num teste, o Haiku respondeu `[]` sem ter o conector carregado. O hook do guarded-access continua valendo:
criar issue exige abrir a janela de escrita do Jira na skill guarded-access (comando humano), e o neko mostra
o comando quando o guard bloqueia. As issues ficam em cache em `~/.cache/neko/jira.json` (`cache_minutes`).
O vínculo com o todo é `--ref jira:KEY` + tag `jira`.

## Config — `~/.config/neko/config.toml`

Veja `config.example.toml`. Regras de contexto por regex no cwd, preços por modelo, escala da barra de custo,
orçamento diário, mapa `@contexto → diretório` para os todos.

## Mapa de requisitos

Levantado a partir do uso real (30 dias até 15/10/2026): 82 sessões, ~$2.558 (~$85/dia), 53% Visio,
1–6 sessões simultâneas, 156 subagentes (19% do custo), espera por você com mediana 2,8 min e p90 54 min,
bloqueios quase sempre por pergunta/decisão (bypass em 83% das sessões), 100% pt-BR.

### Fontes de dados (somente leitura)

| fonte | o que dá |
|---|---|
| `claude agents --json --all` | sessões vivas (interativas e bg) + bg concluídas; id curto para `attach` |
| `~/.claude/sessions/<pid>.json` | status real da interativa (`busy·shell·idle·waiting`) e `waitingFor` — o `.key` ao lado **nunca** é lido |
| `~/.claude/jobs/<id>/state.json` | bg: `detail`, `needs`, `suggestedReply`, `block.questions[].options`, `children` (PRs/artifacts) |
| `~/.claude/projects/<cwd>/<sid>.jsonl` | custo (usage por `requestId`, dedup), título (`ai-title`), últimas mensagens, tool pendente |
| `<sid>/subagents/agent-*.jsonl` + `.meta.json` | árvore de subagents: descrição, tipo, modelo, custo |
| `todo.py list --format json` | a lista de tarefas (escrita só pela CLI da skill) |

Custo = input·in + output·out + cache 5m·1,25in + cache 1h·2in + cache read·cr, por request, somando subagents.
Fica ~5–15% abaixo do `cost-state` oficial (chamadas internas que não vão pro transcript).

### Status

| | requisito | estado |
|---|---|---|
| MUST | lista unificada (interativas + bg + histórico de N dias) por contexto | ✅ |
| MUST | inbox ordenado (bloqueado > voltou > no prompt) com `needs`, perguntas e opções, resposta sugerida | ✅ |
| MUST | resposta rápida para bg (attach no PTY + paste + enter) | ✅ |
| MUST | árvore sessão → subagents com modelo e custo | ✅ |
| MUST | barra de custo por sessão e por contexto, gasto de hoje, preços configuráveis | ✅ |
| MUST | contextos automáticos por cwd (visio/pessoal por tema) | ✅ |
| MUST | refresh incremental (tail por offset, linhas parciais seguras) | ✅ |
| MUST | vim, temas (5), gatinho, notificação (sino + `notify-send`) | ✅ |
| MUST | Claude Code embutido (painel PTY e tela cheia) | ✅ |
| — | todos: ver, start/done, disparar agent no repo certo | ✅ |
| MUST | sessões paradas (> `stale_days`) fora do inbox, ocultas por padrão; arquivar à mão; ordenar | ✅ |
| — | Jira: ver minhas issues, issue → todo, todo → issue (opt-in), agent por issue | ✅ (criar issue ainda não testado de ponta a ponta) |
| SHOULD | responder sessões **interativas** abertas em outro terminal | ❌ sem API pública (socket privado). Caminhos: abrir as sessões pelo neko (`c`), ou kitty remote control (`allow_remote_control`) |
| SHOULD | eventos via hooks (`Notification`/`Stop`/`SubagentStop` → `~/.local/state/neko/events.jsonl`) para latência < 1 s | ✅ (polling de 2 s continua como rede de segurança) |
| SHOULD | mensagens enfileiradas (`queue-operation`), barra de contexto (tokens/1M), `rate-limited` com countdown | ⏳ |
| SHOULD | busca full-text (`history.jsonl`), visão semana por contexto | ⏳ |
| SHOULD | barra de custo empilhada (cache read / write / output — 51% / 38% / 12% do gasto) | ⏳ |
| COULD | override manual de contexto por sessão, renomear sessão, timeline/replay, modo SDK host (stream-json) | ⏳ |
| COULD | índice persistido em `~/.cache/neko` para não reler 346 MB na partida | ⏳ hoje lê só o que mudou nos últimos N dias, máx. 64 MB por arquivo |

### Do estudo do crush (aplicado)

Tela composta num `uv.ScreenBuffer` com o painel do claude copiado célula a célula (`Emulator.Draw`, com lock
próprio porque o `SafeEmulator` não protege o `Draw`); relógio de animação único (150 ms quando algo anima, 1 s
com o gato dormindo); `key.Binding` como fonte única de teclas/ajuda/paleta; diálogos em overlay; paleta fuzzy;
mouse; toasts com `seq`; OSC 7501 + título da janela; notificação só sem foco; debounce do resize do PTY; tema
automático claro/escuro; filtro fuzzy; banner de saída; sem emoji em colunas alinhadas.

Falta: arquivo de tema do usuário (`~/.config/neko/themes/*.toml`) e estilos pré-construídos por tema.

### Riscos

- Os formatos de `~/.claude` são internos e mudam entre versões; o parsing está isolado em `internal/claude`, com testes.
- `claude --bg --resume <sid>` numa sessão que já está rodando cria uma **cópia**: o neko só usa isso para sessões do histórico.
- Sessões em `~` misturam trabalho e pessoal; a regra por cwd erra ali (falta o override manual).
- Transcripts têm dados de clientes. O neko não exporta nada e não chama a API.
