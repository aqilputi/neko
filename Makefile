GO            ?= $(shell command -v go 2>/dev/null || ls $(HOME)/.local/sdk/go/bin/go /usr/local/go/bin/go 2>/dev/null | head -1)
GOLANGCI_LINT ?= golangci-lint
BIN           := nekuroddo
PREFIX        ?= $(HOME)/.local
COVERPROFILE  ?= coverage.out
COVER_MIN     ?= 80
GUI_WEB       := gui/web
NPM           ?= npm
CARGO         ?= $(shell command -v cargo 2>/dev/null || ls $(HOME)/.cargo/bin/cargo 2>/dev/null | head -1)
OFFICE        := gui/office

.PHONY: all build install test cover cover-check cover-html lint fmt vet ci clean
.PHONY: gui-install gui-test gui-e2e gui-build build-gui run-gui-browser install-gui-web
.PHONY: office-build office-test office-lint run-office install-gui

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

# the legacy web nkg (Chrome/Chromium --app window), installed as nkg-web
install-gui-web:
	cd $(GUI_WEB) && npx vite build
	find $(NKG_DIST) -mindepth 1 ! -name .gitkeep -exec rm -rf {} +
	cp -r $(GUI_WEB)/dist/. $(NKG_DIST)/
	$(GO) build -o bin/nkg-web ./cmd/nekuroddo-gui
	install -d $(PREFIX)/bin
	install -m 0755 bin/nkg-web $(PREFIX)/bin/nkg-web

# ---- nkg, the native GUI (gui/office: Rust + Bevy) ----
# Needs a Rust toolchain plus the Bevy system libs (alsa, udev, vulkan,
# wayland/x11, xkbcommon); clang + lld link it (gui/office/.cargo/config.toml).
# It spawns `nk serve` by itself, so nk must be installed (make install).

office-build:
	cd $(OFFICE) && $(CARGO) build --release -p nkg

office-test:
	cd $(OFFICE) && $(CARGO) test --workspace

office-lint:
	cd $(OFFICE) && $(CARGO) fmt --all --check && $(CARGO) clippy --workspace --all-targets -- -D warnings

# the demo world in the native window
run-office:
	cd $(OFFICE) && $(CARGO) run --release -p nkg -- --demo

# nkg + the launcher entry and icon
install-gui: office-build
	install -d $(PREFIX)/bin $(PREFIX)/share/applications $(PREFIX)/share/icons/hicolor/256x256/apps
	install -m 0755 $(OFFICE)/target/release/nkg $(PREFIX)/bin/nkg
	install -m 0644 assets/logo-256.png $(PREFIX)/share/icons/hicolor/256x256/apps/nekuroddo.png
	sed 's#^Exec=nkg#Exec=$(PREFIX)/bin/nkg#' packaging/nekuroddo.desktop > $(PREFIX)/share/applications/nekuroddo.desktop
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null

# ---- nkw, the world (gui/office: Smithay + glow) ----
# docs/plano-mundo.md §9. world-perf needs a GPU session (or --headless by hand) and jq;
# world-cover needs cargo-llvm-cov. Crates of the list that do not exist yet are skipped.
.PHONY: world-build world-test world-lint world-cover run-world world-shot world-perf install-world
WORLD_ALL    := nk-cats nkw-core nkw-objects nkw-sys nkw-gl nkw nkw-testclient
WORLD_CRATES := $(foreach c,$(WORLD_ALL),$(if $(wildcard $(OFFICE)/crates/$(c)/Cargo.toml),-p $(c)))

world-build:
	cd $(OFFICE) && $(CARGO) build --profile world -p nkw -p nkw-testclient

world-test:
	cd $(OFFICE) && $(CARGO) test $(WORLD_CRATES)
	$(OFFICE)/scripts/test-world-perf.sh

world-lint:
	cd $(OFFICE) && $(CARGO) fmt $(WORLD_CRATES) --check && $(CARGO) clippy $(WORLD_CRATES) --all-targets -- -D warnings

world-cover:
	cd $(OFFICE) && $(CARGO) llvm-cov $(WORLD_CRATES) --fail-under-lines 90

run-world:
	cd $(OFFICE) && $(CARGO) run -p nkw -- --demo

world-shot:
	$(OFFICE)/scripts/world-shot.sh

world-perf: world-build
	$(OFFICE)/scripts/world-perf.sh --record

# app compatibility (docs/mundo.md, "Apps dentro do mundo"): the headless suite (testclient +
# the real apps installed, NKW_E2E=1 rows skip what is missing); world-compat-nested opens a
# nested nkw --demo with real apps (throwaway profiles) in your session for up to two minutes
# and rewrites $(OFFICE)/target/compat/matrix.md.
.PHONY: world-compat world-compat-nested
world-compat:
	$(OFFICE)/scripts/world-compat.sh --headless

world-compat-nested:
	@echo "world-compat-nested: abre janelas na sua sessão por até 2 minutos (apps com perfis descartáveis)"
	$(OFFICE)/scripts/world-compat.sh --nested

# nkw (profile world) + the launcher entry "nekuroddo — mundo" and the icon. nkw talks to
# nk serve like nkg does, so nk must be installed too (make install); docs/mundo.md.
install-world:
	cd $(OFFICE) && $(CARGO) build --profile world -p nkw
	install -d $(PREFIX)/bin $(PREFIX)/share/applications $(PREFIX)/share/icons/hicolor/256x256/apps
	install -m 0755 $(OFFICE)/target/world/nkw $(PREFIX)/bin/nkw
	install -m 0644 assets/logo-256.png $(PREFIX)/share/icons/hicolor/256x256/apps/nekuroddo.png
	sed 's#^Exec=nkw#Exec=$(PREFIX)/bin/nkw#' packaging/nekuroddo-mundo.desktop > $(PREFIX)/share/applications/nekuroddo-mundo.desktop
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null
