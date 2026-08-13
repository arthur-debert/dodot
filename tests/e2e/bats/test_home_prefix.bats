#!/usr/bin/env bats
# E2E tests for the home. prefix convention.
#
# Files named `home.something` in a pack become `.something` when deployed.
# This keeps them visible in editors/file browsers while in the repo,
# but they become proper hidden dotfiles at the target location.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

# ── Basic home. prefix stripping ─────────────────────────────────

@test "home.bashrc deploys to ~/.bashrc" {
    create_pack_file "shell" "home.bashrc" "# my bashrc"

    dodot up
    assert_exists "$HOME/.bashrc"
    assert_file_contains "$HOME/.bashrc" "my bashrc"
}

@test "home.zshrc deploys to ~/.zshrc" {
    create_pack_file "shell" "home.zshrc" "# my zshrc"

    dodot up
    assert_exists "$HOME/.zshrc"
}

@test "multiple home. files in same pack" {
    create_pack_file "shell" "home.bashrc" "# bashrc"
    create_pack_file "shell" "home.zshrc" "# zshrc"
    create_pack_file "shell" "home.inputrc" "# inputrc"

    dodot up
    assert_exists "$HOME/.bashrc"
    assert_exists "$HOME/.zshrc"
    assert_exists "$HOME/.inputrc"
}

# ── Status display ──────────────────────────────────────────────

@test "status JSON routes home.bashrc to ~/.bashrc" {
    create_pack_file "shell" "home.bashrc" "# bashrc"

    run dodot status --output json
    [ "$status" -eq 0 ]
    jq -e '
        .packs[]
        | select(.name == "shell")
        | .files[]
        | select(.name == "home.bashrc")
        | .description == "~/.bashrc"
    ' <<<"$output" >/dev/null
}

@test "status JSON reports targets for multiple home. files" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"
    create_pack_file "vim" "home.gvimrc" "set guifont=Mono"

    run dodot status --output json
    [ "$status" -eq 0 ]
    jq -e '
        .packs[]
        | select(.name == "vim")
        | any(.files[]; .name == "home.vimrc" and .description == "~/.vimrc")
          and any(.files[]; .name == "home.gvimrc" and .description == "~/.gvimrc")
    ' <<<"$output" >/dev/null
}

# ── Subdirectory files are NOT stripped ──────────────────────────

@test "home. prefix not stripped for subdirectory files" {
    # Under the top-level-only scanner, the subdir is linked wholesale;
    # nested files are visible through the link. The home. prefix is a
    # top-level-only convention and must NOT be stripped for files
    # living inside a linked directory.
    #
    # Post-#48: top-level dirs deploy under the pack's XDG namespace
    # (~/.config/<pack>/<dir>), so the subdir path is
    # ~/.config/app/subdir/, not ~/.config/subdir/.
    create_pack_file "app" "subdir/home.conf" "config"

    run dodot status --output json
    [ "$status" -eq 0 ]
    jq -e '
        .packs[]
        | select(.name == "app")
        | .files[]
        | select(.name == "subdir")
        | .description == "~/.config/app/subdir"
    ' <<<"$output" >/dev/null

    # After deploy, the nested file is reachable through the dir symlink,
    # and its name retains the literal `home.` prefix (no stripping).
    dodot up
    assert_exists "$HOME/.config/app/subdir/home.conf"
    assert_not_exists "$HOME/.config/app/subdir/.conf"
}

# ── Full lifecycle ──────────────────────────────────────────────

@test "home. prefix lifecycle: up → verify → down → verify" {
    create_pack_file "shell" "home.bashrc" "# my bashrc"

    # Status before deploy
    run dodot status --output json
    [ "$status" -eq 0 ]
    jq -e '
        .packs[]
        | select(.name == "shell")
        | .files[]
        | select(.name == "home.bashrc")
        | .description == "~/.bashrc" and .status_label == "pending"
    ' <<<"$output" >/dev/null

    # Deploy
    dodot up
    assert_exists "$HOME/.bashrc"

    # Status after deploy
    run dodot status --output json
    [ "$status" -eq 0 ]
    jq -e '
        .packs[]
        | select(.name == "shell")
        | .files[]
        | select(.name == "home.bashrc")
        | .status_label == "linked"
    ' <<<"$output" >/dev/null

    # Down
    dodot down

    # Status after down
    run dodot status --output json
    [ "$status" -eq 0 ]
    jq -e '
        .packs[]
        | select(.name == "shell")
        | .files[]
        | select(.name == "home.bashrc")
        | .status_label == "pending"
    ' <<<"$output" >/dev/null

    # Source file still in pack (unchanged)
    assert_exists "$DOTFILES_ROOT/shell/home.bashrc"
}

# ── Coexists with regular files ─────────────────────────────────

@test "home. and regular files deploy correctly together" {
    create_pack_file "mixed" "home.bashrc" "# bashrc"
    create_pack_file "mixed" "vimrc" "set nocompatible"

    dodot up

    # home.bashrc → ~/.bashrc (per-file home opt-in)
    assert_exists "$HOME/.bashrc"
    assert_file_contains "$HOME/.bashrc" "bashrc"

    # vimrc (no home. prefix) → ~/.config/mixed/vimrc (post-#48 default)
    assert_exists "$HOME/.config/mixed/vimrc"
    assert_file_contains "$HOME/.config/mixed/vimrc" "nocompatible"
}

@test "home. prefix works with double-link chain" {
    create_pack_file "shell" "home.bashrc" "# bashrc"

    dodot up

    # Datastore link should use the original filename (home.bashrc)
    assert_symlink \
        "$XDG_DATA_HOME/dodot/packs/shell/symlink/home.bashrc" \
        "$DOTFILES_ROOT/shell/home.bashrc"

    # User link should point to the datastore
    assert_symlink \
        "$HOME/.bashrc" \
        "$XDG_DATA_HOME/dodot/packs/shell/symlink/home.bashrc"
}
