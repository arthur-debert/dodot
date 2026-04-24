#!/usr/bin/env bats
# E2E tests for config-driven behavior:
# - [pack] ignore patterns
# - [symlink] target_overrides (absolute paths)
# - [symlink] force_home
# - [symlink] protected_paths
# - per-pack .dodot.toml overrides

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

# ── Pack ignore patterns ────────────────────────────────────────

@test "pack ignore excludes matching files from processing" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"
    create_pack_file "vim" "notes.bak" "scratch"
    create_pack_config "vim" '[pack]\nignore = ["*.bak"]'

    dodot up
    assert_exists "$HOME/.vimrc"

    # .bak file should not be symlinked
    run dodot status vim
    assert_output_not_contains "notes.bak"
}

# ── Target overrides ───────────────────────────────────────────

@test "target_overrides with absolute path places symlink at exact location" {
    create_pack_file "custom" "myconfig" "custom content"
    create_pack_config "custom" "[symlink]\ntargets = { \"myconfig\" = \"$HOME/custom-location/myconfig\" }"

    dodot up

    assert_exists "$HOME/custom-location/myconfig"
    assert_file_contains "$HOME/custom-location/myconfig" "custom content"
}

@test "target_overrides with relative path resolves from XDG_CONFIG_HOME" {
    create_pack_file "app" "settings.toml" "key = value"
    create_pack_config "app" '[symlink]\ntargets = { "settings.toml" = "myapp/settings.toml" }'

    dodot up

    assert_exists "$XDG_CONFIG_HOME/myapp/settings.toml"
    assert_file_contains "$XDG_CONFIG_HOME/myapp/settings.toml" "key = value"
}

# ── Force home ─────────────────────────────────────────────────

@test "force_home routes subdirectory to HOME instead of XDG" {
    # Default force_home includes "ssh", so ssh/config → ~/.ssh/config
    create_pack_file "mypack" "ssh/config" "Host *"

    # Add "ssh" to force_home via root config (it's in defaults, but explicit is clearer)
    create_root_config '[symlink]\nforce_home = ["ssh"]'

    dodot up
    assert_exists "$HOME/.ssh/config"
    assert_file_contains "$HOME/.ssh/config" "Host"
}

@test "custom force_home entry routes to HOME" {
    create_pack_file "mypack" "mytool/config.yml" "setting: true"
    create_root_config '[symlink]\nforce_home = ["mytool"]'

    dodot up
    assert_exists "$HOME/.mytool/config.yml"
}

# ── Protected paths ────────────────────────────────────────────

@test "protected paths are refused" {
    # .ssh/id_rsa is in the default protected list
    create_pack_file "keys" "ssh/id_rsa" "secret key"

    run dodot up
    # Should report error or skip the protected file
    run dodot status keys
    # The protected file should NOT be deployed
    assert_not_exists "$HOME/.ssh/id_rsa"
}

# ── Per-pack config overrides ──────────────────────────────────

@test "per-pack config overrides root config" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"
    create_pack_file "vim" "cache.tmp" "temporary"

    # Root config has no ignores
    create_root_config '[pack]\nignore = []'

    # Pack config adds ignore
    create_pack_config "vim" '[pack]\nignore = ["*.tmp"]'

    dodot up

    assert_exists "$HOME/.vimrc"
    # .tmp should be excluded by pack-level ignore
    run dodot status vim
    assert_output_not_contains "cache.tmp"
}

@test "per-pack mapping override changes handler assignment" {
    # Root uses default install.sh
    create_root_config '[mappings]\ninstall = ["install.sh"]'

    # This pack uses setup.sh instead
    create_pack_script "custom" "setup.sh" '#!/bin/sh
mkdir -p "$HOME/.dodot-markers"
echo "executed" > "$HOME/.dodot-markers/custom.install"'
    create_pack_config "custom" '[mappings]\ninstall = ["setup.sh"]'

    dodot up
    assert_install_ran "custom"
}
