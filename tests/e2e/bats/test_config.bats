#!/usr/bin/env bats
# E2E tests for `dodot config` subcommands.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "config list displays merged config" {
    create_root_config '[pack]\nignore = ["scratch"]'

    run dodot config list
    [ "$status" -eq 0 ]
    assert_output_contains "pack"
    assert_output_contains "symlink"
}

@test "config get retrieves specific values" {
    create_root_config '[mappings]\ninstall = "setup.sh"'

    run dodot config get mappings.install
    [ "$status" -eq 0 ]
    assert_output_contains "setup.sh"
}

@test "config list with pack override shows merged result" {
    create_root_config '[mappings]\ninstall = "install.sh"'
    create_pack "vim"
    create_pack_config "vim" '[mappings]\ninstall = "vim-setup.sh"'

    # Root config should still show install.sh
    run dodot config list
    [ "$status" -eq 0 ]
    assert_output_contains "install"
}
