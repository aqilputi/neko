# Segurança

## Versões suportadas

Só a versão mais recente (`main` e a última tag) recebe correções.

## Como reportar

**Não abra issue pública.** Use o
[reporte privado de vulnerabilidade do GitHub](https://github.com/aqilputi/nekuroddo/security/advisories/new),
com a versão, os passos para reproduzir e o impacto. A resposta inicial deve chegar em até 7 dias.

## Escopo

Interessam especialmente:

- os hooks `nk hook` e `nk hook-context`, que rodam a cada evento do Claude Code e cuja saída (no caso do
  `hook-context`) entra no contexto da sessão;
- a leitura de arquivos em `~/.claude` (o nekuroddo nunca deve abrir os `.key` nem escrever ali);
- o painel embutido (PTY e sequências de escape repassadas ao terminal, como OSC 52);
- o caminho do Jira via `claude -p`.

Fora de escopo: vulnerabilidades do próprio Claude Code (reporte à Anthropic) e o fato de agents de um
[grupo](docs/grupos.md) trocarem mensagens, que é intencional e passa pelas permissões de cada sessão.
