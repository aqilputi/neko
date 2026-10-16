---
type: Reference
title: Teclas e mouse
description: Mapa completo de teclas (estilo vim), comandos da paleta e comportamento do mouse no nekuroddo.
resource: https://github.com/aqilputi/nekuroddo/blob/main/internal/ui/keys.go
tags: [teclas, vim, mouse, paleta, ui]
timestamp: 2026-10-16T12:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T12:00:00Z }
status: stable
---

# Teclas e mouse

A fonte única das teclas é `internal/ui/keys.go`: cada uma é um `key.Binding`, e a barra de status, a ajuda
(`?`) e a paleta (`:`) são geradas a partir dele.

## Lista e navegação

| tecla | ação |
|---|---|
| `j` `k` `gg` `G` | desce · sobe · topo · fim |
| `ctrl+d/u` · `ctrl+f/b` | meia página · página |
| `h` / `l` | fecha / abre nó (contexto → sessão → subagents) |
| `space` | seleciona a sessão e desce, como no yazi (marca `◆`); numa linha de contexto, dobra a seção |
| `esc` | limpa a seleção; sem seleção, limpa o filtro |
| `a` / `A` | próxima / anterior sessão te esperando |
| `1` `2` `3` `4` / `tab` `shift+tab` | Sessões · Inbox · Todos · Arquivo |
| `/` | filtro fuzzy (`esc` limpa) |
| `H` | mostra/esconde paradas (> `stale_days`), arquivadas e subagents concluídos |
| `S` | ordem: atividade → custo → nome |
| `R` | atualiza e reordena |

## Sessões

| tecla | ação |
|---|---|
| `enter` | abre a sessão no painel e foca (INSERT) |
| `o` / `O` | abre sem focar / tela cheia (`←` dentro do claude volta) |
| `i` / `ctrl+]` | entra / sai do INSERT (as teclas vão para o claude; `esc` fica com o claude) |
| `ctrl+k` | pula para um chat rodando ou esperando (funciona também no INSERT) |
| `r` | resposta rápida, já preenchida com o `suggestedReply`; um número sozinho escolhe a opção da pergunta |
| `c` | nova sessão em background (diretório → prompt) |
| `tab` (ao disparar) | modo de permissão: padrão → acceptEdits → auto → plan → ⚠ bypass (`--permission-mode`; bypass pede confirmação) |
| `X` | arquiva / restaura a sessão (num contexto: todas) |
| `n` | renomeia a sessão; no cabeçalho de um contexto, renomeia a seção (vazio desfaz) |
| `m` | muda a sessão de contexto (vazio volta à regra do diretório) |
| `x` / `dd` | `claude stop` / `claude rm` (pede confirmação) |
| `w` / `z` | fecha o painel / zoom |

## Todos e Jira

| tecla | ação |
|---|---|
| `enter` | dispara um agent para a tarefa (no repo do `@contexto`), marca `start` e anota o id |
| `s` / `D` | start / done |
| `J` | todo → cria issue no Jira (opt-in, com confirmação) · issue → vira todo |
| `F` / `o` | busca suas issues agora (~$0,05–0,12) / abre a issue no browser |

## Geral

| tecla | ação |
|---|---|
| `:` / `ctrl+p` | paleta de comandos fuzzy (abaixo) |
| `u` | uso do plano (`/usage`): janela de 5 h e semana |
| `T` | troca o tema |
| `?` / `ctrl+/` | todos os comandos |
| `q` | sai (pede confirmação; `ctrl+c` duas vezes sai direto) |

## Paleta (`:`)

A paleta busca, por fuzzy, qualquer ação das tabelas acima, qualquer sessão ou tarefa (para pular até ela) e
os temas. Ela também aceita comandos com argumento, que aparecem no topo como o item que o `enter` executa:

| comando | atalho | o quê |
|---|---|---|
| `group <tag>` | `g` | agrupa as sessões selecionadas com `space` (ou a do cursor) |
| `ungroup [tag]` | `ug` | tira as sessões do grupo `tag`, ou de todos |
| `notify [#tag] <mensagem>` | `n` | deixa uma nota para cada membro do grupo; sem `#tag`, usa o grupo da sessão do cursor |

Com sessões selecionadas, a barra de status mostra `:group agrupa N · esc limpa seleção`. Detalhes em
[grupos.md](grupos.md).

## Mouse

- Clique seleciona, e clicar de novo abre. A roda rola a lista.
- Sobre o painel do claude, roda, clique e arrasto vão direto para o programa. O renderizador fullscreen do
  Claude Code rola o chat e seleciona texto sozinho, e o que ele copia chega ao seu clipboard via OSC 52.
- Clicar no painel também entra no INSERT, e o cursor do claude passa a ser o cursor real.
- No renderizador clássico (sem mouse), a roda e `shift+pgup/pgdown` rolam o scrollback.

Veja [painel-embutido.md](painel-embutido.md) para entender como o painel trata as teclas.
