# Bookley Key Trainer - repo-root front door.
# `make` or `make help` lists the targets.

CARGO ?= cargo
FAKE_CLAUDE := $(abspath tests/fake_claude.sh)

.DEFAULT_GOAL := help

.PHONY: help deps build run dev test smoke live-book-smoke lint icons screenshot \
        install uninstall install-claude desktop-install clean

# Everything `make install` puts on the system, so future versions know exactly what to
# replace and `make uninstall` knows exactly what to remove. The binary is fully
# self-contained (fonts, sounds, word list, and the novelist plugin are embedded; the
# plugin re-stages itself into the data dir at runtime). User data (books, settings,
# stats) lives under ~/.config and ~/.local/share/bookleykeytrainer and is NEVER
# touched by install or uninstall. Books especially are sacred.
INSTALL_BIN := $(HOME)/.local/bin/bookley
INSTALL_DESKTOP := $(HOME)/.local/share/applications/bookley.desktop
ICON_SIZES := 16 32 48 64 128 256

help: ## List every target with a one-line description
	@echo "Bookley Key Trainer - make targets:"
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  %-18s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

deps: ## Verify the toolchain and required system libraries
	@command -v $(CARGO) >/dev/null || { echo "cargo not found: install Rust via https://rustup.rs"; exit 1; }
	@command -v cc >/dev/null || { echo "cc not found: install a C toolchain (build-essential)"; exit 1; }
	@command -v pkg-config >/dev/null || { echo "pkg-config not found"; exit 1; }
	@pkg-config --exists alsa || { echo "ALSA dev headers not found: sudo apt install libasound2-dev (needed for the typewriter key sound)"; exit 1; }
	@rustc --version && cargo --version
	@echo "Toolchain OK. GUI needs the usual X11/Wayland dev libs; audio needs libasound2-dev."

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

icons: ## Regenerate icon PNGs (hicolor layout + window icon) from assets/icon/bookley.svg
	$(CARGO) run --bin gen-icons

screenshot: ## Boot the app, render a few frames, save smoke-screenshot.png, exit
	$(CARGO) run -- --screenshot smoke-screenshot.png

install: build ## Install for the current user: ~/.local/bin binary, desktop entry, icons (never touches books/settings)
	@rm -f $(INSTALL_BIN) $(INSTALL_DESKTOP)
	@for s in $(ICON_SIZES); do rm -f $(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley.png; done
	install -Dm755 target/release/bookley $(INSTALL_BIN)
	install -Dm644 assets/bookley.desktop $(INSTALL_DESKTOP)
	@for s in $(ICON_SIZES); do \
		install -Dm644 assets/icon/hicolor/$${s}x$${s}/apps/bookley.png \
			$(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley.png; \
	done
	@echo "Installed $(INSTALL_BIN) plus the desktop entry and icons."
	@echo "The binary is self-contained: the cloned repo can be deleted."
	@echo "Make sure ~/.local/bin is on your PATH."
	@echo "Your books, settings, and stats live under ~/.local/share/bookleykeytrainer"
	@echo "and ~/.config, and are never touched by install or uninstall."

uninstall: ## Remove the installed binary, desktop entry, and icons. Books/settings/stats are NEVER removed
	rm -f $(INSTALL_BIN) $(INSTALL_DESKTOP)
	@for s in $(ICON_SIZES); do rm -f $(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley.png; done
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
	install -Dm644 assets/bookley.desktop $(HOME)/.local/share/applications/bookley.desktop
	@for s in 16 32 48 64 128 256; do \
		install -Dm644 assets/icon/hicolor/$${s}x$${s}/apps/bookley.png \
			$(HOME)/.local/share/icons/hicolor/$${s}x$${s}/apps/bookley.png; \
	done
	@echo "Installed. You may need to refresh your desktop's icon cache."

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -f smoke-screenshot.png
