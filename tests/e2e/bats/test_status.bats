#!/usr/bin/env bats
# E2E tests for `dodot status`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "status shows pending before up" {
    create_pack_file "vim" "vimrc" "set nocompatible"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_contains "vimrc"
    assert_output_contains "pending"
}

@test "status shows deployed after up" {
    create_pack_file "vim" "vimrc" "set nocompatible"

    dodot up
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_contains "vimrc"
    assert_output_contains "deployed"
}

@test "status filters by pack name" {
    create_pack_file "vim" "vimrc" "x"
    create_pack_file "git" "gitconfig" "x"

    run dodot status vim
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_not_contains "git"
}

@test "status shows shell handler as sourced/not sourced" {
    create_pack_file "zsh" "aliases.sh" "alias ll='ls -la'"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "not sourced"

    dodot up
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "sourced"
    assert_output_not_contains "not sourced"
}

@test "status shows path handler as in PATH/not in PATH" {
    create_pack "tools"
    create_pack_bin "tools" "mytool" '#!/bin/sh\necho hello'

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "not in PATH"

    dodot up
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "in PATH"
}

@test "status shows install handler as never run" {
    create_pack_script "tools" "install.sh" '#!/bin/sh\necho done'

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "never run"
}

@test "status skips ignored packs" {
    create_pack_file "vim" "vimrc" "x"
    create_pack_file "disabled" "file" "x"
    mark_ignored "disabled"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_not_contains "disabled"
}

@test "status returns pending after down" {
    create_pack_file "vim" "vimrc" "x"

    dodot up
    dodot down
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "pending"
    assert_output_not_contains "deployed"
}
