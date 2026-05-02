#!/usr/bin/env bats
# Regression guard for the macOS-side Homebrew probe code path.
#
# `sandbox_setup` (helpers/setup.bash) hides `brew` from the sandbox
# PATH by default — see the block comment there for why (Homebrew's
# analytics phone-home turns a 30s CI suite into a 7-minute local
# suite). The default-no-brew sandbox matches CI exactly: probes
# short-circuit to their empty-set branches because `brew` isn't
# found, and assertions about correctness still pass because the
# probe is documented as advisory, never authoritative
# (`docs/reference/symlink-paths.lex` §10).
#
# THIS file is the one place the suite explicitly opts back into the
# real `brew` binary, via `unhide_brew_for_test`. Without it, a future
# regression in the probe code (a stray panic, a hang on a slow
# subprocess, a misparsed JSON shape) could ship without any test
# coverage on a real-brew host. Adding this single bats file is the
# minimum cost to keep that surface alive — every other test stays in
# the fast lane.
#
# Tests skip when `brew` isn't on the host PATH or when no cask is
# installed. Linux CI hits the first skip; macOS dev machines without
# any cask hit the second.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

# Resolve the host's *original* brew path (sandbox_setup hid it from
# $PATH). Returns 1 (and the test should skip) when brew isn't
# installed on this host.
host_has_brew() {
    local IFS=':'
    local entry
    for entry in $DODOT_E2E_ORIGINAL_PATH; do
        if [[ -n "$entry" && -x "$entry/brew" ]]; then
            return 0
        fi
    done
    return 1
}

# Pick a well-known cask whose token matches its app-support folder
# name (so the probe's enrichment lookup succeeds). Returns the
# *folder* name on stdout when one of these casks is installed; empty
# string otherwise. Test should skip on empty.
#
# Limited to (force_app default, cask token) pairs that ship in
# `crates/dodot-lib/src/handlers/symlink.rs` defaults — that way the
# probe match is deterministic without hardcoding what's installed
# on any single dev machine.
#
# Calls real brew via its host-PATH location explicitly. The sandbox
# muzzle (`hide_brew_from_path` in helpers/setup.bash) prepends a
# non-zero `brew` shim to $PATH; resolving via $DODOT_E2E_ORIGINAL_PATH
# bypasses that shim and finds the real binary directly.
host_brew_bin() {
    local IFS=':'
    local entry
    for entry in $DODOT_E2E_ORIGINAL_PATH; do
        if [[ -n "$entry" && -x "$entry/brew" ]]; then
            echo "$entry/brew"
            return 0
        fi
    done
    return 1
}

first_known_force_app_cask_folder() {
    local brew_bin
    brew_bin="$(host_brew_bin)" || return 1
    local installed
    installed="$(HOMEBREW_NO_ANALYTICS=1 HOMEBREW_NO_AUTO_UPDATE=1 \
        "$brew_bin" list --cask --versions 2>/dev/null | awk '{print $1}')"
    # Pairs: <cask token>:<app-support folder>. Folders match the
    # default force_app list. visual-studio-code → Code, cursor → Cursor,
    # zed → Zed, emacs → Emacs.
    local pair
    for pair in 'visual-studio-code:Code' 'cursor:Cursor' 'zed:Zed' 'emacs:Emacs'; do
        local token="${pair%%:*}"
        local folder="${pair##*:}"
        if grep -qx "$token" <<< "$installed"; then
            echo "$folder"
            return 0
        fi
    done
    return 1
}

# ── Default sandbox: brew is hidden ─────────────────────────────

@test "default sandbox hides brew with a non-zero shim (probe falls through to empty)" {
    # Sanity check that sandbox_setup did its job. Without this guard,
    # a refactor that drops the muzzle silently re-introduces the slow
    # path and only shows up as a 7-minute suite — too easy to miss.
    #
    # The shim lives under $SANDBOX, exits non-zero, and is prepended
    # to PATH. dodot's probe runs it, gets a failure, falls through.
    local resolved
    resolved="$(command -v brew)"
    [[ "$resolved" == "$SANDBOX/"* ]] || {
        echo "expected brew shim under \$SANDBOX, got: $resolved" >&2
        return 1
    }
    run brew anything
    [ "$status" -ne 0 ]
    [ -z "$output" ]
}

@test "probe app succeeds without brew (advisory, never authoritative)" {
    create_pack_file "macapps" "Code/User/settings.json" '{}'

    run dodot probe app macapps
    [ "$status" -eq 0 ]
    # Basic layout still renders.
    assert_output_contains "App-support probe"
    assert_output_contains "Code"
    assert_output_contains "force_app"
    # No brew → no cask enrichment lines.
    assert_output_not_contains "cask:"
}

# ── Opt-in: real brew probe ─────────────────────────────────────

@test "probe app surfaces cask metadata when real brew is available" {
    if ! host_has_brew; then
        skip "real brew not installed on this host (Linux CI / fresh macOS)"
    fi

    local folder
    folder="$(first_known_force_app_cask_folder)" || folder=""
    if [[ -z "$folder" ]]; then
        skip "no recognized cask installed (visual-studio-code, cursor, zed, emacs)"
    fi

    # Build a pack with the matching app-support folder. `Code` (and
    # the other defaults above) ship in the default force_app list, so
    # no extra config is needed — the probe will route the entry to
    # `<app_support>/<folder>/...` and look up the cask.
    create_pack_file "macapps" "$folder/marker" 'x'

    unhide_brew_for_test

    run dodot probe app macapps
    [ "$status" -eq 0 ]
    assert_output_contains "App-support probe"
    # The cask-enrichment line is the brew probe's signature output.
    # If this fails on a real-brew host, something in
    # `crates/dodot-lib/src/probe/brew.rs` regressed — the probe
    # itself, the JSON parsing, or the output formatter.
    assert_output_contains "cask:"
}

@test "unhide_brew_for_test is reversible per-test (next test gets the muzzle back)" {
    # Each test runs sandbox_setup fresh, so the previous test's
    # unhide_brew_for_test must not leak. This pins that contract.
    local resolved
    resolved="$(command -v brew)"
    [[ "$resolved" == "$SANDBOX/"* ]] || {
        echo "expected brew shim under \$SANDBOX, got: $resolved" >&2
        return 1
    }
}
