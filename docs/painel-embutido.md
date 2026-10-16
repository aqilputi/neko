---
type: Design Note
title: Painel embutido (PTY + x/vt)
description: Como o nekuroddo roda o claude de verdade dentro de um painel e os bugs do x/vt v0.1.0 que precisou contornar.
resource: https://github.com/aqilputi/nekuroddo/tree/main/internal/term
tags: [pty, terminal, x-vt, osc, teclas]
timestamp: 2026-10-16T12:00:00Z
generated: { by: agent:claude-code/claude-opus-5-5, at: 2026-10-16T12:00:00Z }
status: stable
sources:
  - id: x-vt
    resource: https://github.com/charmbracelet/x/tree/main/vt
    title: charmbracelet/x/vt — emulador de terminal virtual
---

# Painel embutido

`internal/term` roda `claude attach <id>` (ou `claude --bg …`) num PTY. A saída alimenta um emulador
`charmbracelet/x/vt`, e a tela do emulador é copiada **célula a célula** para o `uv.ScreenBuffer` do nekuroddo
via `Emulator.Draw`. Esse caminho usa um lock próprio, porque o `SafeEmulator` não protege o `Draw`.
No modo INSERT, as teclas vão para o programa sem alteração, então o Claude Code se comporta como num
terminal normal e continua sendo a versão instalada.

## Bugs do x/vt v0.1.0 e contornos

**OSC com bytes não-ASCII** (`internal/term/osc.go`). O emulador aborta um OSC no primeiro byte não-ASCII e
imprime o resto como texto. O título que o Claude define (`✳ <resumo da tarefa>`) aparecia como texto
fantasma no prompt. Por isso o nekuroddo filtra os OSC antes de passá-los ao emulador:

- OSC 0/2 (título) vira o subtítulo do painel;
- OSC 52 (clipboard) é repassado ao terminal real, e a cópia chega ao seu clipboard;
- os demais OSC ASCII passam direto.

**Teclas** (`internal/term/keys.go`). Com NumLock, auto-repeat ou modificadores, algumas teclas chegavam
erradas. O nekuroddo recodifica antes de escrever no PTY.

**Resize.** Tem debounce, para o claude não redesenhar a cada pixel enquanto a janela é arrastada.

## Reproduzir

```sh
go run ./cmd/vtprobe -out /tmp/probe -- claude --model haiku
```

Digita prompts, rola com a roda, arrasta para selecionar e, a cada passo, despeja tela, cursor, título e
clipboard. Os testes ao vivo (`NEKO_LIVE=1 go test ./internal/term/`) abrem um claude real sem enviar prompt,
sem gastar tokens.

Parte da [arquitetura](arquitetura.md). Teclas do painel em [teclas.md](teclas.md).
