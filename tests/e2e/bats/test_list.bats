#!/usr/bin/env bats
# E2E tests for `dodot list`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "list shows all packs" {
    create_pack "git"
    create_pack_file "git" "home.gitconfig" "[user]\n  name = test"
    create_pack "vim"
    create_pack_file "vim" "home.vimrc" "set nocompatible"

    run dodot list
    [ "$status" -eq 0 ]
    assert_output_contains "git"
    assert_output_contains "vim"
}

@test "list shows ignored packs with marker" {
    create_pack "active"
    create_pack_file "active" "file" "content"
    create_pack "disabled"
    create_pack_file "disabled" "file" "content"
    mark_ignored "disabled"

    run dodot list
    [ "$status" -eq 0 ]
    assert_output_contains "active"
    assert_output_contains "disabled"
    assert_output_contains "(ignored)"
}

@test "list with no packs shows nothing" {
    run dodot list
    [ "$status" -eq 0 ]
    # Output should be empty or just whitespace
    [[ -z "$(echo "$output" | tr -d '[:space:]')" ]]
}

@test "list ignores dotfiles and hidden directories" {
    create_pack "vim"
    create_pack_file "vim" "home.vimrc" "x"
    # .dodot.toml and .git should not appear as packs
    create_root_config '[pack]\nignore = []'

    run dodot list
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    assert_output_not_contains ".dodot.toml"
    assert_output_not_contains ".git"
}
