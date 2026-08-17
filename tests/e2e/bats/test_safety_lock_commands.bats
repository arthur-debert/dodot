#!/usr/bin/env bats
# Safety Lock across the whole command surface, in a real process.
#
# `test_safety_lock.bats` proves the gate itself — the prompt, the terminal
# requirement, the exit statuses. This file proves the *taxonomy*: every
# family in ADR-0002's protected set crosses the gate (including the routes
# Standout does not dispatch: `config` and the tutorial), every documented
# read-only and dry-run variant bypasses it, the commands ADR-0002 excludes
# stay excluded, and the cache-derived writers only mutate sources inside
# the authorized root.
#
# Like its sibling, this file unsets the DOTFILES_ROOT the rest of the suite
# exports and runs from inside the root, so every ungated invocation here
# exercises *implicit* discovery — the case the gate exists for.

setup() {
    load helpers/setup
    sandbox_setup

    SAFETY_ROOT="$DOTFILES_ROOT"
    export SAFETY_ROOT
    SAFETY_STATE="$XDG_DATA_HOME/dodot/safety-lock.toml"
    export SAFETY_STATE

    create_pack_file "vim" "home.vimrc" "set nocompatible"

    unset DOTFILES_ROOT
    cd "$SAFETY_ROOT" || return 1
}

teardown() {
    sandbox_teardown
}

# See test_safety_lock.bats for why the pauses are load-bearing.
pty_is_util_linux() {
    script --version 2>&1 | grep -q util-linux
}

pty_run() {
    local keystrokes="$1"
    shift

    if ! command -v script >/dev/null 2>&1; then
        skip "script(1) is needed for a real pseudo-terminal"
    fi

    local delay="${PTY_INPUT_DELAY:-0.5}"
    local out=""
    status=0
    if pty_is_util_linux; then
        local quoted
        printf -v quoted '%q ' "$@"
        out="$({ sleep "$delay"; printf '%s' "$keystrokes"; sleep "$delay"; } |
            script -qec "$quoted" /dev/null 2>&1)" || status=$?
    else
        out="$({ sleep "$delay"; printf '%s' "$keystrokes"; sleep "$delay"; } |
            script -q /dev/null "$@" 2>&1)" || status=$?
    fi
    output="$out"
}

# ── Every protected dispatch family refuses untrusted ───────────

@test "every mutating dispatch family refuses on an untrusted implicit root" {
    local families=(
        "down"
        "init newpack"
        "fill vim"
        "adopt --into vim $HOME/some-file"
        "addignore vim"
        "git-install-filters"
        "template install-filter"
        "transform install-hook"
        "refresh"
        "transform check"
    )

    local family
    for family in "${families[@]}"; do
        # shellcheck disable=SC2086
        run dodot $family
        [ "$status" -eq 1 ] || {
            echo "dodot $family exited $status, expected the gate's 1" >&2
            return 1
        }
        echo "$output" | grep -q "has not been approved" || {
            echo "dodot $family failed for a reason other than the gate: $output" >&2
            return 1
        }
    done

    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$SAFETY_ROOT/newpack"
}

# ── config: the passthrough route crosses the same gate ─────────

@test "config set on an untrusted implicit root refuses and writes nothing" {
    local out err rc=0
    out="$SANDBOX/config.out"
    err="$SANDBOX/config.err"

    "$DODOT_BIN" config set profiling.enabled true >"$out" 2>"$err" || rc=$?

    [ "$rc" -eq 1 ]
    # The refusal is the gate's own diagnostic, verbatim — stdout stays
    # empty so a consumer cannot mistake it for a result.
    [ ! -s "$out" ]
    grep -q "has not been approved" "$err"
    assert_not_exists "$SAFETY_ROOT/.dodot.toml"
    assert_not_exists "$SAFETY_STATE"
}

@test "config reads and generation stay available on an untrusted root" {
    run dodot config list
    [ "$status" -eq 0 ]
    assert_output_contains "symlink"

    run dodot config get pack.ignore
    [ "$status" -eq 0 ]

    assert_not_exists "$SAFETY_STATE"
}

@test "config set with an explicit DOTFILES_ROOT persists into that root without approval" {
    DOTFILES_ROOT="$SAFETY_ROOT" run dodot config set profiling.enabled true
    [ "$status" -eq 0 ]

    assert_exists "$SAFETY_ROOT/.dodot.toml"
    assert_file_contains "$SAFETY_ROOT/.dodot.toml" "enabled = true"
    # The deliberate selection records no trust, same as `up`.
    assert_not_exists "$SAFETY_STATE"
}

@test "approving at the config set prompt records trust and persists the value" {
    pty_run "y
" "$DODOT_BIN" config set profiling.enabled true

    [ "$status" -eq 0 ]
    assert_exists "$SAFETY_STATE"
    assert_file_contains "$SAFETY_STATE" "$SAFETY_ROOT"
    assert_file_contains "$SAFETY_ROOT/.dodot.toml" "enabled = true"

    # The approval is the same one every other mutation consumes.
    run dodot up
    [ "$status" -eq 0 ]
    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.vimrc"
}

