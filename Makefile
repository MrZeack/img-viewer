SHELL := bash
.SHELLFLAGS := -euo pipefail -c
.DEFAULT_GOAL := help

BIN := img_viewer
INSTALL_NAME := img
INSTALL_DIR ?= $(HOME)/.local/bin
NATIVE ?= 1

.PHONY: help install build check bump

help: ## Lista los targets
	@awk -F':.*## ' '/^[a-z-]+:.*## / {printf "  %-8s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

install: ## Valida herramientas, compila en release e instala `img` en INSTALL_DIR
	@for t in cargo rustc pkg-config; do \
		command -v $$t >/dev/null || { echo "Falta '$$t'. Rust: https://rustup.rs · macOS: brew install pkg-config libheif · Debian/Ubuntu: apt install pkg-config libheif-dev libheif-plugin-libde265 libfontconfig1-dev"; exit 1; }; \
	done
	@pkg-config --exists libheif || { echo "Falta libheif. macOS: brew install libheif · Debian/Ubuntu: apt install libheif-dev libheif-plugin-libde265"; exit 1; }
	cargo fetch --locked
	$(if $(filter 1,$(NATIVE)),RUSTFLAGS="-C target-cpu=native") cargo build --release --locked
	install -d "$(INSTALL_DIR)"
	install -m 755 target/release/$(BIN) "$(INSTALL_DIR)/$(INSTALL_NAME)"
	@echo "Instalado: $(INSTALL_DIR)/$(INSTALL_NAME)"
	@case ":$$PATH:" in *":$(INSTALL_DIR):"*) ;; *) echo "Aviso: $(INSTALL_DIR) no está en PATH";; esac

build: ## Compila el binario release portable en target/release
	cargo build --release --locked

check: ## fmt, clippy, tests y tests operativos
	cargo fmt --all -- --check
	cargo clippy --all-targets --locked -- -D warnings
	cargo test --locked
	bash scripts/operations.test.sh
	bash scripts/bump.test.sh

bump: ## Versión + CHANGELOG + commit y tag (TYPE=patch|minor|major o VERSION=X.Y.Z; sin TYPE se infiere)
	@scripts/bump.sh
