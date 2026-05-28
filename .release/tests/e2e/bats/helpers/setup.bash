#!/usr/bin/env bash
# Canonical BATS e2e setup. Synced from arthur-debert/release.
#
# Reads tests/e2e/bats.conf for project-specific configuration,
# then provides sandbox_setup/sandbox_teardown for use in .bats files:
#
#   setup()    { load helpers/setup; sandbox_setup; }
#   teardown() { sandbox_teardown; }
#
# See templates/components/bats/ in release/ for the full docs.

_HELPERS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_BATS_DIR="$(cd "$_HELPERS_DIR/.." && pwd)"
_E2E_DIR="$(cd "$_BATS_DIR/.." && pwd)"
_PROJECT_ROOT="$(cd "$_E2E_DIR/../.." && pwd)"

# shellcheck source=../../../../lib/bats-harness.bash
source "$_PROJECT_ROOT/lib/bats-harness.bash"
harness_set_root "$_PROJECT_ROOT"

# Read consumer config
_CONF="$_E2E_DIR/bats.conf"
# shellcheck source=/dev/null
[[ -f "$_CONF" ]] && source "$_CONF"

# Source consumer helpers
for _helper in ${EXTRA_HELPERS:-}; do
  # shellcheck source=/dev/null
  source "$_HELPERS_DIR/$_helper"
done
unset _helper

# Export BIN vars (skip if already set, e.g. from CI or check-e2e)
for _entry in ${BINS:-}; do
  _name="${_entry%%=*}"
  _path="${_entry#*=}"
  _var="$(echo "$_name" | tr '[:lower:]-' '[:upper:]_')_BIN"
  if [[ -z "${!_var:-}" ]]; then
    export "$_var=$_PROJECT_ROOT/$_path"
  fi
done
unset _entry _name _path _var

sandbox_setup() {
  if [[ "${ISOLATION:-per-test}" == "per-test" ]]; then
    harness_create_workspace_notrap
    export SANDBOX="$HARNESS_WORKSPACE"
    # shellcheck disable=SC2086
    harness_mkdir ${WORKSPACE_DIRS:-}
    # shellcheck disable=SC2086
    harness_git_init ${GIT_INIT_DIRS:-}
    if type -t e2e_env &>/dev/null; then
      e2e_env
    fi
  fi
}

sandbox_teardown() {
  if type -t e2e_teardown &>/dev/null; then
    e2e_teardown
  fi
  if [[ "${ISOLATION:-per-test}" == "per-test" ]]; then
    harness_cleanup
  fi
}
