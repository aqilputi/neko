GO            ?= $(shell command -v go 2>/dev/null || ls $(HOME)/.local/sdk/go/bin/go /usr/local/go/bin/go 2>/dev/null | head -1)
GOLANGCI_LINT ?= golangci-lint
BIN           := nekuroddo
PREFIX        ?= $(HOME)/.local
COVERPROFILE  ?= coverage.out
COVER_MIN     ?= 80
GUI_WEB       := gui/web
NPM           ?= npm

.PHONY: all build install test cover cover-check cover-html lint fmt vet ci clean
.PHONY: gui-install gui-test gui-e2e gui-build build-gui run-gui-browser install-gui

all: build

build:
	$(GO) build -o bin/$(BIN) ./cmd/$(BIN)

install: build
	install -d $(PREFIX)/bin
	install -m 0755 bin/$(BIN) $(PREFIX)/bin/$(BIN)
	ln -sf $(BIN) $(PREFIX)/bin/nk

test:
	$(GO) test -race ./...

cover:
	$(GO) test -race -coverprofile=$(COVERPROFILE) -covermode=atomic ./...
	$(GO) tool cover -func=$(COVERPROFILE) | tail -1

# cover-check fails below COVER_MIN% (the same gate CI runs)
cover-check: cover
	GO=$(GO) ./scripts/coverage-gate.sh $(COVERPROFILE) $(COVER_MIN)

cover-html: cover
	$(GO) tool cover -html=$(COVERPROFILE) -o coverage.html
	@echo "wrote coverage.html"

lint:
	$(GOLANGCI_LINT) run ./...

fmt:
	$(GOLANGCI_LINT) fmt ./...

vet:
	$(GO) vet ./...

ci: vet lint cover-check

clean:
	rm -rf bin $(COVERPROFILE) coverage.html

# ---- GUI frontend (gui/web: Vite + TS + Preact) ----

gui-install:
	cd $(GUI_WEB) && $(NPM) ci

# vitest with v8 coverage (≥ 80% lines on src/client and src/state)
gui-test:
	cd $(GUI_WEB) && npx vitest run --coverage

# Playwright smoke against nk serve --demo (builds nk with $(GO));
# first time: cd gui/web && npx playwright install chromium
gui-e2e:
	cd $(GUI_WEB) && GO="$(GO)" npx playwright test

# typecheck + vite build → gui/web/dist
gui-build:
	cd $(GUI_WEB) && npx tsc --noEmit && npx vite build

# ---- nkg (cmd/nekuroddo-gui: Wails v3 window + internal/guihost) ----
# The window needs cgo + GTK4 + WebKitGTK 6.0 (Arch/CachyOS:
# sudo pacman -S webkitgtk-6.0; Debian/Ubuntu: libgtk-4-dev
# libwebkitgtk-6.0-dev). The frontend is embedded from a copy of
# gui/web/dist (go:embed cannot reach outside the package).
NKG_DIST := cmd/nekuroddo-gui/dist

build-gui:
	cd $(GUI_WEB) && npx vite build
	find $(NKG_DIST) -mindepth 1 ! -name .gitkeep -exec rm -rf {} +
	cp -r $(GUI_WEB)/dist/. $(NKG_DIST)/
	$(GO) build -tags wails -o bin/nkg ./cmd/nekuroddo-gui

# no window, no WebKitGTK: the demo world in the default browser
run-gui-browser:
	cd $(GUI_WEB) && npx vite build
	$(GO) run ./cmd/nekuroddo-gui --browser --demo --dev

# nkg as a desktop app without WebKitGTK: the frontend embedded in the binary
# (no wails tag), opened in its own Chrome/Chromium --app window; plus the
# launcher entry and icon. With webkitgtk-6.0 installed, build-gui gives the
# native window instead.
install-gui:
	cd $(GUI_WEB) && npx vite build
	find $(NKG_DIST) -mindepth 1 ! -name .gitkeep -exec rm -rf {} +
	cp -r $(GUI_WEB)/dist/. $(NKG_DIST)/
	$(GO) build -o bin/nkg ./cmd/nekuroddo-gui
	install -d $(PREFIX)/bin $(PREFIX)/share/applications $(PREFIX)/share/icons/hicolor/256x256/apps
	install -m 0755 bin/nkg $(PREFIX)/bin/nkg
	install -m 0644 assets/logo-256.png $(PREFIX)/share/icons/hicolor/256x256/apps/nekuroddo.png
	sed 's#^Exec=nkg#Exec=$(PREFIX)/bin/nkg#' packaging/nekuroddo.desktop > $(PREFIX)/share/applications/nekuroddo.desktop
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null
