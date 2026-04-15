#!/usr/bin/env bash
# E2E test harness — sandbox lifecycle and dodot wrapper.
#
# Every BATS test gets an isolated filesystem sandbox mirroring
# the structure that TempEnvironment creates in the Rust test suite.
#
# Usage in .bats files:
#   setup()    { load helpers/setup; sandbox_setup; }
#   teardown() { sandbox_teardown; }

# Resolve the project root relative to BATS_TEST_DIRNAME
_E2E_HELPERS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_E2E_PROJECT_ROOT="$(cd "$_E2E_HELPERS_DIR/../../../.." && pwd)"

# Where the compiled dodot binary lives.
# Set DODOT_BIN externally to override (e.g. in CI or Docker).
DODOT_BIN="${DODOT_BIN:-$_E2E_PROJECT_ROOT/target/release/dodot}"

# Source sibling helpers
# shellcheck source=fixtures.bash
source "$_E2E_HELPERS_DIR/fixtures.bash"
# shellcheck source=assertions.bash
source "$_E2E_HELPERS_DIR/assertions.bash"

# ── Sandbox lifecycle ───────────────────────────────────────────

sandbox_setup() {
    SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/dodot-e2e-XXXXXXXXXX")"

    # Mirror TempEnvironment layout
    SANDBOX_HOME="$SANDBOX/home"
    SANDBOX_DOTFILES="$SANDBOX_HOME/dotfiles"
    SANDBOX_CONFIG_HOME="$SANDBOX_HOME/.config"
    SANDBOX_DATA_HOME="$SANDBOX_HOME/.local/share"
    SANDBOX_CACHE_HOME="$SANDBOX_HOME/.cache"

    mkdir -p \
        "$SANDBOX_HOME" \
        "$SANDBOX_DOTFILES" \
        "$SANDBOX_CONFIG_HOME" \
        "$SANDBOX_DATA_HOME/dodot/packs" \
        "$SANDBOX_DATA_HOME/dodot/shell" \
        "$SANDBOX_CACHE_HOME/dodot"

    # Redirect all env vars into the sandbox
    export HOME="$SANDBOX_HOME"
    export DOTFILES_ROOT="$SANDBOX_DOTFILES"
    export XDG_CONFIG_HOME="$SANDBOX_CONFIG_HOME"
    export XDG_DATA_HOME="$SANDBOX_DATA_HOME"
    export XDG_CACHE_HOME="$SANDBOX_CACHE_HOME"

    # Prevent git from escaping the sandbox
    export GIT_CEILING_DIRECTORIES="$SANDBOX"

    # Initialize a git repo in the dotfiles root so that
    # discover_dotfiles_root()'s git-toplevel fallback stays contained
    git init -q "$SANDBOX_DOTFILES"

    # Plain text output for reliable grep/assertion matching
    export NO_COLOR=1
}

sandbox_teardown() {
    if [[ -n "${SANDBOX:-}" && -d "$SANDBOX" ]]; then
        rm -rf "$SANDBOX"
    fi
}

# ── dodot wrapper ───────────────────────────────────────────────

# Run the dodot binary.
# Usage:  dodot status
#         dodot up --dry-run
#         dodot up vim
dodot() {
    "$DODOT_BIN" "$@"
}
