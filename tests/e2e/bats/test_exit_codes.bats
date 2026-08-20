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

# A failing provisioning command is the case scripts actually hit:
# `dodot up && ./next-step.sh` used to continue against a machine whose
# install script had failed, because `up` reported success no matter
# what. The failure is now contained to its own file *and* reaches the
# exit code — both halves are asserted together, since either one alone
# would leave the user worse off.

@test "up exits non-zero when an install script fails, and still deploys the rest of the pack" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
echo "something went wrong" >&2
exit 1'
    create_pack_file "tools" "home.vimrc" "set nocompatible"

    run dodot up
    [ "$status" -ne 0 ]

    # The symlink in the same pack deployed anyway — provisioning runs
    # before the link phase, so this is what a propagating failure cost.
    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/tools/symlink/home.vimrc"
}

@test "a clean up exits zero" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
exit 0'
    create_pack_file "tools" "home.vimrc" "set nocompatible"

    run dodot up
    [ "$status" -eq 0 ]
}

@test "up --dry-run exits zero even when the script it previews would fail" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
exit 1'

    run dodot up --dry-run
    [ "$status" -eq 0 ]
    # Nothing ran, so there is nothing to have failed.
    assert_not_exists "$XDG_DATA_HOME/dodot/packs/tools/install"
}

@test "a failed install script leaves no sentinel, so the next up retries it" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
exit 1'

    run dodot up
    [ "$status" -ne 0 ]
    assert_not_exists "$XDG_DATA_HOME/dodot/packs/tools/install"
}
