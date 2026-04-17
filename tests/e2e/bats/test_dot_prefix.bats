#!/usr/bin/env bats
# E2E tests for the dot. prefix convention.
#
# Files named `dot.something` in a pack become `.something` when deployed.
# This keeps them visible in editors/file browsers while in the repo,
# but they become proper hidden dotfiles at the target location.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

# ── Basic dot. prefix stripping ─────────────────────────────────

@test "dot.bashrc deploys to ~/.bashrc" {
    create_pack_file "shell" "dot.bashrc" "# my bashrc"

    dodot up
    assert_exists "$HOME/.bashrc"
    assert_file_contains "$HOME/.bashrc" "my bashrc"
}

@test "dot.zshrc deploys to ~/.zshrc" {
    create_pack_file "shell" "dot.zshrc" "# my zshrc"

    dodot up
    assert_exists "$HOME/.zshrc"
}

@test "multiple dot. files in same pack" {
    create_pack_file "shell" "dot.bashrc" "# bashrc"
    create_pack_file "shell" "dot.zshrc" "# zshrc"
    create_pack_file "shell" "dot.inputrc" "# inputrc"

    dodot up
    assert_exists "$HOME/.bashrc"
    assert_exists "$HOME/.zshrc"
    assert_exists "$HOME/.inputrc"
}

# ── Status display ──────────────────────────────────────────────

@test "status shows ~/.bashrc not ~/.dot.bashrc" {
    create_pack_file "shell" "dot.bashrc" "# bashrc"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "~/.bashrc"
    assert_output_not_contains "~/.dot.bashrc"
}

@test "status shows correct target for mix of dot. and regular files" {
    create_pack_file "vim" "dot.vimrc" "set nocompatible"
    create_pack_file "vim" "gvimrc" "set guifont=Mono"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "~/.vimrc"
    assert_output_contains "~/.gvimrc"
    assert_output_not_contains "~/.dot.vimrc"
}

# ── Subdirectory files are NOT stripped ──────────────────────────

@test "dot. prefix not stripped for subdirectory files" {
    # Under the top-level-only scanner, the subdir is linked wholesale;
    # nested files are visible through the link. The dot. prefix is a
    # top-level-only convention and must NOT be stripped for files
    # living inside a linked directory.
    create_pack_file "app" "subdir/dot.conf" "config"

    run dodot status
    [ "$status" -eq 0 ]
    # Status shows the wholesale subdir entry, not the nested file.
    assert_output_contains "subdir"
    assert_output_contains "~/.config/subdir"

    # After deploy, the nested file is reachable through the dir symlink,
    # and its name retains the literal `dot.` prefix (no stripping).
    dodot up
    assert_exists "$HOME/.config/subdir/dot.conf"
    assert_not_exists "$HOME/.config/subdir/.conf"
}

# ── Full lifecycle ──────────────────────────────────────────────

@test "dot. prefix lifecycle: up → verify → down → verify" {
    create_pack_file "shell" "dot.bashrc" "# my bashrc"

    # Status before deploy
    run dodot status
    assert_output_contains "~/.bashrc"
    assert_output_contains "pending"

    # Deploy
    dodot up
    assert_exists "$HOME/.bashrc"

    # Status after deploy
    run dodot status
    assert_output_contains "deployed"

    # Down
    dodot down

    # Status after down
    run dodot status
    assert_output_contains "pending"

    # Source file still in pack (unchanged)
    assert_exists "$DOTFILES_ROOT/shell/dot.bashrc"
}

# ── Coexists with regular files ─────────────────────────────────

@test "dot. and regular files deploy correctly together" {
    create_pack_file "mixed" "dot.bashrc" "# bashrc"
    create_pack_file "mixed" "vimrc" "set nocompatible"

    dodot up

    # dot.bashrc → ~/.bashrc
    assert_exists "$HOME/.bashrc"
    assert_file_contains "$HOME/.bashrc" "bashrc"

    # vimrc → ~/.vimrc (regular dot-prefix behavior)
    assert_exists "$HOME/.vimrc"
    assert_file_contains "$HOME/.vimrc" "nocompatible"
}

@test "dot. prefix works with double-link chain" {
    create_pack_file "shell" "dot.bashrc" "# bashrc"

    dodot up

    # Datastore link should use the original filename (dot.bashrc)
    assert_symlink \
        "$XDG_DATA_HOME/dodot/packs/shell/symlink/dot.bashrc" \
        "$DOTFILES_ROOT/shell/dot.bashrc"

    # User link should point to the datastore
    assert_symlink \
        "$HOME/.bashrc" \
        "$XDG_DATA_HOME/dodot/packs/shell/symlink/dot.bashrc"
}
