# Build / deploy / run helpers for pcso-results-downloader.
#
# Overridable variables — pass on the command line or export in your env:
#   make deploy SSH_HOST=my-other-pi
#
# Defaults assume Apple Silicon Mac → 64-bit DietPi (Pi 3/4/5) reachable via
# the SSH alias `dietpi` (set up in ~/.ssh/config).

# ---- Configurable ----------------------------------------------------------
TARGET      ?= aarch64-unknown-linux-gnu
GLIBC       ?= 2.36
SSH_HOST    ?= dietpi
PI_BIN_DIR  ?= /mnt/apps/pcso
# If empty (default), the binary uses the standard AWS credential chain
# (AWS_PROFILE env var, then [default] in ~/.aws/credentials, then env vars).
# Pass PROFILE=name to force a specific named profile.
PROFILE     ?=

# S3 bucket the binary uploads to. Required by the CLI; the Makefile checks
# for it before invoking any run/deploy target. Override per-call or export
# in your shell:   export BUCKET=my-bucket
BUCKET      ?=

# Expands to `--profile <name>` when PROFILE is set, otherwise nothing.
PROFILE_ARG := $(if $(PROFILE),--profile $(PROFILE),)
BUCKET_ARG  := $(if $(BUCKET),--bucket $(BUCKET),)

# Helper for run targets — fails fast if BUCKET isn't set rather than letting
# the binary error out mid-run.
require-bucket = @test -n "$(BUCKET)" || { echo "✗ BUCKET=<s3-bucket> is required (export BUCKET=… or pass it on the command line)"; exit 1; }

# ---- Derived ---------------------------------------------------------------
BIN_NAME    := pcso-results-downloader
BUILD_DIR   := target/$(TARGET)/release
LOCAL_BIN   := $(BUILD_DIR)/$(BIN_NAME)
SSH         := ssh $(SSH_HOST)
RSYNC       := rsync -avz --progress

.DEFAULT_GOAL := help

# ---- Help ------------------------------------------------------------------
.PHONY: help
help:  ## Show this help.
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ---- Configurable: Mac side ------------------------------------------------
MAC_INSTALL_DIR ?= /usr/local/bin
MAC_BIN          := target/release/$(BIN_NAME)

# ---- Build -----------------------------------------------------------------
.PHONY: build
build:  ## Cross-compile a release binary for the Pi.
	cargo zigbuild --release --target $(TARGET).$(GLIBC)

.PHONY: build-mac
build-mac:  ## Build a release binary for the local Mac.
	cargo build --release

.PHONY: check
check:  ## Fast type-check without producing a binary.
	cargo check

.PHONY: test
test:  ## Run unit tests on the local machine.
	cargo test

# ---- Local (Mac) install / run --------------------------------------------
.PHONY: install-mac
install-mac: build-mac  ## Build + copy the binary into $(MAC_INSTALL_DIR) (default /usr/local/bin).
	@test -w $(MAC_INSTALL_DIR) || { echo "✗ $(MAC_INSTALL_DIR) not writable — try: sudo make install-mac, or override MAC_INSTALL_DIR=$$HOME/bin"; exit 1; }
	install -m 0755 $(MAC_BIN) $(MAC_INSTALL_DIR)/$(BIN_NAME)
	@echo "✓ Installed $(MAC_INSTALL_DIR)/$(BIN_NAME)"

.PHONY: run-mac
run-mac: build-mac  ## Build + run on Mac for today (headless first; falls back to --headed if Akamai blocks).
	$(call require-bucket)
	$(MAC_BIN) $(BUCKET_ARG) $(PROFILE_ARG)

.PHONY: run-mac-headed
run-mac-headed: build-mac  ## Build + run on Mac with a visible (minimized) browser window.
	$(call require-bucket)
	$(MAC_BIN) --headed --minimize $(BUCKET_ARG) $(PROFILE_ARG)

.PHONY: run-mac-date
run-mac-date: build-mac  ## Build + run on Mac for a specific date: make run-mac-date FROM=06-30-2026 [TO=07-01-2026]
	$(call require-bucket)
	@test -n "$(FROM)" || { echo "✗ FROM=MM-dd-yyyy required"; exit 1; }
	$(MAC_BIN) --from $(FROM) $(if $(TO),--to $(TO),) $(BUCKET_ARG) $(PROFILE_ARG)

# ---- Deploy ----------------------------------------------------------------
.PHONY: deploy
deploy: build  ## Build + rsync the binary to the Pi (SSH_HOST=alias).
	$(SSH) "mkdir -p $(PI_BIN_DIR)"
	$(RSYNC) $(LOCAL_BIN) $(SSH_HOST):$(PI_BIN_DIR)/
	$(SSH) "chmod +x $(PI_BIN_DIR)/$(BIN_NAME)"
	@echo "✓ Deployed to $(SSH_HOST):$(PI_BIN_DIR)/$(BIN_NAME)"

.PHONY: deploy-only
deploy-only:  ## rsync the existing binary without rebuilding.
	@test -f $(LOCAL_BIN) || { echo "✗ $(LOCAL_BIN) missing — run 'make build' first"; exit 1; }
	$(SSH) "mkdir -p $(PI_BIN_DIR)"
	$(RSYNC) $(LOCAL_BIN) $(SSH_HOST):$(PI_BIN_DIR)/
	$(SSH) "chmod +x $(PI_BIN_DIR)/$(BIN_NAME)"

# ---- Remote run ------------------------------------------------------------
.PHONY: run-pi
run-pi:  ## Run the deployed binary on the Pi for today's date (Asia/Manila).
	$(call require-bucket)
	$(SSH) "$(PI_BIN_DIR)/$(BIN_NAME) $(BUCKET_ARG) $(PROFILE_ARG)"

.PHONY: run-pi-date
run-pi-date:  ## Run for a specific date: make run-pi-date FROM=06-30-2026 [TO=07-01-2026]
	$(call require-bucket)
	@test -n "$(FROM)" || { echo "✗ FROM=MM-dd-yyyy required"; exit 1; }
	$(SSH) "$(PI_BIN_DIR)/$(BIN_NAME) --from $(FROM) $(if $(TO),--to $(TO),) $(BUCKET_ARG) $(PROFILE_ARG)"

.PHONY: ssh
ssh:  ## Shorthand: ssh into the Pi.
	$(SSH)

# ---- Housekeeping ----------------------------------------------------------
.PHONY: clean
clean:  ## Remove all build artifacts.
	cargo clean

.PHONY: pi-info
pi-info:  ## Print Pi arch + glibc to help pick TARGET/GLIBC.
	@$(SSH) "uname -m && ldd --version | head -1"
