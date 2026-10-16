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
