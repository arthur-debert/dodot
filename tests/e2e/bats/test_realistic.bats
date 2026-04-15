#!/usr/bin/env bats
# E2E tests using the realistic multi-pack dotfiles fixture.
# Exercises all handlers, subdirectory routing, force_home, and
# the full deploy/source/verify cycle.

setup() {
    load helpers/setup
    sandbox_setup
    create_realistic_dotfiles
    install_brew_mock
}

teardown() {
    sandbox_teardown
}

# ── Shell handler ───────────────────────────────────────────────

@test "all shell files are sourced after up" {
    dodot up
    eval_init_sh

    assert_shell_loaded "vim" "aliases.sh"
    assert_shell_loaded "zsh" "aliases.sh"
    assert_shell_loaded "zsh" "profile.sh"
    assert_shell_loaded "zsh" "login.sh"
}

@test "shell files export custom env vars" {
    dodot up
    eval_init_sh

    assert_env_var "ZSH_PROFILE_LOADED" "1"
    assert_env_var "ZSH_LOGIN_LOADED" "1"
}

@test "shell files are not sourced after down" {
    dodot up
    dodot down

    # Fresh eval should not load anything
    eval_init_sh
    assert_shell_not_loaded "vim" "aliases.sh"
    assert_shell_not_loaded "zsh" "aliases.sh"
}

# ── Path handler ────────────────────────────────────────────────

@test "bin scripts are on PATH after up" {
    dodot up
    eval_init_sh

    assert_bin_available "tools" "devtool"
}

@test "bin scripts are not on PATH after down" {
    dodot up
    dodot down
    eval_init_sh

    assert_bin_not_available "tools" "devtool"
}

# ── Install handler ─────────────────────────────────────────────

@test "install.sh executes on up" {
    dodot up
    assert_install_ran "tools"
}

@test "install.sh skipped with --no-provision" {
    dodot up --no-provision
    assert_install_not_ran "tools"
}

# ── Homebrew handler ────────────────────────────────────────────

@test "brew mock receives bundle command on up" {
    dodot up

    assert_brew_invoked
    assert_brew_invoked_with "bundle" "--file"
}

@test "brew skipped with --no-provision" {
    dodot up --no-provision
    assert_brew_not_invoked
}

@test "Brewfile shows in status as not installed / installed" {
    run dodot status tools
    assert_output_contains "not installed"

    dodot up
    run dodot status tools
    assert_output_contains "installed"
}

# ── Symlink handler: top-level files ───────────────────────────

@test "top-level dotfiles land in HOME" {
    dodot up

    assert_exists "$HOME/.vimrc"
    assert_file_contains "$HOME/.vimrc" "set nocompatible"
    assert_exists "$HOME/.gitconfig"
    assert_file_contains "$HOME/.gitconfig" "testuser"
}

# ── Symlink handler: subdirectory → XDG ────────────────────────

@test "subdirectory files route to XDG_CONFIG_HOME" {
    dodot up

    assert_exists "$XDG_CONFIG_HOME/nvim/init.lua"
    assert_exists "$XDG_CONFIG_HOME/nvim/lua/plugins.lua"
}

# ── Symlink handler: force_home ─────────────────────────────────

@test "force_home routes ssh/ to HOME instead of XDG" {
    dodot up

    # ssh is in the default force_home list, so ssh/config → ~/.ssh/config
    assert_exists "$HOME/.ssh/config"
    assert_file_contains "$HOME/.ssh/config" "ServerAliveInterval"
}

# ── Ignored packs ──────────────────────────────────────────────

@test "ignored packs excluded from status and up" {
    run dodot status
    [ "$status" -eq 0 ]
    assert_output_not_contains "disabled"

    dodot up
    assert_not_exists "$HOME/.notes.txt"
}

# ── Full lifecycle with realistic fixture ──────────────────────

@test "full lifecycle: up → verify → down → verify" {
    # Deploy
    dodot up
    eval_init_sh

    # All handlers active
    assert_shell_loaded "vim" "aliases.sh"
    assert_bin_available "tools" "devtool"
    assert_install_ran "tools"
    assert_brew_invoked
    assert_exists "$HOME/.vimrc"
    assert_exists "$XDG_CONFIG_HOME/nvim/init.lua"
    assert_exists "$HOME/.ssh/config"

    # Tear down
    dodot down

    # Status should show everything pending/not sourced
    run dodot status
    assert_output_contains "pending"
    assert_output_contains "not sourced"
    assert_output_contains "not in PATH"
}
