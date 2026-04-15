#!/usr/bin/env bats
# E2E tests for symlink conflicts and cross-pack routing collisions.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "up reports error when target is an existing unmanaged file" {
    create_pack_file "vim" "vimrc" "set nocompatible"
    create_home_file ".vimrc" "existing content"

    run dodot up
    assert_output_contains "conflict"

    # Make sure we didn't overwrite the user's file
    assert_file_contains "$HOME/.vimrc" "existing content"
}

@test "up allows a pack to overwrite another pack's deep target constraint (last deployed wins)" {
    create_pack_file "pack_a" "settings.toml" "content a"
    create_pack_config "pack_a" '[symlink]\ntargets = { "settings.toml" = "myapp/settings.toml" }'

    create_pack_file "pack_b" "settings.toml" "content b"
    create_pack_config "pack_b" '[symlink]\ntargets = { "settings.toml" = "myapp/settings.toml" }'

    dodot up pack_a
    assert_file_contains "$XDG_CONFIG_HOME/myapp/settings.toml" "content a"

    # pack_b deployed last, should overwrite the symlink to point to pack_b
    dodot up pack_b
    assert_file_contains "$XDG_CONFIG_HOME/myapp/settings.toml" "content b"
}
