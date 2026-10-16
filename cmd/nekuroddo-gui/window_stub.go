//go:build !wails

package main

import (
	"context"
	"errors"
	"io/fs"
)

// hasWindow: this binary was built without the Wails window.
const hasWindow = false

const noWindowHint = `nkg: este binário foi construído sem a janela (build tag "wails").
  agora, no navegador:  go run ./cmd/nekuroddo-gui --browser --demo
                        (ou make run-gui-browser)
  a janela (GTK4 + WebKitGTK 6.0):
                        sudo pacman -S webkitgtk-6.0   # Arch/CachyOS
                        make build-gui && bin/nkg
`

// embeddedAssets: no embedded frontend; it is read from --assets.
func embeddedAssets() fs.FS { return nil }

// runWindow is unreachable without the tag (run checks hasWindow first).
func runWindow(context.Context, string) error { return errors.New("nkg sem janela") }
