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
    create_pack_file "vim" "home.vimrc" "set nocompatible"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_contains "vimrc"
    assert_output_contains "pending"
}

@test "status shows deployed after up" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"

    dodot up
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_contains "vimrc"
    assert_output_contains "deployed"
}

@test "status filters by pack name" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "git" "home.gitconfig" "x"

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

@test "status skips ignored packs from main listing but shows them as ignored" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "disabled" "file" "x"
    mark_ignored "disabled"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    # Ignored packs are not scanned/deployed, but are listed under an
    # "Ignored Packs" heading so users aren't baffled when a directory
    # they expected doesn't appear in the main listing.
    assert_output_contains "Ignored Packs"
    assert_output_contains "disabled"
    # The ignored pack's contents should NOT be scanned or shown.
    assert_output_not_contains "file"
}

@test "status returns pending after down" {
    create_pack_file "vim" "home.vimrc" "x"

    dodot up
    dodot down
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "pending"
    assert_output_not_contains "deployed"
}

@test "status --short collapses each pack to one summary line" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "nvim" "home.config/nvim/init.lua" "x"

    run dodot status --short
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_contains "nvim"
    assert_output_contains "(1) pending"
    # Short mode hides per-file rows
    assert_output_not_contains "vimrc"
    assert_output_not_contains "init.lua"
}

@test "status --by-status groups packs under banners" {
    create_pack_file "vim" "home.vimrc" "x"

    run dodot status --by-status
    [ "$status" -eq 0 ]
    assert_output_contains "Pending Packs"
    assert_output_contains "vim"
    # No deployed or error packs — banners for empty groups are hidden
    assert_output_not_contains "Deployed Packs"
    assert_output_not_contains "Error Packs"
}

@test "status --by-status after up shows only deployed banner" {
    create_pack_file "vim" "home.vimrc" "x"
    dodot up

    run dodot status --by-status
    [ "$status" -eq 0 ]
    assert_output_contains "Deployed Packs"
    assert_output_not_contains "Pending Packs"
    assert_output_not_contains "Error Packs"
}

@test "status --short --by-status combines both" {
    create_pack_file "vim" "home.vimrc" "x"

    run dodot status --short --by-status
    [ "$status" -eq 0 ]
    assert_output_contains "Pending Packs"
    assert_output_contains "(1) pending"
    assert_output_not_contains "vimrc"
}

@test "status rejects --short and --full together" {
    run dodot status --short --full
    [ "$status" -ne 0 ]
}

@test "status rejects --by-name and --by-status together" {
    run dodot status --by-name --by-status
    [ "$status" -ne 0 ]
}
