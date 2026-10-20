package main

import (
	"embed"
	"io/fs"
)

// dist is gui/web/dist, copied here by `make build-gui` / `make install-gui`
// before the build (only dist/.gitkeep is in git). Both builds embed it: the
// native window and the app/browser modes of a binary built without it.
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
