//go:build !wails

package main

import (
	"context"
	"errors"
)

// hasWindow: this binary was built without the Wails window.
const hasWindow = false

// runWindow is unreachable without the tag (run opens an app window instead).
func runWindow(context.Context, string) error { return errors.New("nkg sem janela") }