# ── tutorial: the real deployment step crosses the gate ─────────

@test "the tutorial's real deployment step is gated on an untrusted implicit root" {
    # Enter accepts each tutorial prompt's default (intro, check_root,
    # pick-pack select, two press-enters, targets, dry-run) until the real
    # deployment step puts the root to Safety Lock's own prompt — where a
    # bare Enter is a refusal, the destructive-action convention. The pack
    # is config-only so the shell-integration step never fires, and the
    # surplus keystrokes after the refusal are never read.
    PTY_INPUT_DELAY=2 pty_run "$(printf '\n%.0s' {1..10})" "$DODOT_BIN" tutorial --reset

    [ "$status" -eq 1 ]
    assert_output_contains "Approve this root?"
    assert_output_contains "left untrusted"
    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

# ── The excluded set stays excluded (ADR-0002) ──────────────────

@test "mutations the root did not select run without trust" {
    # The write probe spawns `$SHELL -ic`; the CI image ships bash.
    export SHELL=/bin/bash

    # Dismissed-prompt management: Dodot's own registry.
    run dodot prompts reset --all
    [ "$status" -eq 0 ]

    # Shell-rc wiring: the user's rc file, chosen by shell, not by root.
    run dodot git-install-alias --shell zsh
    [ "$status" -eq 0 ]
    assert_exists "$HOME/.zshrc"

    # The shell hookup writes the rc file the shell selected.
    run dodot install --write --rc "$HOME/custom-rc"
    [ "$status" -eq 0 ]
    assert_exists "$HOME/custom-rc"

    assert_not_exists "$SAFETY_STATE"
}

@test "stdin/stdout filter passthroughs run without trust" {
    printf '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0"><dict><key>k</key><string>v</string></dict></plist>\n' > "$SANDBOX/in.plist"

    run bash -c "'$DODOT_BIN' plist clean < '$SANDBOX/in.plist'"
    [ "$status" -eq 0 ]
    assert_output_contains "plist"

    assert_not_exists "$SAFETY_STATE"
}

@test "factory reset stays available without trust" {
    run dodot reset --force
    [ "$status" -eq 0 ]
    assert_not_exists "$SAFETY_STATE"
}

# ── Cache-derived writers scope to the authorized root ──────────

@test "two roots sharing one cache: refresh mutates only the authorized root's sources" {
    # Root A: the sandbox's dotfiles repo, with a template pack.
    create_pack "appa"
    create_pack_file "appa" "cfg-a.toml.tmpl" 'name = {{ name }}'
    create_pack_config "appa" '[preprocessor.template.vars]\nname = "Alice"'

    # Root B: a second dotfiles root sharing the same XDG data dir (and so
    # the same preprocessor baseline cache).
    local root_b="$HOME/dotfiles-b"
    mkdir -p "$root_b/appb"
    printf 'host = {{ host }}' > "$root_b/appb/cfg-b.toml.tmpl"
    printf '[preprocessor.template.vars]\nhost = "hb"\n' > "$root_b/appb/.dodot.toml"

    DOTFILES_ROOT="$SAFETY_ROOT" "$DODOT_BIN" up appa
    DOTFILES_ROOT="$root_b" "$DODOT_BIN" up appb

    local src_a="$SAFETY_ROOT/appa/cfg-a.toml.tmpl"
    local src_b="$root_b/appb/cfg-b.toml.tmpl"
    local mtime_a_before mtime_b_before
    mtime_a_before=$(mtime "$src_a")
    mtime_b_before=$(mtime "$src_b")

    # Edit both rendered files so both baselines diverge.
    sleep 1
    echo "name = Edited" > "$XDG_DATA_HOME/dodot/packs/appa/preprocessed/cfg-a.toml"
    echo "host = Edited" > "$XDG_DATA_HOME/dodot/packs/appb/preprocessed/cfg-b.toml"

    # Refresh authorized for root B: B's source is touched, A's is reported
    # as belonging to another root and left exactly as it was.
    DOTFILES_ROOT="$root_b" run dodot refresh
    [ "$status" -eq 0 ]
    assert_output_contains "Touched"
    assert_output_contains "other root"
    assert_output_contains "$src_a"

    [ "$(mtime "$src_a")" = "$mtime_a_before" ]
    [ "$(mtime "$src_b")" -gt "$mtime_b_before" ]
}

# ── Documented previews bypass without establishing trust ───────

@test "documented dry-run and read-only variants bypass the gate untrusted" {
    run dodot down --dry-run
    [ "$status" -eq 0 ]

    run dodot refresh --list-paths
    [ "$status" -eq 0 ]

    run dodot transform check --dry-run
    [ "$status" -eq 0 ]

    assert_not_exists "$SAFETY_STATE"
}
