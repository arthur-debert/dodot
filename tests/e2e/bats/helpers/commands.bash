#!/usr/bin/env bash
# CLI wrapper and brew-muzzle escape hatch.

dodot() {
  "$DODOT_BIN" "$@"
}

unhide_brew_for_test() {
  if [[ -z "${DODOT_E2E_ORIGINAL_PATH:-}" ]]; then
    echo "unhide_brew_for_test: DODOT_E2E_ORIGINAL_PATH not set; sandbox_setup must run first" >&2
    return 1
  fi
  export PATH="$DODOT_E2E_ORIGINAL_PATH"
}
