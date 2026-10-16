// Command nekuroddo-gui (nkg) is the graphical client of plan §1.2: the
// frontend in gui/web served by internal/guihost on a loopback origin, in a
// Wails v3 window (GTK4 + WebKitGTK 6.0) or, with --browser, in the default
// browser.
//
//	nkg                      the core in this process, in a window
//	nkg --connect unix://…   a client of a running nk serve / nk --listen
//	nkg --demo               the scripted demo world (no real claude)
//	nkg --browser [--no-open] no window: serve and open the default browser
//	nkg --dev                frontend from gui/web/dist on disk
//
// The window needs the build tag `wails` (cgo, gtk4, webkitgtk-6.0): `make
// build-gui`. Without it nkg still builds, and --browser works.
package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"net"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"strings"
	"syscall"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/guihost"
	"github.com/aqilputi/nekuroddo/internal/paths"
	"github.com/aqilputi/nekuroddo/internal/rpc"
)

// version is what hello reports (set with -ldflags "-X main.version=…").
var version = "dev"

func main() {
	os.Exit(run(context.Background(), os.Args[1:], os.Stdout, os.Stderr))
}

// flags of one nkg run.
type flags struct {
	connect, tokenFile, listen, addr, assets string
	demo, dev, browser, noOpen, safeGfx      bool
}

func parseFlags(args []string, stderr io.Writer) (flags, error) {
	var f flags
	fs := newFlagSet(stderr)
	fs.StringVar(&f.connect, "connect", "", "cliente de um nk serve / nk --listen (unix:///… ou tcp://127.0.0.1:porta); vazio: core neste processo")
	fs.StringVar(&f.tokenFile, "token-file", rpc.DefaultTokenPath(), "token do --connect tcp://")
	fs.StringVar(&f.listen, "listen", "", "também serve o core embutido por RPC (unix:///… ou tcp://127.0.0.1:0)")
	fs.StringVar(&f.addr, "addr", "127.0.0.1:0", "endereço loopback da página")
	fs.StringVar(&f.assets, "assets", filepath.Join("gui", "web", "dist"), "frontend construído (com --dev, ou num binário sem a tag wails)")
	fs.BoolVar(&f.demo, "demo", false, "mundo de demonstração roteirizado (sem claude de verdade)")
	fs.BoolVar(&f.dev, "dev", false, "frontend do disco (--assets) em vez do embutido")
	fs.BoolVar(&f.browser, "browser", false, "sem janela: abre o navegador padrão")
	fs.BoolVar(&f.noOpen, "no-open", false, "com --browser: só imprime o link de abertura")
	fs.BoolVar(&f.safeGfx, "safe-gfx", false, "janela com WEBKIT_DISABLE_DMABUF_RENDERER=1 (NVIDIA/X11)")
	if err := fs.Parse(args); err != nil {
		return f, err
	}
	switch {
	case fs.NArg() > 0:
		return f, fmt.Errorf("argumento inesperado %q", fs.Arg(0))
	case f.connect != "" && (f.demo || f.listen != ""):
		return f, errors.New("--connect não combina com --demo nem --listen (o core é o do outro processo)")
	case f.noOpen && !f.browser:
		return f, errors.New("--no-open só vale com --browser")
	}
	return f, nil
}

// run is nkg until the window closes, ctx ends or SIGINT/SIGTERM.
func run(ctx context.Context, args []string, stdout, stderr io.Writer) int {
	f, err := parseFlags(args, stderr)
	if err != nil {
		if !errors.Is(err, errHelp) {
			fmt.Fprintln(stderr, "nkg:", err)
		}
		return 2
	}
	if !f.browser && !hasWindow {
		fmt.Fprint(stderr, noWindowHint)
		return 2
	}
	assets, err := pickAssets(f.dev, f.assets, embeddedAssets())
	if err != nil {
		fmt.Fprintln(stderr, "nkg:", err)
		return 1
	}
	ctx, stop := signal.NotifyContext(ctx, os.Interrupt, syscall.SIGTERM)
	defer stop()

	up, closeCore, err := core(ctx, f, stdout, stderr)
	if err != nil {
		fmt.Fprintln(stderr, "nkg:", err)
		return 1
	}
	defer closeCore()
	gh, err := guihost.Serve(ctx, guihost.Options{Assets: assets, Upstream: up, Listen: f.addr,
		Logf: func(format string, a ...any) { fmt.Fprintf(stderr, format+"\n", a...) }})
	if err != nil {
		fmt.Fprintln(stderr, "nkg:", err)
		return 1
	}
	defer gh.Close()

	if f.browser {
		return browse(ctx, gh, f.noOpen, stdout, stderr)
	}
	marker := filepath.Join(paths.StateDir(), "nkg-window.running")
	safe := useSafeGfx(f.safeGfx, os.Getenv(dmabufEnv) != "", fileExists(marker))
	if safe {
		_ = os.Setenv(dmabufEnv, "1")
		if !f.safeGfx {
			fmt.Fprintf(stderr, "nkg: a última janela não fechou direito; tentando com %s=1 (--safe-gfx)\n", dmabufEnv)
		}
	}
	done := markRunning(marker)
	if err := runWindow(ctx, gh.LaunchURL()); err != nil {
		fmt.Fprintln(stderr, "nkg:", err)
		return 1 // the marker stays: the next run tries the safe renderer
	}
	done()
	return 0
}

