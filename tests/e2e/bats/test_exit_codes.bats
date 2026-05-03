#!/usr/bin/env bats
# Exit-code contract: every standout-dispatched subcommand must exit
# non-zero when its handler returns Err. Regression for dodot#86 /
# standout#141 (fixed in standout 7.6.2): pre-fix, the dispatcher
# stuffed handler errors into `RunResult::Handled`, so the CLI printed
# `Error: ...` but exited 0 — scripts piping with `&&` and CI invocations
# saw success on every failure path.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "status on a nonexistent pack exits non-zero with an error message" {
    run dodot status nonexistent-pack
    [ "$status" -ne 0 ]
    assert_output_contains "pack not found"
}

@test "up on a nonexistent pack exits non-zero with an error message" {
    run dodot up nonexistent-pack
    [ "$status" -ne 0 ]
    assert_output_contains "pack not found"
}

@test "down on a nonexistent pack exits non-zero with an error message" {
    run dodot down nonexistent-pack
    [ "$status" -ne 0 ]
    assert_output_contains "pack not found"
}

@test "adopt --into on a nonexistent pack exits non-zero with an error message" {
    create_home_file ".vimrc" "set nocompatible"
    run dodot adopt --into nonexistent-pack "$HOME/.vimrc"
    [ "$status" -ne 0 ]
    assert_output_contains "pack not found"
}

@test "adopt of a nonexistent source exits non-zero with an error message" {
    create_pack "vim"
    run dodot adopt --into vim "$HOME/.does-not-exist"
    [ "$status" -ne 0 ]
    assert_output_contains "source does not exist"
}
