#!/usr/bin/env bats
# E2E tests for `dodot init`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "init creates pack directory with config" {
    run dodot init newpack
    [ "$status" -eq 0 ]
    assert_output_contains "newpack"

    assert_dir_exists "$DOTFILES_ROOT/newpack"
    assert_exists "$DOTFILES_ROOT/newpack/.dodot.toml"
}

@test "init reports error if pack already exists" {
    create_pack "existing"
    create_pack_file "existing" "file" "x"

    run dodot init existing
    assert_output_contains "already exists"
}

@test "init creates pack that shows in list" {
    dodot init mypack

    run dodot list
    [ "$status" -eq 0 ]
    assert_output_contains "mypack"
}
