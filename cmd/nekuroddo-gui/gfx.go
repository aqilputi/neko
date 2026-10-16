package main

import (
	"os"
	"path/filepath"
)

// dmabufEnv turns off WebKitGTK's DMA-BUF renderer, the documented cause
// of blank or crashing WebGL on NVIDIA and X11 (plan §4.6, §7).
const dmabufEnv = "WEBKIT_DISABLE_DMABUF_RENDERER"

// useSafeGfx: set dmabufEnv when --safe-gfx asks for it or the last window
// did not close cleanly — never over a value the user already set.
func useSafeGfx(forced, envSet, lastRunFailed bool) bool {
	return !envSet && (forced || lastRunFailed)
}

// markRunning leaves a marker at path while the window is up; done removes
// it after a clean close. A marker found at start means the last window
// crashed (or never drew), so that run retries with the safe renderer.
func markRunning(path string) (done func()) {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err == nil {
		_ = os.WriteFile(path, []byte("1\n"), 0o600)
	}
	return func() { os.Remove(path) }
}
