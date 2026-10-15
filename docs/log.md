# Log — neko

Histórico de decisões e progresso. Entradas mais recentes no topo.

## 2026-10-15

- **publicação:** repositório público em github.com/aqilputi/neko, documentação reorganizada como
  bundle OKF 0.2 em `docs/`, licença MIT. Módulo renomeado para `github.com/aqilputi/neko`.
  O `.gitignore` ignorava `cmd/neko/` por engano (padrão `neko` sem âncora) e foi corrigido para `/neko`.
  Valores específicos de organização saíram do código e do exemplo: `approve_hint` agora não tem padrão e o
  teste ao vivo do Jira lê `NEKO_JIRA_SITE`.
- **progresso:** hooks em tempo real (`neko hook` → `events.jsonl`), [Jira](jira.md) sem token via
  `claude -p` + conector, paleta `:`, diálogos em overlay, mouse, arquivar (`X`), sessões paradas ocultas (`H`).
- **decisão:** painel embutido = PTY + `x/vt` rodando `claude attach`, sem reimplementar o claude. Bugs do
  `x/vt` v0.1.0 (OSC não-ASCII, teclas com modificadores) contornados em `internal/term`
  ([painel-embutido.md](painel-embutido.md)).
- **decisão:** embrulhar o `claude` instalado em vez de usar o SDK, para receber as atualizações do Claude Code
  sem mudança no neko ([arquitetura.md](arquitetura.md)).
- **pesquisa:** [mapa de requisitos](requisitos.md) levantado de 30 dias de uso real.