// browse is --browser: open the launch URL (or print it) and serve until
// ctx ends.
func browse(ctx context.Context, gh *guihost.Server, noOpen bool, stdout, stderr io.Writer) int {
	fmt.Fprintf(stdout, "nkg: servindo em %s (Ctrl+C encerra)\n", gh.URL())
	if noOpen {
		fmt.Fprintf(stdout, "nkg: abra (o link vale uma vez): %s\n", gh.LaunchURL())
	} else if err := openBrowser(gh.LaunchURL()); err != nil {
		fmt.Fprintln(stderr, "nkg: abrir o navegador:", err)
		fmt.Fprintf(stdout, "nkg: abra (o link vale uma vez): %s\n", gh.LaunchURL())
	}
	select {
	case <-ctx.Done():
	case <-gh.Done():
		if err := gh.Err(); err != nil {
			fmt.Fprintln(stderr, "nkg:", err)
			return 1
		}
	}
	return 0
}

// pickAssets is the frontend: the embedded one unless --dev or there is
// none, else the directory, which must hold a built index.html.
func pickAssets(dev bool, dir string, embedded fs.FS) (fs.FS, error) {
	if !dev && embedded != nil {
		return embedded, nil
	}
	if _, err := os.Stat(filepath.Join(dir, "index.html")); err != nil {
		return nil, fmt.Errorf("frontend não encontrado em %s (rode `make gui-build` ou `cd gui/web && npx vite build`, ou passe --assets)", dir)
	}
	return os.DirFS(dir), nil
}

// core is where /v1 goes: a running server (--connect) or one built here.
// closeCore ends what core started.
func core(ctx context.Context, f flags, stdout, stderr io.Writer) (guihost.Upstream, func(), error) {
	if f.connect != "" {
		up, err := remote(ctx, f.connect, f.tokenFile)
		return up, func() {}, err
	}
	cfg := loadConfig(stderr)
	var h *host
	if f.demo {
		h = newDemoHost(cfg)
	} else {
		h = newHost(cfg, stderr)
	}
	// one server for the page and --listen: both see the same panes
	var l net.Listener
	var token, addr string
	if f.listen != "" {
		var err error
		if l, token, addr, err = listenRPC(f.listen); err != nil {
			h.Close()
			return guihost.Upstream{}, nil, err
		}
	}
	srv := rpc.New(h.app, rpcOptions(h, token))
	closers := []func(){h.Close, func() { _ = srv.Close() }}
	if l != nil {
		go func() { _ = srv.Serve(l) }() // ends with http.ErrServerClosed on Close
		fmt.Fprintf(stdout, "nkg: core também em %s\n", addr)
	}
	sctx, cancel := context.WithCancel(ctx)
	ended := make(chan struct{})
	go func() {
		defer close(ended)
		h.app.RunSchedule(sctx, app.ScheduleFrom(cfg))
	}()
	closers = append(closers, func() { cancel(); <-ended })
	return guihost.Upstream{Handler: srv.Handler()}, func() {
		for i := len(closers) - 1; i >= 0; i-- {
			closers[i]()
		}
	}, nil
}

// remote checks that the server at addr answers (with the token from
// tokenFile for TCP) before the page is served.
func remote(ctx context.Context, addr, tokenFile string) (guihost.Upstream, error) {
	up := guihost.Upstream{Addr: addr}
	if !strings.HasPrefix(addr, "unix://") {
		tok, err := rpc.ReadTokenFile(tokenFile)
		if err != nil {
			return up, fmt.Errorf("token de %s: %w", addr, err)
		}
		up.Token = tok
	}
	c, err := rpc.Dial(addr, up.Token)
	if err != nil {
		return up, err
	}
	defer c.Close()
	hctx, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()
	if _, err := c.Hello(hctx); err != nil {
		return up, fmt.Errorf("%s não responde: %w", addr, err)
	}
	return up, nil
}

func loadConfig(stderr io.Writer) config.Config {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(stderr, "nkg: %s: %v (usando padrões)\n", config.Path(), err)
	}
	return cfg
}

// openBrowser opens url in the default browser (tests swap it).
var openBrowser = func(url string) error {
	name := "xdg-open"
	if runtime.GOOS == "darwin" {
		name = "open"
	}
	cmd := exec.Command(name, url)
	if err := cmd.Start(); err != nil {
		return err
	}
	go func() { _ = cmd.Wait() }()
	return nil
}

func fileExists(p string) bool {
	_, err := os.Stat(p)
	return err == nil
}
