# neko 🐈‍⬛

Orquestrador TUI das suas sessões do [Claude Code](https://claude.com/claude-code), em Go com Charm
(Bubble Tea v2, Lip Gloss v2, Bubbles). Um gatinho preto (`ᓚᘏᗢ`) dorme no canto inferior direito e acorda
para miar quando um agent volta.

**Embrulha o `claude` instalado, sem reimplementá-lo.** A lista vem de `claude agents --json`, as sessões
novas saem de `claude --bg`, e o painel da direita é um terminal virtual (PTY + `charmbracelet/x/vt`)
rodando `claude attach <id>` de verdade. Toda atualização do Claude Code chega ao neko sem mudar nada aqui.

- árvore **contexto → sessão → subagents**, com modelo e custo de cada nó
- **inbox** de quem está te esperando, com a pergunta, as opções e a resposta sugerida (`r` responde)
- **Claude Code embutido** num painel ou em tela cheia, com mouse e clipboard
- barra de custo por sessão e por contexto, gasto do dia e orçamento
- teclas de vim, paleta fuzzy (`:`), 5 temas, eventos em tempo real via hooks
- opcional: lista de tarefas (skill `todo`) e Jira sem token

## Instalação

```sh
go install github.com/aqilputi/neko/cmd/neko@latest   # Go 1.27.2+
neko
```

Requer o `claude` no `PATH`. Detalhes em [docs/instalacao.md](docs/instalacao.md).

## Teclas essenciais

| tecla | ação |
|---|---|
| `j` `k` `h` `l` | navega / abre / fecha |
| `a` | próxima sessão te esperando |
| `enter` / `O` | abre a sessão no painel / tela cheia |
| `i` / `ctrl+]` | entra / sai do claude |
| `r` | resposta rápida |
| `c` | nova sessão em background |
| `:` · `?` · `q` | paleta · ajuda · sair |

Lista completa em [docs/teclas.md](docs/teclas.md).

## Documentação

Bundle [OKF 0.2](https://okf.md/spec/) em [docs/index.md](docs/index.md):
[instalação](docs/instalacao.md) · [teclas](docs/teclas.md) · [configuração](docs/configuracao.md) ·
[arquitetura](docs/arquitetura.md) · [fontes de dados](docs/fontes-de-dados.md) ·
[painel embutido](docs/painel-embutido.md) · [hooks](docs/hooks.md) · [Jira](docs/jira.md) ·
[requisitos](docs/requisitos.md) · [log](docs/log.md)

## Privacidade

O neko só **lê** arquivos locais do `~/.claude`, nunca abre os arquivos `.key` e não chama a API nem envia
dados a lugar nenhum. Os formatos que ele lê são internos ao Claude Code e podem mudar entre versões.
O parsing fica isolado em `internal/claude`, com testes.

## Licença

[MIT](LICENSE)
