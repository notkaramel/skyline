# Skyline — Wayland status bar
#
# Usage:
#   make            # release build
#   make run        # build + run
#   make install    # install binary + man to /usr/local (needs sudo)
#   make uninstall  # remove installed files
#   make help       # list targets

PREFIX      ?= /usr/local
BINDIR      := $(PREFIX)/bin
MANDIR      := $(PREFIX)/share/man/man1
CARGO       ?= cargo
PACKAGE     := skyline
BINARY      := skyline
TARGET_DIR  := target
RELEASE_BIN := $(TARGET_DIR)/release/$(BINARY)
DEBUG_BIN   := $(TARGET_DIR)/debug/$(BINARY)
EXAMPLE_CFG := examples/config.toml
USER_CFG    := $(HOME)/.config/skyline/config.toml
MANPAGE     := docs/skyline.1

.PHONY: all build build-debug run run-debug install uninstall \
        config example-config clean check help

all: build

## Build release binary
build:
	$(CARGO) build --release -p $(PACKAGE)

## Build debug binary
build-debug:
	$(CARGO) build -p $(PACKAGE)

## Release build then run
run: build
	$(RELEASE_BIN)

## Debug build then run
run-debug: build-debug
	$(DEBUG_BIN)

## Install binary + man page to $(PREFIX) (default /usr/local)
install: build
	install -d "$(DESTDIR)$(BINDIR)"
	install -m 755 "$(RELEASE_BIN)" "$(DESTDIR)$(BINDIR)/$(BINARY)"
	install -d "$(DESTDIR)$(MANDIR)"
	install -m 644 "$(MANPAGE)" "$(DESTDIR)$(MANDIR)/skyline.1"
	@echo "installed $(DESTDIR)$(BINDIR)/$(BINARY)"
	@echo "installed $(DESTDIR)$(MANDIR)/skyline.1"

## Remove binary and man page from $(PREFIX)
uninstall:
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
	@echo "Skyline Makefile"
	@echo ""
	@echo "  make / make build   release build -> $(RELEASE_BIN)"
	@echo "  make build-debug    debug build"
	@echo "  make run            build + run (release)"
	@echo "  make run-debug      build + run (debug)"
	@echo "  make install        install binary + man to $(PREFIX) (sudo make install)"
	@echo "  make uninstall      remove binary + man"
	@echo "  make config         copy example config if missing"
	@echo "  make example-config force-write config via CLI"
	@echo "  make check          cargo check"
	@echo "  make clean          cargo clean"
	@echo ""
	@echo "Variables: PREFIX=$(PREFIX)  DESTDIR=$(DESTDIR)"
	@echo "Man page:  man skyline  (after install)"
