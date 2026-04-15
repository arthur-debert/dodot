#!/usr/bin/env bats
# E2E tests for `dodot fill`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "fill adds template files to empty pack" {
    create_pack "mypack"

    run dodot fill mypack
    [ "$status" -eq 0 ]

    # Should have created placeholder files
    assert_exists "$DOTFILES_ROOT/mypack"
    # fill creates .dodot.toml and placeholder files
    local file_count
    file_count=$(find "$DOTFILES_ROOT/mypack" -type f | wc -l | tr -d ' ')
    [ "$file_count" -gt 0 ]
}

@test "fill skips existing files" {
    create_pack_file "mypack" "vimrc" "my custom content"

    run dodot fill mypack
    [ "$status" -eq 0 ]

    # Existing file should be untouched
    assert_file_contents "$DOTFILES_ROOT/mypack/vimrc" "my custom content"
}

@test "fill on nonexistent pack reports error" {
    run dodot fill nonexistent
    assert_output_contains "Error"
}
