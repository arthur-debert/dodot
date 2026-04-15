#!/usr/bin/env bats
# E2E tests for full dodot lifecycle (up → status → down → status).

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "full lifecycle: up → status → down → status" {
    create_pack_file "vim" "vimrc" "set nocompatible"
    create_pack_file "git" "gitconfig" "[user]\n  name = test"

    # 1. Status before up — all pending
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "pending"
    assert_output_not_contains "deployed"

    # 2. Up
    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "deployed"

    # 3. Status after up — deployed
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "deployed"

    # Symlinks exist
    assert_exists "$HOME/.vimrc"
    assert_exists "$HOME/.gitconfig"

    # 4. Down
    run dodot down
    [ "$status" -eq 0 ]
    assert_output_contains "deactivated"

    # 5. Status after down — pending again
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "pending"
    assert_output_not_contains "deployed"
}

@test "lifecycle with re-deploy is idempotent" {
    create_pack_file "vim" "vimrc" "set nocompatible"

    dodot up
    run dodot status
    assert_output_contains "deployed"

    dodot down
    run dodot status
    assert_output_contains "pending"

    # Re-deploy
    dodot up
    run dodot status
    assert_output_contains "deployed"

    # Symlink chain works
    assert_file_contains "$HOME/.vimrc" "set nocompatible"
}

@test "lifecycle with mixed handlers" {
    create_pack_file "dev" "vimrc" "set nocompatible"
    create_pack_file "dev" "aliases.sh" "alias g=git"
    create_pack "dev"
    create_pack_bin "dev" "devtool" '#!/bin/sh\necho dev'

    dodot up

    # All handlers should be active
    run dodot status
    assert_output_contains "deployed"
    assert_output_contains "sourced"
    assert_output_contains "in PATH"

    dodot down

    run dodot status
    assert_output_contains "pending"
    assert_output_contains "not sourced"
    assert_output_contains "not in PATH"
}

@test "selective up then full down" {
    create_pack_file "vim" "vimrc" "x"
    create_pack_file "git" "gitconfig" "x"

    # Deploy only vim
    dodot up vim
    run dodot status vim
    assert_output_contains "deployed"
    run dodot status git
    assert_output_contains "pending"

    # Down everything
    dodot down
    run dodot status
    assert_output_not_contains "deployed"
}
