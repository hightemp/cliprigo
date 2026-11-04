# Shortcuts for the usual development commands. `make` lists them.

CARGO := cd src-tauri && cargo
# Android ABIs to build: aarch64 for most phones, armv7 for older ones, x86_64 for the emulator.
ANDROID_TARGETS ?= aarch64 armv7

.DEFAULT_GOAL := help
.PHONY: help install dev build build-debug android lint check fmt clippy \
	test test-ui test-rust test-live preview preview-mobile icons clean

help: ## Show this list
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | \
		awk -F':.*## ' '{ printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 }'

install: ## Install JavaScript dependencies
	pnpm install

dev: ## Run the app in development mode
	pnpm tauri dev

build: ## Build installers for this system
	pnpm tauri build

build-debug: ## Debug build without installers (src-tauri/target/debug/cliprigo)
	pnpm tauri build --debug --no-bundle

android: ## Build Android APKs, one per ABI (ANDROID_TARGETS="aarch64 armv7")
	pnpm tauri android build --apk --split-per-abi --target $(ANDROID_TARGETS)

lint: ## Everything CI checks: Prettier, svelte-check, frontend tests, rustfmt, clippy
	pnpm lint
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings

check: ## Type-check the frontend
	pnpm check

fmt: ## Format all code
	pnpm format
	$(CARGO) fmt

clippy: ## Rust lints
	$(CARGO) clippy --all-targets -- -D warnings

test: test-ui test-rust ## Frontend and Rust tests

test-ui: ## Frontend unit tests
	pnpm test

test-rust: ## Rust tests
	$(CARGO) test

test-live: ## End-to-end tests against real YouTube (network)
	$(CARGO) test live_ -- --ignored --test-threads=1

preview: ## UI scenarios in headless Chrome, screenshots in ui-preview/
	pnpm ui:preview

preview-mobile: ## The same at phone size
	PREVIEW_MOBILE=1 pnpm ui:preview

icons: ## Regenerate app, Android and browser icons
	pnpm icons

clean: ## Remove build output (Rust target, Android build, frontend build)
	$(CARGO) clean
	rm -rf build .svelte-kit ui-preview \
		src-tauri/gen/android/app/build src-tauri/gen/android/build \
		src-tauri/gen/android/buildSrc/build
