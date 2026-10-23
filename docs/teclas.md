---
type: Reference
title: Teclas e mouse
description: Mapa completo de teclas (estilo vim), o comando por trás de cada uma, comandos da paleta e comportamento do mouse no nekuroddo.
resource: https://github.com/aqilputi/nekuroddo/blob/main/internal/app/commands.go
tags: [teclas, vim, mouse, paleta, ui, comandos]
timestamp: 2026-10-16T23:30:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T23:30:00Z }
status: stable
---

# Teclas e mouse

Cada tecla resolve para um **comando** com nome (`session.archive`, `nav.down`…), do mesmo registro que
alimenta a paleta, o Lua e o protocolo do core (ver [plano-arquitetura.md](plano-arquitetura.md), §2.4 e §2.7).
As teclas vêm dos próprios comandos (`Keys`/`Mode` de cada `core.Command`); `app.Keymap()` monta com elas o
`core.Keymap`, e a TUI desenha a barra de status e a ajuda (`?`) a partir dele. `internal/tui/keys.go` guarda só
a apresentação: a ordem da barra e das colunas da ajuda e os rótulos curtos.

O modo `normal.todos` vale na aba Todos: as teclas dele têm prioridade, e o resto cai no `normal`. Os
comandos de escopo `client` (navegação, abas, vista, diálogos) são resolvidos por cada cliente; os demais
rodam no core com a seleção do cliente (sessão do cursor, selecionadas com `space`, aba, tarefa ou issue).

## Lista e navegação

| tecla | ação | comando |
|---|---|---|
| `j` `k` `gg` `G` | desce · sobe · topo · fim | `nav.down` · `nav.up` · `nav.top` · `nav.bottom` |
| `ctrl+d/u` · `ctrl+f/b` | meia página · página | `nav.half_down/half_up` · `nav.page_down/page_up` |
| `h` / `l` | fecha / abre nó (contexto → sessão → subagents) | `nav.collapse` / `nav.expand` |
| `space` | seleciona a sessão e desce, como no yazi (marca `◆`); numa linha de contexto, dobra a seção | `nav.pick` |
| `esc` | limpa a seleção; sem seleção, limpa o filtro | — (da TUI) |
| `a` / `A` | próxima / anterior sessão te esperando | `nav.next_attention` / `nav.prev_attention` |
| `1` `2` `3` `4` / `tab` `shift+tab` | Sessões · Inbox · Todos · Arquivo | `tab.sessions/inbox/todos/archive` / `tab.next/prev` |
| `/` | filtro fuzzy (`esc` limpa) | `view.filter` |
| `H` | mostra/esconde paradas (> `stale_days`), arquivadas e subagents concluídos | `view.show_all` |
| `S` | ordem: atividade → custo → nome | `view.sort` |
| `R` | atualiza e reordena | `sessions.refresh` |

## Sessões

| tecla | ação | comando |
|---|---|---|
| `enter` | abre a sessão no painel e foca (INSERT) | `session.open` |
| `o` / `O` | abre sem focar / tela cheia (`←` dentro do claude volta) | `session.preview` / `session.fullscreen` |
| `i` / `ctrl+]` | entra / sai do INSERT (as teclas vão para o claude; `esc` fica com o claude) | `session.insert` / — (da TUI) |
| `ctrl+k` | pula para um chat rodando ou esperando (funciona também no INSERT) | `switcher.open` |
| `r` | resposta rápida, já preenchida com o `suggestedReply`; um número sozinho escolhe a opção da pergunta | `session.reply` |
| `c` | nova sessão em background (diretório → prompt) | `session.new` |
| `tab` (ao disparar) | modo de permissão: padrão → acceptEdits → auto → plan → ⚠ bypass (`--permission-mode`; bypass pede confirmação) | — (linha de entrada) |
| `X` | arquiva / restaura a sessão (num contexto: todas) | `session.archive` |
| `n` | renomeia a sessão; no cabeçalho de um contexto, renomeia a seção (vazio desfaz) | `session.rename` |
| `m` | muda a sessão de contexto (vazio volta à regra do diretório) | `session.move` |
| `x` / `dd` | `claude stop` / `claude rm` (pede confirmação) | `session.stop` / `session.remove` |
| `w` / `z` | fecha o painel / zoom | `pane.close` / `pane.zoom` |

