#!/usr/bin/env bash
# bats-harness.bash — Reusable lifecycle for BATS e2e test suites.
# Synced to consumers via release-sync from arthur-debert/release.
#
# Source this in your test runner, then call lifecycle functions in
# order. Each function does one thing; combine them to match your
# project's needs.
#
# SUITE-LEVEL ISOLATION (padz pattern)
# -------------------------------------
#   source lib/bats-harness.bash
#   harness_require_bats
#   harness_set_root "$SCRIPT_DIR/.."
#   harness_build "cargo build --bin myapp"
#   harness_create_workspace
#   harness_mkdir "data" "projects/a"
#   harness_git_init "projects/a"
#   export MY_BIN="$HARNESS_ROOT/target/debug/myapp"
#   export MY_DATA="$HARNESS_WORKSPACE/data"
#   harness_bats "$SCRIPT_DIR/tests" "$@"
#
# PER-TEST ISOLATION (helper.bash pattern)
# -----------------------------------------
#   source lib/bats-harness.bash
#   harness_set_root "$BATS_TEST_DIRNAME/../.."
#   setup()    { harness_create_workspace_notrap; cd "$HARNESS_WORKSPACE"; }
#   teardown() { cd /; harness_cleanup; }
#
# SPLIT PHASES (server-in-the-middle)
# ------------------------------------
#   harness_build "go build ./cmd/myapp"
#   harness_create_workspace
#   start_my_server          # consumer-specific
#   wait_for_health          # consumer-specific
#   harness_bats "$DIR" "$@"
#
# EXPORTED VARIABLES
# -------------------
#   HARNESS_ROOT       — project root directory
#   HARNESS_WORKSPACE  — isolated workspace (mktemp -d)

# --- State ----------------------------------------------------------------

HARNESS_ROOT=""
HARNESS_WORKSPACE=""

# --- Output ---------------------------------------------------------------

if [[ -t 2 ]]; then
  _harness_gray=$'\033[38;5;245m'
  _harness_red=$'\033[0;31m'
  _harness_nc=$'\033[0m'
else
  _harness_gray=""
  _harness_red=""
  _harness_nc=""
fi

_harness_status() {
  printf '%s%s%s\n' "$_harness_gray" "$1" "$_harness_nc" >&2
}

_harness_error() {
  printf '%s%s%s\n' "$_harness_red" "$1" "$_harness_nc" >&2
}

# --- Lifecycle functions --------------------------------------------------

harness_require_bats() {
  if ! command -v bats &>/dev/null; then
    _harness_error "bats not found. Install with: brew install bats-core"
    exit 1
  fi
}

harness_set_root() {
  HARNESS_ROOT="${1:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
  export HARNESS_ROOT
}

harness_build() {
  _harness_status "Building..."
  if ! ( cd "$HARNESS_ROOT" && eval "$1" ); then
    _harness_error "Build failed"
    exit 1
  fi
}

harness_create_workspace() {
  HARNESS_WORKSPACE="$(mktemp -d)"
  export HARNESS_WORKSPACE
  _harness_status "Workspace: $HARNESS_WORKSPACE"
  trap '_harness_exit_cleanup' EXIT INT TERM
}

_harness_exit_cleanup() {
  local rc=$?
  [[ -n "${HARNESS_WORKSPACE:-}" ]] && rm -rf "$HARNESS_WORKSPACE"
  exit $rc
}

harness_create_workspace_notrap() {
  HARNESS_WORKSPACE="$(mktemp -d)"
  export HARNESS_WORKSPACE
}

harness_cleanup() {
  if [[ -n "${HARNESS_WORKSPACE:-}" ]]; then
    rm -rf "$HARNESS_WORKSPACE"
    HARNESS_WORKSPACE=""
  fi
}

harness_mkdir() {
  for dir in "$@"; do
    mkdir -p "$HARNESS_WORKSPACE/$dir"
  done
}

harness_git_init() {
  for dir in "$@"; do
    ( cd "$HARNESS_WORKSPACE/$dir" && git init --quiet )
  done
}

harness_bats() {
  local tests_dir="$1"
  shift
  _harness_status "Running tests..."
  echo >&2
  if [[ $# -eq 0 ]]; then
    bats "$tests_dir"
  elif [[ -f "$tests_dir/$1" ]]; then
    local first="$tests_dir/$1"
    shift
    bats "$first" "$@"
  else
    bats "$@"
  fi
}
