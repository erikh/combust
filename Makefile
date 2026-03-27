SHORTCUT_DIR := $(HOME)/.config/taskwarrior-tui/shortcut-scripts
CONTRIB_DIR := contrib/taskwarrior-tui
SCRIPTS := $(wildcard $(CONTRIB_DIR)/*.py)

.PHONY: all lint test build install install-shortcuts

all: test

lint:
	cargo check --workspace
	cargo clippy --workspace --tests -- -D warnings

test: lint
	cargo test --workspace

build:
	cargo build --workspace

install:
	cargo install --path combust

install-shortcuts:
	mkdir -p $(SHORTCUT_DIR)
	@for s in $(SCRIPTS); do \
		ln -sf $(CURDIR)/$$s $(SHORTCUT_DIR)/$$(basename $$s); \
	done
	@echo "Symlinked shortcuts into $(SHORTCUT_DIR)"
