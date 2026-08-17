#!/usr/bin/env bats
# Safety Lock in a real process: exit statuses, terminal detection, signals.
#
# The rest of the suite exports DOTFILES_ROOT, which is the deliberate
# selection path and therefore the one that never prompts. This file unsets it
# and runs from inside the root, so every test here exercises *implicit*
# discovery — the case the gate exists for. See `docs/spec/safety-lock.md`.
#
# What lives here rather than in the Rust harness: a real pseudo-terminal, a
# real SIGINT, and the process's actual exit status. Piping `yes` into a
# program is not a test that it required a terminal, so the affirmative path
# has to run under a PTY.

setup() {
    load helpers/setup
    sandbox_setup

    # The root the sandbox built, kept before DOTFILES_ROOT goes away.
    SAFETY_ROOT="$DOTFILES_ROOT"
    export SAFETY_ROOT
    SAFETY_STATE="$XDG_DATA_HOME/dodot/safety-lock.toml"
    export SAFETY_STATE

    create_pack_file "vim" "home.vimrc" "set nocompatible"
    create_pack_file "zsh" "aliases.sh" "alias ll='ls -la'"

    unset DOTFILES_ROOT
    cd "$SAFETY_ROOT" || return 1
}

teardown() {
    sandbox_teardown
}

# True when `script(1)` is the util-linux build, whose `-e` reports the
# child's exit status (128+signal for a killed child). The BSD/macOS build
# reports its own status for a signalled child, which is why the interrupt
# test only asserts the exact 130 on util-linux.
pty_is_util_linux() {
    script --version 2>&1 | grep -q util-linux
}

# Run `$@` with a real pseudo-terminal on stdin, stdout, and stderr, typing
# `$1` into it. Sets `output` and `status` the way bats' own `run` does.
#
# The pauses around the keystrokes are load-bearing. Input queued before the
# program reaches its read arrives as a closed stream, so every "the user
# answered X" test would silently become "the user hit EOF" and pass for the
# wrong reason — which is exactly the confusion between a piped answer and a
# typed one that this file exists to rule out.
#
# Skipped rather than degraded where no `script(1)` exists: a piped-stdin
# fallback would not be a terminal test at all.
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
        # util-linux: script [options] [file] -c command
        local quoted
        printf -v quoted '%q ' "$@"
        out="$({ sleep "$delay"; printf '%s' "$keystrokes"; sleep "$delay"; } |
            script -qec "$quoted" /dev/null 2>&1)" || status=$?
    else
        # BSD / macOS: script [-q] file command [args...]
        out="$({ sleep "$delay"; printf '%s' "$keystrokes"; sleep "$delay"; } |
            script -q /dev/null "$@" 2>&1)" || status=$?
    fi
    output="$out"
}

# ── Non-interactive refusal ─────────────────────────────────────

