#!/usr/bin/env bats
# E2E tests for `dodot up`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "up deploys symlink files" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"

    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "Packs deployed"

    # Double-link chain: source -> datastore -> ~/.vimrc
    assert_double_link "vim" "symlink" "home.vimrc" \
        "$DOTFILES_ROOT/vim/home.vimrc" \
        "$HOME/.vimrc"

    # Content should be readable through the symlink chain
    assert_file_contains "$HOME/.vimrc" "set nocompatible"
}

@test "up deploys multiple files in a pack" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"
    create_pack_file "vim" "home.gvimrc" "set guifont=Mono"

    dodot up
    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.vimrc"
    assert_symlink "$HOME/.gvimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.gvimrc"
}

@test "up deploys shell files" {
    create_pack_file "zsh" "aliases.sh" "alias ll='ls -la'"

    dodot up

    # Shell file should be staged in datastore
    assert_exists "$XDG_DATA_HOME/dodot/packs/zsh/shell/aliases.sh"

    # Shell init script should reference it
    assert_file_contains "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" "aliases.sh"
}

@test "up deploys path directories" {
    create_pack "tools"
    create_pack_bin "tools" "mytool" '#!/bin/sh\necho hello'

    dodot up

    # Path dir should be staged in datastore
    assert_exists "$XDG_DATA_HOME/dodot/packs/tools/path/bin"

    # Init script should add bin to PATH
    assert_file_contains "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" "PATH="
}

@test "up --dry-run shows plan without changes" {
    create_pack_file "vim" "home.vimrc" "x"

    run dodot up --dry-run
    [ "$status" -eq 0 ]
    assert_output_contains "dry run"

    # Nothing should actually be deployed
    assert_not_exists "$HOME/.vimrc"
    assert_no_handler_state "vim" "symlink"
}

@test "up --no-provision skips install scripts" {
    create_pack_file "tools" "vimrc" "x"
    create_pack_script "tools" "install.sh" '#!/bin/sh\ntouch "$HOME/.tools-installed"'

    dodot up --no-provision

    # Install script should NOT have run
    assert_not_exists "$HOME/.tools-installed"
}

@test "up deploys selected packs only" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "git" "home.gitconfig" "x"

    dodot up vim

    # vim should be deployed
    assert_exists "$HOME/.vimrc"
    # git should NOT be deployed
    assert_not_exists "$HOME/.gitconfig"
}

@test "up is idempotent" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"

    dodot up
    assert_exists "$HOME/.vimrc"

    # Second up should succeed without error
    run dodot up
    [ "$status" -eq 0 ]

    # Symlink should still work
    assert_file_contains "$HOME/.vimrc" "set nocompatible"
}

@test "up deploys multiple packs" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "git" "home.gitconfig" "x"

    dodot up
    assert_exists "$HOME/.vimrc"
    assert_exists "$HOME/.gitconfig"
}

@test "up skips ignored packs" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "disabled" "file" "x"
    mark_ignored "disabled"

    dodot up

    assert_exists "$HOME/.vimrc"
    assert_not_exists "$HOME/.file"
}
