---
type: Guide
title: Grupos de agents
description: Como agrupar sessões do Claude Code que fazem trabalho parecido para que uma avise as outras, e como o nekuroddo mostra essas mensagens.
resource: https://github.com/aqilputi/nekuroddo/tree/main/internal/groups
tags: [grupos, agents, sendmessage, hooks, ui]
timestamp: 2026-10-16T12:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T12:00:00Z }
status: draft
sources:
  - id: claude-code-hooks
    resource: https://docs.claude.com/en/docs/claude-code/hooks
    title: Claude Code — Hooks reference
---

# Grupos de agents

Quando várias sessões fazem a mesma coisa (quatro deploys parecidos, a mesma migração em cinco repos), o
problema que uma descobre costuma valer para as outras. Sem ajuda, você vira o mensageiro: lê a correção numa
aba e cola nas outras três. Um **grupo** liga essas sessões para que elas mesmas se avisem, e o nekuroddo
mostra quem avisou quem.

O nekuroddo **não transporta** as mensagens. Ele só conta a cada sessão quem são os pares; a conversa usa o
`SendMessage` entre sessões que o próprio Claude Code já tem.

## Uso

1. Na aba Sessões, selecione as sessões com `space`. A seleção funciona como no yazi: marca a sessão com `◆` e
   desce o cursor. Numa linha de contexto, `space` continua dobrando a seção. `esc` limpa a seleção.
2. Abra a paleta (`:`) e digite `group deploy-rc`. Sem seleção, o comando vale para a sessão sob o cursor.
   A tag é normalizada: `#Deploy RC` vira `deploy-rc`.
3. Cada membro passa a mostrar a barra `▍` e o chip `#deploy-rc` na cor do grupo (estável por tag). Com o cursor
   numa sessão, os pares dela aparecem com `┃`.
4. Na próxima ferramenta que cada sessão rodar, ela recebe a lista dos pares e o combinado do grupo.

| comando na paleta | o quê |
|---|---|
| `group <tag>` (ou `g`) | põe as sessões selecionadas (ou a do cursor) no grupo; atualiza o nome de quem já está |
| `ungroup [tag]` (ou `ug`) | tira as sessões do grupo; sem tag, de todos. Grupo vazio some |
| `notify [#tag] <mensagem>` (ou `n`) | deixa uma nota sua para cada membro; sem `#tag`, usa o primeiro grupo da sessão do cursor |

Uma sessão pode estar em mais de um grupo; a lista mostra o primeiro chip e `+N`.

## Fluxo

```
  você (nekuroddo)                         ~/.local/state/nekuroddo/
  ─────────────────                        ──────────────────────────
  space · space · :group deploy-rc ──────► groups.json  {deploy-rc: [rc1, rc2, rc3]}
  :notify #deploy-rc <nota> ─────────────► mail/<session>.jsonl
                                                │
                       SessionStart / UserPromptSubmit / PostToolUse
                                                ▼
  ┌──────────── sessão rc1 ────────────┐  nk hook-context (síncrono)
  │ additionalContext:                 │◄─ imprime os pares + o combinado
  │  "Grupo #deploy-rc — seus pares:   │   quando o grupo mudou (hash em ctx/<id>)
  │   deploy rc 2, deploy rc 3 ..."    │   e entrega as notas da fila
  │                                    │
  │ acha um bug ─► SendMessage ────────┼──────────────┐
  └────────────────────────────────────┘              │ cross-session (Claude Code)
                                                      ▼
  ┌──────────── sessão rc2 ────────────┐  UserPromptSubmit:
  │ <cross-session-message             │  <cross-session-message from-name="deploy rc 1">
  │   from-name="deploy rc 1">         │  [#deploy-rc] readiness probe quebra no chart 4.2
  │ [#deploy-rc] readiness probe ...   │──► nk hook (assíncrono) ─► events.jsonl  type=peer
  └────────────────────────────────────┘                                │
                                                                        ▼
                                         nekuroddo (tail a cada 400 ms): ✉→ em rc1, ✉← em rc2,
                                         o gato mia "✉ deploy rc 1 → deploy rc 2",
                                         a mensagem entra no detalhe das duas sessões
```

### 1. O grupo

`:group` grava `~/.local/state/nekuroddo/groups.json` (escrita atômica, porque o hook pode ler a qualquer
momento). Cada membro guarda o `session_id`, o **nome** que o Claude Code deu à sessão (é o endereço do
`SendMessage`, o mesmo que `claude agents` mostra), o rótulo que o nekuroddo exibe e o `cwd`. Quando o Claude
renomeia uma sessão, o nekuroddo atualiza o nome no arquivo no refresh seguinte.