## Todos e Jira (modo `normal.todos`)

| tecla | ação | comando |
|---|---|---|
| `enter` | dispara um agent para a tarefa (no repo do `@contexto`), marca `start` e anota o id | `todo.dispatch` |
| `s` / `D` | start / done | `todo.start` / `todo.done` |
| `J` | todo → cria issue no Jira (opt-in, com confirmação) · issue → vira todo | `jira.toggle` |
| `F` / `o` | busca suas issues agora (~$0,05–0,12) / abre a issue no browser | `jira.fetch` / `jira.open_url` |

## Geral

| tecla | ação | comando |
|---|---|---|
| `:` / `ctrl+p` | paleta de comandos fuzzy (abaixo) | `palette.open` |
| `u` | uso do plano (`/usage`): janela de 5 h e semana | `usage.show` |
| `T` | troca o tema | `view.theme` |
| `?` / `ctrl+/` | todos os comandos | `help.open` |
| `q` | sai (pede confirmação; `ctrl+c` duas vezes sai direto) | `app.quit` |

## Paleta (`:`)

A paleta lista os comandos que o core oferece para a seleção atual (`Registry.List`), na ordem da ajuda — os
comandos de Lua aparecem sozinhos, com a tecla deles —, qualquer sessão ou tarefa (para pular até ela) e os
temas. Também estão lá `inbox.summon` (vai para a inbox), `sessions.remove_ended` (limpa chats encerrados) e
`sessions.archive_stale` (arquiva as paradas), que não têm tecla. Ela aceita comandos com argumento, que
aparecem no topo como o item que o `enter` executa:

| comando | atalho | o quê | comando do core |
|---|---|---|---|
| `group <tag>` | `g` | agrupa as sessões selecionadas com `space` (ou a do cursor) | `group.add` |
| `ungroup [tag]` | `ug` | tira as sessões do grupo `tag`, ou de todos | `group.remove` |
| `notify [#tag] <mensagem>` | `n` | deixa uma nota para cada membro do grupo; sem `#tag`, usa o grupo da sessão do cursor | `group.notify` |
| `desfazer grupo #tag` | — | desfaz o grupo (pede confirmação) | `group.dissolve` |
| `<nome> <arg>…` | — | qualquer comando com argumentos (de Lua, por exemplo): as palavras preenchem os argumentos em ordem, a última leva o resto | o próprio |

Com sessões selecionadas, a barra de status mostra `:group agrupa N · esc limpa seleção`. Detalhes em
[grupos.md](grupos.md).

## Mouse

- Clique seleciona, e clicar de novo abre. A roda rola a lista.
- Sobre o painel do claude, roda, clique e arrasto vão direto para o programa. O renderizador fullscreen do
  Claude Code rola o chat e seleciona texto sozinho, e o que ele copia chega ao seu clipboard via OSC 52.
- Clicar no painel também entra no INSERT, e o cursor do claude passa a ser o cursor real.
- No renderizador clássico (sem mouse), a roda e `shift+pgup/pgdown` rolam o scrollback.

Veja [painel-embutido.md](painel-embutido.md) para entender como o painel trata as teclas.

## Mundo (`nkw`)

