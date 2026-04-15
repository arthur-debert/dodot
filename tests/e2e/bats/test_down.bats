#!/usr/bin/env bats
# E2E tests for `dodot down`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "down removes deployed state" {
    create_pack_file "vim" "vimrc" "x"

    dodot up
    assert_exists "$XDG_DATA_HOME/dodot/packs/vim/symlink/vimrc"

    run dodot down
    [ "$status" -eq 0 ]
    assert_output_contains "deactivated"

    # Datastore state should be gone
    assert_no_handler_state "vim" "symlink"
}

@test "down makes status return to pending" {
    create_pack_file "vim" "vimrc" "x"

    dodot up
    dodot down

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "pending"
    assert_output_not_contains "deployed"
}

@test "down --dry-run shows plan without changes" {
    create_pack_file "vim" "vimrc" "x"

    dodot up
    run dodot down --dry-run
    [ "$status" -eq 0 ]
    assert_output_contains "dry run"

    # State should still be deployed
    run dodot status
    assert_output_contains "deployed"
}

@test "down removes selected packs only" {
    create_pack_file "vim" "vimrc" "x"
    create_pack_file "git" "gitconfig" "x"

    dodot up
    dodot down vim

    # vim should be pending, git still deployed
    run dodot status vim
    assert_output_contains "pending"
    run dodot status git
    assert_output_contains "deployed"
}

@test "down on already-inactive packs is safe" {
    create_pack_file "vim" "vimrc" "x"

    # Never deployed — down should succeed
    run dodot down
    [ "$status" -eq 0 ]
}

@test "down removes shell handler state" {
    create_pack_file "zsh" "aliases.sh" "alias ll='ls -la'"

    dodot up
    assert_exists "$XDG_DATA_HOME/dodot/packs/zsh/shell/aliases.sh"

    dodot down
    assert_no_handler_state "zsh" "shell"
}

@test "down removes path handler state" {
    create_pack "tools"
    create_pack_bin "tools" "mytool" '#!/bin/sh\necho hello'

    dodot up
    assert_exists "$XDG_DATA_HOME/dodot/packs/tools/path/bin"

    dodot down
    assert_no_handler_state "tools" "path"
}
