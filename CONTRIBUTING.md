# Contribuindo com o nekuroddo

Obrigado pelo interesse. Issues e pull requests são bem-vindos, em português ou em inglês.

## Antes de começar

- Para bugs, abra uma issue com a versão do nekuroddo (`git describe` ou o commit), a do Claude Code
  (`claude --version`), o terminal e os passos para reproduzir.
- Para mudanças maiores (um recurso novo, uma dependência nova, mudança de comportamento de tecla), abra uma
  issue antes para combinar a abordagem.
- Leia o [Código de Conduta](CODE_OF_CONDUCT.md). Vulnerabilidades vão pelo [SECURITY.md](SECURITY.md), não
  por issue pública.

## Ambiente

- Go 1.27.2+ (a versão do `go.mod`)
- [golangci-lint](https://golangci-lint.run/) v2, para `make lint` e `make fmt`
- Claude Code no `PATH`, para rodar a TUI de verdade (os testes não precisam dele)

```sh
git clone https://github.com/aqilputi/nekuroddo && cd nekuroddo
make build && ./bin/nekuroddo
```

## Make

| alvo | o quê |
|---|---|
| `make build` | compila `bin/nekuroddo` |
| `make install` | instala em `~/.local/bin/nekuroddo` + atalho `nk` (`PREFIX=` muda o destino) |
| `make test` | `go test -race ./...` |
| `make cover` / `make cover-html` | cobertura em `coverage.out` / `coverage.html` |
| `make cover-check` | falha se a cobertura total ficar abaixo de `COVER_MIN` (80%) |
| `make lint` / `make fmt` | `golangci-lint run` / formata o código |
| `make vet` | `go vet ./...` |
| `make ci` | `vet` + `lint` + `cover-check`, o mesmo que roda no GitHub Actions |

Rode `make ci` antes de abrir o PR.

## Estilo

- **Siga o código ao redor.** Os pacotes são pequenos e diretos; prefira a forma que já existe no arquivo a
  introduzir uma abstração nova.
- **Comentários no código em inglês**; **textos da interface em pt-BR** (rótulos, toasts, ajuda, mensagens
  do gato). A documentação em `docs/` é em pt-BR.
- `gofmt`/`goimports` via `make fmt`. O lint segue o `.golangci.yml`.
- Teclas novas entram em `internal/tui/keys.go`: é a fonte única da barra de status, da ajuda e da paleta.
  Atualize também [docs/teclas.md](docs/teclas.md).
- Os hooks (`nk hook`, `nk hook-context`) **nunca podem falhar para o Claude Code**: sem saída de erro, código
  de saída 0, poucos milissegundos. Nada de imports pesados nesse caminho.
- O nekuroddo **só lê** `~/.claude`. Parsing de formatos internos do Claude Code fica em `internal/claude` (ou
  `internal/events`), com testes.
- Embrulhe o `claude` em vez de reimplementá-lo: se algo pode ser feito chamando o CLI, chame o CLI.

## Testes (TDD)

O projeto é validado por testes, escritos **antes** do código:

1. Escreva o teste que descreve o comportamento novo (ou que reproduz o bug) e veja ele falhar.
2. Escreva o mínimo de código para ele passar.
3. Refatore com os testes verdes.

- Teste comportamento, não implementação: tabelas de casos (`[]struct{...}`), entradas e saídas observáveis.
  Na `internal/tui`, dirija o `Model` por teclas/mensagens e confira o estado ou a tela sem ANSI (`ansi.Strip`).
- **Cobertura mínima de 80%** do total, travada no CI (`scripts/coverage-gate.sh`; o `cmd/vtprobe`, ferramenta de
  depuração, fica de fora). PR que derruba a cobertura abaixo disso não passa.
- Bug encontrado sem correção ainda: deixe o teste que o reproduz com `t.Skip("BUG: …")` e abra uma issue.
- `make test` roda tudo com `-race`; `make cover` mostra a cobertura total.
- Testes que tocam o estado usam diretórios temporários (`t.TempDir()` + `XDG_*_HOME`) e apontam o binário do
  claude para `/bin/false`. Nunca dependa do seu `~/.claude` real.
- Testes ao vivo (claude de verdade, Jira) ficam atrás de variáveis de ambiente e são pulados por padrão; veja o
  comentário no topo de cada `*live*_test.go`.

## Pacotes de trabalho (vários agents em paralelo)

A refatoração de arquitetura em [docs/plano-arquitetura.md](docs/plano-arquitetura.md) é dividida em **pacotes
de trabalho (WP)**, pensados para vários agents trabalharem ao mesmo tempo sem pisar um no outro. Na prática,
hoje isso funciona assim (diferente do fluxo de branch-por-PR de um contribuidor solo, acima):

- Os agents de uma mesma "onda" trabalham **em paralelo, na mesma branch**, cada um dono de um escopo de
  arquivos fechado e disjunto (ex.: um WP só mexe em `internal/core/**`, outro só em `internal/app/**`). Dentro
  de uma onda, nenhum par de WPs compartilha arquivo.
- `scripts/wp-scope.sh '<regex>'` confere que o diff do seu WP não saiu do escopo combinado
  (`git diff --name-only` contra a base, filtrado pela regex do escopo).
- Ordem **TDD** dentro do WP, igual ao resto do projeto: testes primeiro (vermelhos), depois o mínimo de
  código, depois refatoração.
- Pacote Go **novo** (ex. `internal/core`, `internal/app`) entra com **cobertura ≥ 90%**; a cobertura total do
  projeto nunca pode cair mais de **0,5 ponto** em relação ao que estava antes do WP.
- Um agent **integrador** roda `make ci` depois que os WPs de uma onda terminam e comita o resultado da onda
  (não um commit por WP isolado); conflitos de rebase são resolvidos pelo dono do WP afetado.
- Comentários de código em inglês; textos de interface **e documentação** em pt-BR — a mesma regra de
  "Estilo", acima, vale dentro de um WP.
- Cada WP que muda comportamento ou adiciona algo visível ganha sua própria linha em `## [Não lançado]` no
  [CHANGELOG.md](CHANGELOG.md), para o integrador não ter que reconstruir o que mudou.

## Commits

- Mensagens curtas, no imperativo ou descritivas, com o escopo na frente quando ajuda:
  `jira: criação de issue sai do auto mode`, `ui: space seleciona sessões`.
- Um commit por mudança lógica. O corpo explica o **porquê** quando não for óbvio.

## Pull requests

- [ ] `make ci` passa
- [ ] testes escritos antes do código (TDD), cobrindo a mudança; cobertura total ≥ 80%
- [ ] docs atualizadas (`README.md`, `docs/`, `config.example.toml`) quando o comportamento muda
- [ ] entrada em `## [Não lançado]` no [CHANGELOG.md](CHANGELOG.md)
- [ ] nenhum segredo ou dado de organização no diff

## Segredos e dados da sua organização

Nunca commite IDs da sua empresa, `cloud_id`, `account_id`, sites do Atlassian, tokens, caminhos internos ou
trechos de transcripts. Configuração pessoal fica em `~/.config/nekuroddo/config.toml`, fora do repositório; o
[`config.example.toml`](config.example.toml) usa só valores genéricos (`acme.atlassian.net`, `<uuid do site>`).
Em testes, use dados inventados.

## Licença

Ao contribuir, você concorda que a sua contribuição é licenciada sob a [MIT](LICENSE).
