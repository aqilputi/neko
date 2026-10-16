//go:build wails

package main

import (
	"context"
	"embed"
	"io/fs"
	"net/http"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// hasWindow: built with the Wails v3 window.
const hasWindow = true

const noWindowHint = ""

// dist is gui/web/dist, copied here by `make build-gui` before the build
// (only dist/.gitkeep is in git).
//
//go:embed all:dist
var dist embed.FS

// embeddedAssets is the frontend built into the binary (nil when the copy
// is missing: --assets is read then).
func embeddedAssets() fs.FS {
	sub, err := fs.Sub(dist, "dist")
	if err != nil {
		return nil
	}
	if _, err := fs.Stat(sub, "index.html"); err != nil {
		return nil
	}
	return sub
}

// runWindow opens the one window on url (guihost's launch URL: the page,
// /v1 and the pane WebSockets all live on that loopback origin, so no Wails
// binding is needed) until it is closed or ctx ends.
func runWindow(ctx context.Context, url string) error {
	app := application.New(application.Options{
		Name:        "nekuroddo",
		Description: "nekuroddo — sessões do Claude Code como gatos",
		Linux:       application.LinuxOptions{ProgramName: "nekuroddo"},
		// the page is served by guihost; Wails' own asset server is unused
		Assets: application.AssetOptions{Handler: http.NotFoundHandler()},
	})
	app.Window.NewWithOptions(application.WebviewWindowOptions{
		Title:  "nekuroddo",
		Width:  1280,
		Height: 800,
		URL:    url,
		Linux:  application.LinuxWindow{WebviewGpuPolicy: application.WebviewGpuPolicyAlways},
	})
	stop := context.AfterFunc(ctx, app.Quit)
	defer stop()
	return app.Run()
}