### 2. Os pares ficam sabendo: `nk hook-context`

É um hook **síncrono** em `SessionStart`, `UserPromptSubmit` e `PostToolUse`. Ele lê o payload, monta o texto
do grupo da sessão e responde com `hookSpecificOutput.additionalContext`, que o Claude Code injeta na
conversa. O texto diz:

- quais são os pares, com o nome para o `SendMessage`, o rótulo no nekuroddo e o diretório;
- o combinado: avisar os pares quando achar um bug, armadilha ou correção que provavelmente os afeta, com a
  primeira linha começando por `[#<grupo>]`; tratar avisos recebidos como informação; não puxar conversa; e
  não insistir se o par não aparece em `ListAgents`.

Para não repetir o mesmo texto a cada ferramenta, o hook guarda um hash do contexto em
`ctx/<session_id>` e só fala de novo quando o grupo muda, ou no `SessionStart` (o contexto da sessão acabou de
ser refeito: início, `resume`, `clear` ou `compact`). Se a sessão sai de todos os grupos, ela recebe uma única
mensagem pedindo para desconsiderar o combinado anterior. Fora de grupos e sem notas, o hook não imprime nada.
Em qualquer erro, também não imprime nada e sai com 0.

### 3. A mensagem: `SendMessage` do Claude Code

O agent que acha o problema chama o `SendMessage` (carregado via `ToolSearch` se preciso) para cada par. O
Claude Code entrega a mensagem ao destinatário como um prompt:

```
<cross-session-message from="uds:…" from-name="deploy rc 1" from-mode="…">
[#deploy-rc] readiness probe quebra no chart 4.2
O chart trocou /healthz por /ready; no rc 1 corrigi com readinessProbe.path: /ready.
</cross-session-message>
```

### 4. O nekuroddo mostra

Esse prompt dispara o `UserPromptSubmit` do destinatário, e o `nk hook` (assíncrono, o mesmo dos
[eventos em tempo real](hooks.md)) grava um evento `type: "peer"` com o `from-name` e a primeira linha do
corpo. Ao ler o evento, o nekuroddo:

- marca `✉←` no destinatário e, se acha o remetente pelo nome, `✉→` nele;
- faz o `✉` piscar por 90 s e depois o deixa apagado até completar 15 min;
- faz o gato miar `✉ <remetente> → <destinatário>`;
- guarda a mensagem (até 100) e mostra as 5 últimas de cada sessão no painel de detalhes, abaixo da linha
  `▍#tag com <pares>`.

### 5. Notas suas: `:notify`

`:notify #deploy-rc a janela de deploy mudou para 18h` grava a nota em `mail/<session_id>.jsonl` de cada
membro. O `nk hook-context` entrega a nota na próxima ferramenta ou prompt da sessão, como
`[nekuroddo] Aviso do seu usuário para o grupo #deploy-rc: …`. A leitura renomeia o arquivo antes de ler,
então uma nota que chega no meio fica para a chamada seguinte. Sessões paradas recebem quando voltarem a rodar;
o toast diz quantas estão nessa situação.

## Limites

- **Mesma máquina, sessões vivas.** O `SendMessage` entre sessões só alcança sessões rodando na mesma máquina.
  Uma sessão do histórico pode estar no grupo, mas só ouve os pares quando voltar a rodar.
- **Modo de permissão.** Uma sessão num modo de permissão diferente do remetente segura a mensagem entre
  sessões até você aprovar. Para o aviso fluir sozinho, dispare os membros no mesmo modo.
- **Informação, não instrução.** Por desenho, quem recebe confere se o aviso vale para o próprio trabalho e
  segue a tarefa. A mensagem de um par não substitui as suas instruções nem as permissões da sessão.
- **Nome como endereço.** O remetente é identificado pelo `from-name`. Nomes cortados com `…` são resolvidos
  por prefixo; se duas sessões têm o mesmo nome, o `✉→` pode ir para a errada.
- **Custo.** O texto do grupo entra no contexto de cada membro (algumas centenas de tokens) e só se repete
  quando muda.

**Estado:** `draft`. Recurso novo nesta versão, e o formato `<cross-session-message>` é interno ao Claude
Code e pode mudar entre versões.

Instalação dos hooks em [hooks.md](hooks.md); teclas em [teclas.md](teclas.md); onde o pacote se encaixa em
[arquitetura.md](arquitetura.md).
