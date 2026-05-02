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

    hide_brew_from_path
}

# ── Homebrew probe muzzle ───────────────────────────────────────
#
# Why this lives at the *base* setup level, not in individual tests:
#
# dodot's macOS-side advisory probes (probe::brew::list_installed_casks
# in `crates/dodot-lib/src/probe/brew.rs`) shell out to `brew` for
# enrichment — cask metadata in `dodot probe app`, sibling-adoption
# tips in `dodot adopt`, missing-folder hints in `dodot up`/`status`.
# On any host with Homebrew installed, every `brew` invocation spawns
# two `curl` processes that phone home to Homebrew's analytics
# endpoint with `--max-time 3`. Across the suite this turns a sub-30s
# CI run (no brew) into a 7-minute local run.
#
# The shim below makes the *default* match CI: dodot's probe finds
# `brew` on PATH, runs it, gets a non-zero exit, and falls through to
# its empty-set branches. The probe is documented as advisory, never
# authoritative (see `docs/reference/symlink-paths.lex` §10), so
# correctness assertions pass without real cask data. Tests that need
# to exercise the real brew code path call `unhide_brew_for_test`
# explicitly — making real brew exposure opt-in, not opt-out, so the
# next person who touches PATH handling can't accidentally re-
# introduce the slowdown.
#
# Why a shim, not a PATH strip: removing the homebrew bin dir from
# PATH wholesale also removes everything else that lives there
# (homebrew bash 5+, gnu-coreutils on Linux, host-specific tools
# tests rely on). The shim hides only `brew` itself.
#
# The HOMEBREW_NO_* env vars are belt-and-suspenders: if a future
# test escapes the shim via its own PATH twiddling, at least the
# analytics phone-home stays disabled.
hide_brew_from_path() {
    # Snapshot the original PATH for opt-in helpers (unhide_brew_for_test).
    if [[ -z "${DODOT_E2E_ORIGINAL_PATH:-}" ]]; then
        export DODOT_E2E_ORIGINAL_PATH="$PATH"
    fi

    # Per-sandbox shim dir prepended to PATH. Lives under SANDBOX so
    # sandbox_teardown wipes it; nothing leaks across tests.
    local shim_dir="$SANDBOX/.brew-muzzle"
    mkdir -p "$shim_dir"
    cat > "$shim_dir/brew" <<'SHIM'
#!/bin/sh
# E2E sandbox brew shim — see helpers/setup.bash for the rationale.
# Exits non-zero with no output so dodot's advisory brew probe falls
# through to its empty-set branch, matching CI's no-brew behavior.
exit 1
SHIM
    chmod +x "$shim_dir/brew"
    export PATH="$shim_dir:$PATH"

    export HOMEBREW_NO_ANALYTICS=1
    export HOMEBREW_NO_AUTO_UPDATE=1
    export HOMEBREW_NO_INSTALL_FROM_API=1
}

# Restore the pre-muzzle PATH so a single test can exercise the real
# `brew` probe code path. The muzzle (set up in `hide_brew_from_path`)
# prepends a non-zero `brew` shim to PATH; this helper drops that
# prepended shim by reverting to the snapshotted original. Most tests
# should NOT call this — see the block comment above.
# `tests/e2e/bats/test_brew_probe.bats` is the canonical example and
# the regression guard for the probe code.
#
# This explicit opt-in lives at the helper level (not inline in tests)
# so the "I want real brew here" intent is grep-able and reviewers
# can see at a glance which tests escape the muzzle.
unhide_brew_for_test() {
    if [[ -z "${DODOT_E2E_ORIGINAL_PATH:-}" ]]; then
        echo "unhide_brew_for_test: DODOT_E2E_ORIGINAL_PATH not set; sandbox_setup must run first" >&2
        return 1
    fi
    export PATH="$DODOT_E2E_ORIGINAL_PATH"
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
