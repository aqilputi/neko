GO            ?= go
GOLANGCI_LINT ?= golangci-lint
BIN           := nekuroddo
PREFIX        ?= $(HOME)/.local
COVERPROFILE  ?= coverage.out
COVER_MIN     ?= 80

.PHONY: all build install test cover cover-check cover-html lint fmt vet ci clean

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