@test "a mutating command on an unapproved implicit root refuses without a terminal" {
    run dodot up
    [ "$status" -eq 1 ]
    assert_output_contains "has not been approved"

    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "refusal leaves stdout empty so a consumer cannot mistake it for a result" {
    local out err rc=0
    out="$SANDBOX/refusal.out"
    err="$SANDBOX/refusal.err"

    "$DODOT_BIN" up >"$out" 2>"$err" || rc=$?

    [ "$rc" -eq 1 ]
    [ ! -s "$out" ]
    grep -q "has not been approved" "$err"
}

@test "piped affirmative input cannot bypass the terminal requirement" {
    run bash -c "yes | '$DODOT_BIN' up"
    [ "$status" -eq 1 ]
    assert_output_contains "has not been approved"

    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "a terminal stdin with redirected stderr refuses" {
    # Approval must never be accepted for a root and an inventory that were
    # written to a channel the user was never looking at.
    local err="$SANDBOX/redirected.err"

    pty_run "y
" bash -c "'$DODOT_BIN' up 2>'$err'"

    [ "$status" -eq 1 ]
    grep -q "has not been approved" "$err"
    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "a wrong-root down on an untrusted root leaves deployed state intact" {
    # Deploy deliberately from the real root, then try to tear it down from
    # an unrelated directory that Dodot would otherwise adopt as the root.
    DOTFILES_ROOT="$SAFETY_ROOT" "$DODOT_BIN" up
    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.vimrc"

    mkdir -p "$HOME/unrelated"
    cd "$HOME/unrelated" || return 1

    run dodot down
    [ "$status" -eq 1 ]

    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.vimrc"
    assert_not_exists "$SAFETY_STATE"
}

# ── The paths that never prompt ─────────────────────────────────

@test "a dry run neither prompts nor establishes trust" {
    run dodot up --dry-run
    [ "$status" -eq 0 ]

    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "read-only commands work on an untrusted root" {
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "home.vimrc"

    assert_not_exists "$SAFETY_STATE"
}

@test "a valid DOTFILES_ROOT deploys without prompting and records no approval" {
    DOTFILES_ROOT="$SAFETY_ROOT" run dodot up
    [ "$status" -eq 0 ]

    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.vimrc"
    assert_not_exists "$SAFETY_STATE"
}

@test "an invalid DOTFILES_ROOT fails without falling back to git or cwd" {
    DOTFILES_ROOT="$HOME/no-such-root" run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "no-such-root"

    # The fallback root would have deployed; nothing did.
    assert_not_exists "$HOME/.vimrc"
    assert_not_exists "$SAFETY_STATE"
}

@test "an empty DOTFILES_ROOT fails rather than silently discovering a root" {
    DOTFILES_ROOT="" run dodot up
    [ "$status" -ne 0 ]

    assert_not_exists "$HOME/.vimrc"
    assert_not_exists "$SAFETY_STATE"
}

# ── Real terminal interaction ───────────────────────────────────

@test "the first-run prompt names the root, how it was selected, and what it found" {
    pty_run "n
" "$DODOT_BIN" up

    assert_output_contains "has not been approved"
    assert_output_contains "$SAFETY_ROOT"
    assert_output_contains "selected by"
    assert_output_contains "shell"
    assert_output_contains "link"
    assert_output_contains "home.vimrc"
    assert_output_contains "aliases.sh"
    assert_output_contains "Approve this root?"
}

@test "declining at the prompt refuses, exits 1, and writes nothing" {
    pty_run "n
" "$DODOT_BIN" up

    [ "$status" -eq 1 ]
    assert_output_contains "left untrusted"
    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "pressing enter at the prompt refuses" {
    # The destructive-action convention: a stray keypress must never approve.
    pty_run "
" "$DODOT_BIN" up

    [ "$status" -eq 1 ]
    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "an unrecognized answer refuses" {
    pty_run "maybe
" "$DODOT_BIN" up

    [ "$status" -eq 1 ]
    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "EOF at the prompt refuses" {
    pty_run "" "$DODOT_BIN" up

    [ "$status" -eq 1 ]
    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

@test "approving deploys, records the root, and the next run is not asked again" {
    pty_run "y
" "$DODOT_BIN" up

    [ "$status" -eq 0 ]
    assert_exists "$SAFETY_STATE"
    assert_file_contains "$SAFETY_STATE" "$SAFETY_ROOT"
    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.vimrc"

    # Second run: no terminal at all, and it still proceeds.
    run dodot up
    [ "$status" -eq 0 ]
    assert_output_not_contains "has not been approved"
}

@test "YES approves too — the answer is case-insensitive" {
    pty_run "YES
" "$DODOT_BIN" up

    [ "$status" -eq 0 ]
    assert_file_contains "$SAFETY_STATE" "$SAFETY_ROOT"
}

@test "interrupting the prompt leaves the root untrusted and nothing deployed" {
    # A longer pause than the other PTY tests. An answer that arrives early
    # just waits in the terminal's input buffer, but a ^C is acted on by the
    # line discipline the moment it lands — before dodot owns the foreground
    # process group, it would be delivered to something else entirely.
    PTY_INPUT_DELAY=2 pty_run "$(printf '\003')" "$DODOT_BIN" up

    if pty_is_util_linux; then
        # util-linux's `script -e` reports 128+signal for a killed child, so
        # the conventional interrupt status is observable end to end.
        [ "$status" -eq 130 ]
    fi

    assert_not_exists "$SAFETY_STATE"
    assert_not_exists "$HOME/.vimrc"
}

# ── Managing approvals ──────────────────────────────────────────

@test "roots list reports no approvals before anything is approved" {
    run dodot roots list
    [ "$status" -eq 0 ]
    assert_output_contains "No root has been approved"
}

@test "roots list and forget round-trip an approval" {
    pty_run "y
" "$DODOT_BIN" up

    run dodot roots list
    [ "$status" -eq 0 ]
    assert_output_contains "$SAFETY_ROOT"

    run dodot roots forget "$SAFETY_ROOT"
    [ "$status" -eq 0 ]
    assert_output_contains "Forgot"

    run dodot roots list
    assert_output_contains "No root has been approved"

    # And the gate is back.
    run dodot up
    [ "$status" -eq 1 ]
    assert_output_contains "has not been approved"
}

@test "roots list keeps stdout structured in json and yaml" {
    pty_run "y
" "$DODOT_BIN" up

    run dodot roots list --output json
    [ "$status" -eq 0 ]
    assert_output_contains '"roots"'
    assert_output_contains '"path"'

    run dodot roots list --output yaml
    [ "$status" -eq 0 ]
    assert_output_contains "roots:"
}

@test "roots list names its own styles in terminal-debug mode" {
    run dodot roots list --output term-debug
    [ "$status" -eq 0 ]
    assert_output_contains "[header]"
    # An undefined style renders as `[name?]`; none may appear.
    assert_output_not_contains "?]"
}

@test "roots forget reports an argument that matches no approval" {
    run dodot roots forget "$HOME/never-approved"
    [ "$status" -eq 0 ]
    assert_output_contains "No approved root matches"
}

@test "a trust file dodot cannot read stops mutation but not inspection" {
    mkdir -p "$(dirname "$SAFETY_STATE")"
    printf '[roots]\napproved = ["relative/dotfiles"]\n' >"$SAFETY_STATE"

    run dodot status
    [ "$status" -eq 0 ]

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "safety-lock.toml"
}

@test "factory reset removes a trust file dodot cannot parse" {
    mkdir -p "$(dirname "$SAFETY_STATE")"
    printf 'this is not toml at all\n' >"$SAFETY_STATE"

    run dodot reset --force
    [ "$status" -eq 0 ]

    assert_not_exists "$SAFETY_STATE"
}
