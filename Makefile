# Bookley Key Trainer - repo-root front door.
# `make` or `make help` lists the targets.

# Find cargo even when rustup was just installed by `make deps` and this shell's PATH
# does not know about ~/.cargo/bin yet.
CARGO ?= $(shell command -v cargo 2>/dev/null || echo $(HOME)/.cargo/bin/cargo)
FAKE_CLAUDE := $(abspath tests/fake_claude.sh)

.DEFAULT_GOAL := help

.PHONY: help deps build run dev test smoke live-book-smoke lint icons screenshot \
        install uninstall install-claude desktop-install clean

# Everything `make install` puts on the system, so future versions know exactly what to
# replace and `make uninstall` knows exactly what to remove. The binary is fully
# self-contained (fonts, sounds, word list, and the novelist plugin are embedded; the
# plugin re-stages itself into the data dir at runtime). User data (books, settings,
# stats) lives under ~/.config and ~/.local/share/bookley-key-trainer and is NEVER
# touched by install or uninstall. Books especially are sacred.
INSTALL_BIN := $(HOME)/.local/bin/bookley-key-trainer
INSTALL_DESKTOP := $(HOME)/.local/share/applications/bookley-key-trainer.desktop
# Pre-rename installs used the short "bookley" name; install and uninstall clean those
# up too so an upgrade never leaves stale copies behind.
LEGACY_BIN := $(HOME)/.local/bin/bookley
LEGACY_DESKTOP := $(HOME)/.local/share/applications/bookley.desktop
ICON_SIZES := 16 32 48 64 128 256

help: ## List every target with a one-line description
	@echo "Bookley Key Trainer - make targets:"
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  %-18s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

deps: ## Install missing build dependencies: system libraries and the Rust toolchain (uses sudo only when something is missing)
	@if command -v apt-get >/dev/null 2>&1; then \
		missing=""; \
		command -v cc >/dev/null 2>&1 || missing="$$missing build-essential"; \
		command -v pkg-config >/dev/null 2>&1 || missing="$$missing pkg-config"; \
		command -v curl >/dev/null 2>&1 || missing="$$missing curl"; \
		pkg-config --exists alsa 2>/dev/null || missing="$$missing libasound2-dev"; \
		pkg-config --exists wayland-client 2>/dev/null || missing="$$missing libwayland-dev"; \
		pkg-config --exists xkbcommon 2>/dev/null || missing="$$missing libxkbcommon-dev"; \
		pkg-config --exists x11 2>/dev/null || missing="$$missing libx11-dev"; \
		if [ -n "$$missing" ]; then \
			echo "Installing missing system packages:$$missing (asks for your sudo password)"; \
			sudo apt-get update && sudo apt-get install -y $$missing; \
		else \
			echo "System libraries: all present."; \
		fi; \
	else \
		echo "Non-apt system: install the equivalents of build-essential, pkg-config,"; \
		echo "curl, libasound2-dev, libwayland-dev, libxkbcommon-dev, and libx11-dev"; \
		echo "with your package manager, then re-run make deps for the Rust check."; \
	fi
	@if [ -x "$(CARGO)" ] || command -v cargo >/dev/null 2>&1; then \
		echo "Rust: $$($(CARGO) --version)"; \
	else \
		echo "Installing the Rust toolchain via rustup (https://rustup.rs)..."; \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
		echo "Rust installed under ~/.cargo. This Makefile finds it by itself, so you"; \
		echo "can run make run right away; new terminals pick it up automatically."; \
	fi
	@echo "Dependencies OK."

build: ## Compile the app (release)
	$(CARGO) build --release

run: ## Build and run the app
	$(CARGO) run --release

dev: ## Run with developer shortcuts (F9 auto-type, F10 page, F12 chapter) and a DEV badge
	$(CARGO) run -- --dev

test: ## Run the unit and integration test suite (never touches real claude)
	BOOKLEY_CLAUDE_BIN=$(FAKE_CLAUDE) $(CARGO) test

