#!/usr/bin/env bats
# E2E tests for `dodot adopt`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "adopt moves file into pack and creates symlink" {
    create_pack "vim"
    create_home_file ".vimrc" "set nocompatible"

    run dodot adopt vim "$HOME/.vimrc"
    [ "$status" -eq 0 ]
    assert_output_contains "1 file"

    # File should be in the pack (dot prefix stripped)
    assert_exists "$DOTFILES_ROOT/vim/vimrc"
    assert_file_contents "$DOTFILES_ROOT/vim/vimrc" "set nocompatible"

    # Original location should be a symlink
    [ -L "$HOME/.vimrc" ]
}

@test "adopt with --force overwrites existing pack file" {
    create_pack_file "vim" "vimrc" "old content"
    create_home_file ".vimrc" "new content"

    run dodot adopt vim --force "$HOME/.vimrc"
    [ "$status" -eq 0 ]

    assert_file_contents "$DOTFILES_ROOT/vim/vimrc" "new content"
}

@test "adopt reports error without --force when file exists in pack" {
    create_pack_file "vim" "vimrc" "old content"
    create_home_file ".vimrc" "new content"

    run dodot adopt vim "$HOME/.vimrc"
    assert_output_contains "already exists"

    # Original pack file should be unchanged
    assert_file_contents "$DOTFILES_ROOT/vim/vimrc" "old content"
}

@test "adopt multiple files" {
    create_pack "shell"
    create_home_file ".bashrc" "# bashrc"
    create_home_file ".zshrc" "# zshrc"

    run dodot adopt shell "$HOME/.bashrc" "$HOME/.zshrc"
    [ "$status" -eq 0 ]
    assert_output_contains "2 file"

    assert_exists "$DOTFILES_ROOT/shell/bashrc"
    assert_exists "$DOTFILES_ROOT/shell/zshrc"
}
