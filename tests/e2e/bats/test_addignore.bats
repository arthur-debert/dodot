#!/usr/bin/env bats
# E2E tests for `dodot addignore`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "addignore creates .dodotignore file" {
    create_pack_file "scratch" "notes" "x"

    run dodot addignore scratch
    [ "$status" -eq 0 ]
    assert_output_contains "ignored"

    assert_exists "$DOTFILES_ROOT/scratch/.dodotignore"
}

@test "addignore is idempotent" {
    create_pack_file "scratch" "notes" "x"
    mark_ignored "scratch"

    run dodot addignore scratch
    [ "$status" -eq 0 ]
    assert_output_contains "already ignored"
}

@test "addignore makes pack show as ignored in list" {
    create_pack_file "scratch" "notes" "x"

    dodot addignore scratch
    run dodot list
    [ "$status" -eq 0 ]
    assert_output_contains "scratch"
    assert_output_contains "(ignored)"
}

@test "addignore makes pack skipped by status" {
    create_pack_file "scratch" "notes" "x"

    dodot addignore scratch
    run dodot status
    [ "$status" -eq 0 ]
    # Ignored packs appear under "Ignored Packs" so users aren't
    # baffled, but their contents are not scanned.
    assert_output_contains "Ignored Packs"
    assert_output_contains "scratch"
    assert_output_not_contains "notes"
}