smoke: ## Scriptable end-to-end self-test against the fake claude (no real usage, no auth)
	BOOKLEY_CLAUDE_BIN=$(FAKE_CLAUDE) $(CARGO) run -- --smoke

live-book-smoke: ## OPT-IN: one real chapter via the real claude CLI (consumes subscription usage)
	@echo "This generates one real chapter with the claude CLI and consumes subscription usage."
	$(CARGO) run -- --smoke --live

lint: ## cargo clippy + rustfmt check
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) fmt --check

icons: ## Regenerate icon PNGs (hicolor layout + window icon) from assets/icon/bookley-key-trainer.svg
	$(CARGO) run --bin gen-icons

screenshot: ## Boot the app, render a few frames, save smoke-screenshot.png, exit
	$(CARGO) run -- --screenshot smoke-screenshot.png

install: build ## Install for the current user: ~/.local/bin binary, desktop entry, icons (never touches books/settings)
	@rm -f $(INSTALL_BIN) $(INSTALL_DESKTOP) $(LEGACY_BIN) $(LEGACY_DESKTOP)
	@for s in $(ICON_SIZES); do \
		rm -f $(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley-key-trainer.png \
			$(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley.png; \
	done
	install -Dm755 target/release/bookley-key-trainer $(INSTALL_BIN)
	install -Dm644 assets/bookley-key-trainer.desktop $(INSTALL_DESKTOP)
	@for s in $(ICON_SIZES); do \
		install -Dm644 assets/icon/hicolor/$${s}x$${s}/apps/bookley-key-trainer.png \
			$(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley-key-trainer.png; \
	done
	@echo "Installed $(INSTALL_BIN) plus the desktop entry and icons."
	@echo "The binary is self-contained: the cloned repo can be deleted."
	@echo "Make sure ~/.local/bin is on your PATH."
	@echo "Your books, settings, and stats live under ~/.local/share/bookley-key-trainer"
	@echo "and ~/.config, and are never touched by install or uninstall."

uninstall: ## Remove the installed binary, desktop entry, and icons. Books/settings/stats are NEVER removed
	rm -f $(INSTALL_BIN) $(INSTALL_DESKTOP) $(LEGACY_BIN) $(LEGACY_DESKTOP)
	@for s in $(ICON_SIZES); do \
		rm -f $(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley-key-trainer.png \
			$(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley.png; \
	done
	@echo "Removed the app. Your books, settings, and stats remain untouched."

install-claude: ## Install the Claude Code CLI from Anthropic's APT repo (Debian/Ubuntu, uses sudo; no-op if present)
	@if command -v claude >/dev/null 2>&1; then \
		echo "claude is already installed: $$(command -v claude) ($$(claude --version 2>/dev/null))"; \
	elif command -v apt-get >/dev/null 2>&1; then \
		echo "Installing Claude Code from Anthropic's APT repository (needs sudo)..."; \
		sudo install -d -m 0755 /etc/apt/keyrings && \
		sudo curl -fsSL https://downloads.claude.ai/keys/claude-code.asc -o /etc/apt/keyrings/claude-code.asc && \
		echo "deb [signed-by=/etc/apt/keyrings/claude-code.asc] https://downloads.claude.ai/claude-code/apt/stable stable main" | sudo tee /etc/apt/sources.list.d/claude-code.list && \
		sudo apt-get update && \
		sudo apt-get install -y claude-code; \
	else \
		echo "apt not available; using Anthropic's native installer..."; \
		curl -fsSL https://claude.ai/install.sh | bash; \
	fi
	@echo "Done. Authenticate inside the app: Book mode -> Connect Claude."

desktop-install: ## Install the .desktop entry and icons for the current user (~/.local/share)
	install -Dm644 assets/bookley-key-trainer.desktop $(INSTALL_DESKTOP)
	@for s in $(ICON_SIZES); do \
		install -Dm644 assets/icon/hicolor/$${s}x$${s}/apps/bookley-key-trainer.png \
			$(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley-key-trainer.png; \
	done
	@echo "Installed. You may need to refresh your desktop's icon cache."

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -f smoke-screenshot.png