O compositor do mundo (`nkw`, guia de uso em [mundo.md](mundo.md), plano em [plano-mundo.md](plano-mundo.md) §3)
usa o mesmo keymap do core, em modos só dele. Os comandos estão em [comandos.md](comandos.md#mundo--worldgo) (`internal/app/world.go`); a TUI e o GUI
nunca mostram esses modos nem esses comandos (`core.IsWorldMode`).

**A tecla `world`.** `world` é um modificador abstrato nos nomes de tecla (`world+w`): vira `ctrl+alt` quando o
`nkw` roda aninhado (`--world-mod`, `keys.world_mod` no `world.toml`) e `super` numa sessão própria (fase 7). Toda
tecla com `world+` é um **acorde global**, valha em que modo estiver registrada: funciona em todo modo do mundo,
inclusive no FOCO, e nenhum acorde pertence a dois comandos. Assim um comando carrega a tecla simples e o acorde
(`w` e `world+w` para ANDAR). As teclas de mídia (`XF86Audio…`) também são globais.

| modo | quando | ordem de resolução (feita pelo `nkw`) |
|---|---|---|
| `world` | acordes globais, em todo modo do mundo | sempre primeiro |
| `normal.world` | NAV | `world` → `normal.world` → `normal` → `*` |
| `world.walk` | ANDAR | `world` → `world.walk` → `*` |
| `world.focus` | FOCO | `world` → `world.focus`; **sem fallback**, o resto vai para a janela |
| `world.talk` | CONVERSA | `world` → `world.talk` → `normal` (só comandos `scope: core`, com o gato como contexto) |
| `world.hint`, `world.palette`, `world.line` | SALTO, COMANDOS, linhas de texto | só o próprio modo; teclas imprimíveis são texto |
| `world.library` | diante de uma estante da biblioteca | `world` → `world.library` → `normal.world` → `normal` → `*` |
| `world.phone` | o telefone aberto (por cima dos COMANDOS, de qualquer modo) | `world` → `world.phone`; **sem fallback**; teclas imprimíveis que ele não liga são texto |

`normal.world` é um sub-modo de `normal`, como o `normal.todos` (`core.ParentMode`): as teclas dele sombreiam as
de `normal` e o resto cai em `normal` — os comandos do core que o NAV herda (`r x n m X dd c a A ctrl+k`) agem
sobre o gato da estação. `?` e `z` o NAV liga ele mesmo (`world.help`, `world.hud`): sombreiam o `help.open` e o
`pane.zoom` só no mundo. O `nkw` nunca liga `app.quit`. `world.library` é sub-modo do NAV (uma exceção à regra
do ponto em `core.ParentMode`): a estante só liga as teclas dela e o resto é o NAV. `world.phone` é a outra
exceção: `core.ParentMode("world.phone") == ""`.

| tecla | NAV (`normal.world`) |
|---|---|
| `h` `l` / `j` `k` (e setas) | estação anterior/próxima · tela de baixo/de cima |
| `J` `K` · `1`…`9` · `gg` `G` | sala próxima/anterior · sala N · primeira/última mesa |
| `enter` `i` · `t` · `w` | FOCO · CONVERSA com o gato · ANDAR |
| `f` · `F` · `o` · `b` | SALTO · SALTO direto para o FOCO · mapa · biblioteca |
| `H` `L` · `,` `.` · `-` `=` · `p` · `M` | tela para a mesa ao lado · junta/tira da pilha · mesa mais estreita/larga · prende no ar · manda para outra sala |
| `T` · `Q` · `shift+space` · `:` `ctrl+p` | terminal novo · fecha a janela · chama o gato mais urgente · COMANDOS |
| `tab` · `?` · `z` | telefone · ajuda (o telefone no app Ajuda) · esconde/mostra o HUD |

| acorde (`world`) | comando |
|---|---|
| `world+w` | `world.walk` |
| `world+h` `world+l` `world+k` `world+j` | `world.focus_left` / `_right` / `_up` / `_down` |
| `world+shift+j` `world+shift+k` | `world.room_next` / `world.room_prev` |
| `world+space` | `world.summon` |
| `world+f` · `world+p` · `world+o` | `world.hint` · `world.palette` · `world.overview` |
| `world+enter` · `world+q` · `world+d` | `world.terminal` · `world.close` · `world.launcher` |
| `world+n` | `world.phone` (o telefone, também no FOCO) |
| `world+shift+e` | `world.quit` (confirma) |

No ANDAR (`world.walk`): `w` `s` `a` `d` andam (segurar), `q` `e` (e `left` `right`) giram, `up` `down` olham, `c`
volta ao horizonte, `enter` `space` usam o que está na mira, `j` `k` ajustam o objeto, `h` `l` passam
anterior/próximo nele, `f` é SALTO, `tab` abre o telefone e `esc` volta ao NAV. As teclas de andar e girar são
**seguradas**: o `nkw` manda o comando ao apertar e de novo ao soltar (`pressed`), então religá-las no Lua
funciona como qualquer outra. O mouse e o carregar:

- **Mouse no ANDAR** (`[walk] mouse_look = true`, ou `nkw --mouse-look on|off`): ao entrar, o ponteiro do host
  fica travado e escondido e o movimento relativo gira a câmera (0,12° por contagem, sem aceleração); botão
  esquerdo = `world.interact`, direito = `world.grab`. Ao sair do ANDAR, ou quando a janela do mundo perde o
  foco, o ponteiro volta. Sem movimento relativo no host, olhar continua pelo teclado.
- **Carregar uma tela**: `g` pega a tela na mira, que flutua 1,2 m à sua frente, sempre virada para você; `[` `]`
  aproximam/afastam (0,8 a 3 m, passos de 0,2 m), `-` `=` mudam o tamanho; `g` de novo solta — numa mesa a até
  0,6 m entra na pilha, numa parede a até 0,4 m (e até 15° de desvio) encosta nela, senão fica no ar.

Na estante da biblioteca (`world.library`): `h` `l` (e `left` `right`) passam pelos livros, `j` `k` (e `down`
`up`) trocam de prateleira, `enter` abre o livro ou entra na pasta, `backspace` sobe um nível, `/` busca na
estante, `e` abre no editor, `p` mostra a prévia no atril, `y` copia o caminho e `esc` sai da estante. O resto
(`t`, `o`, `b`, os acordes) é o NAV. No FOCO só `ctrl+]`
(`keys.focus_escape`) e os acordes são do mundo. Na CONVERSA: `space`/`enter` avançam, `i`/`r` respondem,
`f` senta no terminal do gato, `j` `k` `1`…`9` escolhem, `esc`/`q` encerram. No SALTO, `esc` cancela e
`backspace` apaga; na paleta e nas linhas, `ctrl+a` `ctrl+e` `ctrl+w` `ctrl+u` editam e `esc`/`enter`
cancelam/confirmam (`tab` completa na paleta, `shift+enter` quebra a linha).

No telefone (`world.phone`, [mundo.md](mundo.md#o-telefone)): `h` `l` `j` `k` (e as setas) andam pela grade de
apps e pelas listas, `enter`/`space` abrem o item, `backspace` volta (sub-página → app → início), `esc`/`tab`
fecham, `/` vai para a linha de busca e `1`…`9` abrem o app N da tela inicial. Um item pode ter teclas próprias
(`i` "abrir isolado" nos Apps, `Q` fechar nas Janelas…), escritas no rodapé; elas valem antes da tabela do modo.

Do Lua, `nk.keymap.set` vale nesses modos como em qualquer outro ([lua.md](lua.md)):

```lua
nk.keymap.set("world.walk", "j", "world.turn_left")      -- girar com j/l em vez de q/e
nk.keymap.set("normal.world", "s", "world.walk")         -- s entra no ANDAR
nk.keymap.set("normal.world", "n", function()            -- uma função: o nkw a roda pelo core
  nk.ui.toast("olá do mundo")
end)
```

Essas sobreposições chegam ao `nkw` pelo `GET /v1/keymap` (uma função vira `lua.key:<modo>:<tecla>`, rodada
com `POST /v1/commands/lua.key:…`; ver [lua.md](lua.md#teclado)).
