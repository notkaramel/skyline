# Skyline — Wayland status bar
#
# Usage (no sudo; binary stays in ./target):
#   make            # release build → target/release/skyline
#   make run        # run target/release/skyline (no build)
#   make install    # same as build (local artifact only)
#   make help

CARGO       ?= cargo
PACKAGE     := skyline
BINARY      := skyline
TARGET_DIR  := target
RELEASE_BIN := $(TARGET_DIR)/release/$(BINARY)
DEBUG_BIN   := $(TARGET_DIR)/debug/$(BINARY)
EXAMPLE_CFG := examples/config.toml
USER_CFG    := $(HOME)/.config/skyline/config.toml
MANPAGE     := docs/skyline.1

# Optional user-local install root (never /usr/local). Override if needed:
#   make install-user PREFIX=$$HOME/.local
PREFIX      ?= $(HOME)/.local
BINDIR      := $(PREFIX)/bin
MANDIR      := $(PREFIX)/share/man/man1

.PHONY: all build build-debug run run-debug install install-user uninstall-user \
        config example-config clean check help

all: build

## Build release binary into ./target/release
build:
	$(CARGO) build --release -p $(PACKAGE)
	@echo "built $(RELEASE_BIN)"

## Build debug binary into ./target/debug
build-debug:
	$(CARGO) build -p $(PACKAGE)
	@echo "built $(DEBUG_BIN)"

## Run release binary from ./target (does not build)
run:
	@test -x "$(RELEASE_BIN)" || { echo "missing $(RELEASE_BIN) — run: make build"; exit 1; }
	@exec $(RELEASE_BIN)

## Run debug binary from ./target (does not build)
run-debug:
	@test -x "$(DEBUG_BIN)" || { echo "missing $(DEBUG_BIN) — run: make build-debug"; exit 1; }
	@exec $(DEBUG_BIN)

## Local build only — does not copy into /usr or require sudo
install: build
	@echo "skyline is ready at $(RELEASE_BIN)"
	@echo "run:  make run"
	@echo "or:   $(RELEASE_BIN)"
	@echo "(optional user install: make install-user)"

## Optional: install binary + man under $PREFIX (default: ~/.local), no sudo
install-user: build
	install -d "$(DESTDIR)$(BINDIR)"
	install -m 755 "$(RELEASE_BIN)" "$(DESTDIR)$(BINDIR)/$(BINARY)"
	install -d "$(DESTDIR)$(MANDIR)"
	install -m 644 "$(MANPAGE)" "$(DESTDIR)$(MANDIR)/skyline.1"
	@echo "installed $(DESTDIR)$(BINDIR)/$(BINARY)"
	@echo "installed $(DESTDIR)$(MANDIR)/skyline.1"

## Remove files installed by install-user
uninstall-user:
	rm -f "$(DESTDIR)$(BINDIR)/$(BINARY)"
	rm -f "$(DESTDIR)$(MANDIR)/skyline.1"
	@echo "removed $(DESTDIR)$(BINDIR)/$(BINARY)"
	@echo "removed $(DESTDIR)$(MANDIR)/skyline.1"

## Write example config to ~/.config/skyline/config.toml (no overwrite)
config: build
	@if [ -f "$(USER_CFG)" ]; then \
		echo "config already exists: $(USER_CFG)"; \
	else \
		mkdir -p "$$(dirname "$(USER_CFG)")"; \
		cp "$(EXAMPLE_CFG)" "$(USER_CFG)"; \
		echo "wrote $(USER_CFG)"; \
	fi

## Force-write example config via skyline CLI
example-config: build
	$(RELEASE_BIN) --write-example-config

## Typecheck
check:
	$(CARGO) check -p $(PACKAGE)

## Remove cargo build artifacts
clean:
	$(CARGO) clean

## Show available targets
help:
	@echo "Skyline Makefile (local builds — no sudo, nothing in /usr/local)"
	@echo ""
	@echo "  make / make build   release build -> $(RELEASE_BIN)"
	@echo "  make build-debug    debug build  -> $(DEBUG_BIN)"
	@echo "  make run            run $(RELEASE_BIN) (no build)"
	@echo "  make run-debug      run $(DEBUG_BIN) (no build)"
	@echo "  make install        alias for local build (stays in target/)"
	@echo "  make install-user   optional copy to $(PREFIX) (default ~/.local)"
	@echo "  make uninstall-user remove install-user files"
	@echo "  make config         copy example config if missing"
	@echo "  make example-config force-write config via CLI"
	@echo "  make check          cargo check"
	@echo "  make clean          cargo clean"
	@echo ""
	@echo "Variables: PREFIX=$(PREFIX)  DESTDIR=$(DESTDIR)"
