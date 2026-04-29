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

    # HOME-direct dotfile requires --into (pack name has no path-derivable
    # source under the new inference rules).
    run dodot adopt --into vim "$HOME/.vimrc"
    [ "$status" -eq 0 ]
    # Adopt output is now the destination pack's status (matches `dodot status vim`).
    assert_output_contains "vim"
    assert_output_contains "vimrc"
    assert_output_contains "pending"

    # File should be in the pack (dot prefix stripped, home. prefix added).
    assert_exists "$DOTFILES_ROOT/vim/home.vimrc"
    assert_file_contents "$DOTFILES_ROOT/vim/home.vimrc" "set nocompatible"

    # Original location should be a symlink
    [ -L "$HOME/.vimrc" ]
}

@test "adopt with --force overwrites existing pack file" {
    create_pack_file "vim" "home.vimrc" "old content"
    create_home_file ".vimrc" "new content"

    run dodot adopt --into vim --force "$HOME/.vimrc"
    [ "$status" -eq 0 ]

    assert_file_contents "$DOTFILES_ROOT/vim/home.vimrc" "new content"
}

@test "adopt reports error without --force when file exists in pack" {
    create_pack_file "vim" "home.vimrc" "old content"
    create_home_file ".vimrc" "new content"

    run dodot adopt --into vim "$HOME/.vimrc"
    assert_output_contains "already exists"

    # Original pack file should be unchanged
    assert_file_contents "$DOTFILES_ROOT/vim/home.vimrc" "old content"
}

@test "adopt multiple files" {
    create_pack "shell"
    create_home_file ".bashrc" "# bashrc"
    create_home_file ".zshrc" "# zshrc"

    run dodot adopt --into shell "$HOME/.bashrc" "$HOME/.zshrc"
    [ "$status" -eq 0 ]
    # Status output lists both adopted files under the destination pack.
    assert_output_contains "shell"
    assert_output_contains "bashrc"
    assert_output_contains "zshrc"

    # `bashrc` and `zshrc` are in the default force_home list, so they
    # adopt with the bare in-pack name (Priority 3 routes them back to
    # ~/.X without the `home.` prefix).
    assert_exists "$DOTFILES_ROOT/shell/bashrc"
    assert_exists "$DOTFILES_ROOT/shell/zshrc"
}

@test "adopt reports error when target file does not exist" {
    create_pack "vim"

    run dodot adopt --into vim "$HOME/.nonexistent"
    assert_output_contains "source does not exist"
}

@test "adopt infers pack from XDG path and auto-creates" {
    # No pre-existing pack, no --into — pack name comes from the path
    # (`~/.config/<X>/...` → pack `<X>`), and the pack is auto-created.
    create_home_file ".config/ghostty/config" "theme = dark"

    run dodot adopt "$HOME/.config/ghostty/config"
    [ "$status" -eq 0 ]
    assert_output_contains "ghostty"

    # Pack auto-created at <dotfiles>/ghostty/, file at pack root (the
    # default rule routes pack `ghostty`/`config` back to ~/.config/ghostty/config).
    assert_exists "$DOTFILES_ROOT/ghostty/config"
    assert_file_contents "$DOTFILES_ROOT/ghostty/config" "theme = dark"
    [ -L "$HOME/.config/ghostty/config" ]
}

@test "adopt of XDG pack-root directory expands to children" {
    # Adopting the directory itself enumerates its children and adopts
    # each as its own top-level pack entry (rather than symlinking the
    # whole directory).
    create_home_file ".config/helix/config.toml" "theme = \"onedark\""
    create_home_file ".config/helix/themes/extra.toml" "fg = \"white\""

    run dodot adopt "$HOME/.config/helix"
    [ "$status" -eq 0 ]

    assert_exists "$DOTFILES_ROOT/helix/config.toml"
    assert_exists "$DOTFILES_ROOT/helix/themes/extra.toml"
    [ -L "$HOME/.config/helix/config.toml" ]
    [ -L "$HOME/.config/helix/themes" ]
    # The pack-root directory itself stays a real directory.
    [ ! -L "$HOME/.config/helix" ]
}

@test "adopt without --into for HOME source errors with hint" {
    create_home_file ".vimrc" "set nocompatible"

    run dodot adopt "$HOME/.vimrc"
    [ "$status" -ne 0 ]
    # The error message points the user at --into since HOME has no
    # inferable pack structure.
    assert_output_contains "--into"
}
