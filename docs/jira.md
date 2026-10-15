---
type: Guide
title: Integração Jira
description: Ler e criar issues sem guardar token, via claude -p headless com o conector Atlassian do claude.ai.
resource: https://github.com/aqilputi/neko/tree/main/internal/jira
tags: [jira, atlassian, mcp, todos]
timestamp: 2026-10-15T17:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-15T17:00:00Z }
status: draft
---

# Jira

O neko **não guarda token**. Leitura e escrita passam por um `claude -p` headless que usa o conector Atlassian
já conectado no claude.ai (`ENABLE_CLAUDEAI_MCP_SERVERS=true` + `MCP_CONNECTOR_PREWAIT_MS`), com uma
única ferramenta liberada por chamada (`mcp__claude_ai_Atlassian_Rovo__…`).

## Por que ler o resultado cru

O neko lê o **resultado cru da ferramenta** no `stream-json` e não confia no texto do modelo. Num teste, o Haiku
respondeu `[]` sem ter carregado o conector. Se a ferramenta não foi chamada, o neko mostra um erro
("o claude não chamou … (conector Atlassian conectado?)") em vez de uma lista vazia falsa.

## Uso

- Aba Todos: `F` busca suas issues (~$0,05–0,12 por busca), `o` abre a issue no browser, `J` numa issue a
  transforma em todo e `J` num todo cria a issue, com confirmação.
- O vínculo entre todo e issue é feito com `--ref jira:KEY` mais a tag `jira`.
- `enter` numa issue dispara um agent em `agent_dir` com a instrução de ler a issue completa primeiro.
- O cache fica em `~/.cache/neko/jira.json` e vale `cache_minutes`.

## Permissões e escrita negada

O `claude -p` roda com `--permission-mode default`: só a ferramenta do `--allowedTools` passa, mesmo que o seu
`defaultMode` seja `auto`. Sem isso, o classificador do auto mode negava toda criação de issue ("External System
Writes") depois de você já ter confirmado no neko. Os hooks `PreToolUse` do seu settings continuam valendo.

Quando o claude nega uma chamada (regra de permissão ou hook), o neko mostra um aviso fixo com o motivo que ele
deu. Com `approve_hint` configurado, o aviso inclui também o comando que libera a escrita. Essas chamadas não
deixam transcript (`--no-session-persistence`); o `stream-json` da última fica em `~/.cache/neko/jira-last.jsonl`.

**Estado:** criar issue pelo neko ainda não foi testado de ponta a ponta (por isso `status: draft`).

Configuração em [configuracao.md](configuracao.md#jira--jira).
