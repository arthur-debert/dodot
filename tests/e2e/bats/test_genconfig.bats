#!/usr/bin/env bats
# E2E tests for `dodot genconfig`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "genconfig returns TOML template" {
    run dodot genconfig
    [ "$status" -eq 0 ]
    assert_output_contains "[pack]"
    assert_output_contains "[symlink]"
    assert_output_contains "[mappings]"
}

@test "genconfig --write creates file at dotfiles root" {
    run dodot genconfig --write
    [ "$status" -eq 0 ]

    assert_exists "$DOTFILES_ROOT/.dodot.toml"
    assert_file_contains "$DOTFILES_ROOT/.dodot.toml" "[pack]"
}

@test "genconfig --write does not overwrite existing config" {
    create_root_config "[pack]\nignore = [\"scratch\"]"

    run dodot genconfig --write
    # Should either fail or report that config exists
    # The file should still have original content
    assert_file_contains "$DOTFILES_ROOT/.dodot.toml" "scratch"
}
