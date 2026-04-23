#!/usr/bin/env bats
# E2E tests for config parsing errors.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "up reports error when pack config is malformed" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"
    # Create invalid toml
    create_pack_config "vim" '[symlink]\ntargets = "not an array"'

    run dodot up
    assert_output_contains "config error"
}
