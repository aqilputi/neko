---
type: Reference
title: Teclas e mouse
description: Mapa completo de teclas (estilo vim) e do comportamento do mouse no neko.
resource: https://github.com/aqilputi/neko/blob/main/internal/ui/keys.go
tags: [teclas, vim, mouse, ui]
timestamp: 2026-10-15T17:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-15T17:00:00Z }
status: stable
---

# Teclas e mouse

A fonte única das teclas é `internal/ui/keys.go`: cada uma é um `key.Binding`, e a ajuda (`?`) e a paleta (`:`)
são geradas a partir dele.

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
| `r` | resposta rápida, que já vem preenchida com o `suggestedReply`; um número sozinho escolhe a opção da pergunta |
| `c` | nova sessão em background (diretório → prompt) |
| `tab` (ao disparar) | modo de permissão: padrão → acceptEdits → auto → plan → ⚠ bypass (`--permission-mode`; bypass pede confirmação) |
| `x` / `dd` | `claude stop` / `claude rm` (pede confirmação) |
| `w` / `z` | fecha o painel / zoom |
| Todos: `enter` | dispara um agent para a tarefa (no repo do `@contexto`), marca `start` e anota o id |
| Todos: `s` / `D` | start / done |
| Todos: `J` | todo → cria issue no Jira (opt-in, com confirmação) · issue → vira todo |
| Todos: `F` / `o` | busca suas issues agora (~$0,05–0,12) / abre a issue no browser |
| `:` / `ctrl+p` | paleta de comandos fuzzy: qualquer ação, pular para sessão/tarefa, trocar tema |
| `T` `R` `?` `q` | tema · atualizar · ajuda · sair (pede confirmação; `ctrl+c` duas vezes sai direto) |

## Mouse

- Clique seleciona, e clicar de novo abre. A roda rola a lista.
- Sobre o painel do claude, roda, clique e arrasto vão direto para o programa. O renderizador fullscreen do
  Claude Code rola o chat e seleciona texto sozinho, e o que ele copia chega ao seu clipboard via OSC 52.
- Clicar no painel também entra no INSERT, e o cursor do claude passa a ser o cursor real.
- No renderizador clássico (sem mouse), a roda e `shift+pgup/pgdown` rolam o scrollback.

Veja [painel-embutido.md](painel-embutido.md) para entender como o painel trata as teclas.
