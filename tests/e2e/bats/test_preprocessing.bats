#!/usr/bin/env bats
# E2E tests for the preprocessing pipeline (template + unarchive).
#
# Intentionally narrow: the Rust suite already covers the matrix of
# preprocessor behaviours. These tests verify the *integration* — that
# `dodot up` wires preprocessing into the real binary, renders
# `.tmpl` files end-to-end, and exposes clean errors to the CLI.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "template file is rendered and deployed via symlink" {
    create_pack "app"
    create_pack_file "app" "vimrc.tmpl" 'set user={{ name }}
set os={{ dodot.os }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # The user-visible file has the `.tmpl` extension stripped and gets
    # the dotfile prefix from the symlink handler: vimrc → ~/.vimrc.
    [ -L "$HOME/.vimrc" ]

    # Rendered content: the variable is substituted and dodot.os is a
    # non-empty string (whatever the host OS is).
    assert_file_contains "$HOME/.vimrc" "set user=Alice"
    assert_file_contains "$HOME/.vimrc" "set os="

    # The rendered file lives in the datastore under packs/<pack>/preprocessed/.
    assert_exists "$XDG_DATA_HOME/dodot/packs/app/preprocessed/vimrc"
}

@test "template can be disabled globally via root config" {
    create_root_config '[preprocessor]
enabled = false'
    create_pack "app"
    create_pack_file "app" "vimrc.tmpl" 'set user={{ name }}'

    run dodot up
    [ "$status" -eq 0 ]

    # With preprocessing disabled the `.tmpl` file is deployed verbatim,
    # extension preserved, no rendering.
    [ -L "$HOME/.vimrc.tmpl" ]
    assert_file_contains "$HOME/.vimrc.tmpl" '{{ name }}'
}

@test "undefined template variable surfaces with source path" {
    create_pack "app"
    create_pack_file "app" "bad.tmpl" 'value = "{{ nope }}"'

    # `dodot up` reports per-pack errors in its output but still exits 0
    # when at least the run itself completed. The important thing is the
    # error naming the template so the user can find it, plus no stray
    # file getting deployed.
    run dodot up
    assert_output_contains "bad.tmpl"
    assert_output_contains "template render failed"
    [ ! -e "$HOME/.bad" ]
}

@test "pack config overrides root config for template vars" {
    create_root_config '[preprocessor.template.vars]
name = "Root"'
    create_pack "app"
    create_pack_file "app" "greeting.tmpl" 'hello {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Pack"'

    run dodot up
    [ "$status" -eq 0 ]
    # File is top-level, so it deploys as ~/.greeting (dotfile prefix).
    assert_file_contains "$HOME/.greeting" "hello Pack"
}

@test "template collision with regular file is rejected" {
    create_pack "app"
    create_pack_file "app" "config.toml" "regular"
    create_pack_file "app" "config.toml.tmpl" 'templated {{ 1 + 1 }}'

    run dodot up
    assert_output_contains "preprocessing collision"
    assert_output_contains "config.toml"
    # Neither file should have been deployed — the pack short-circuits
    # on the collision.
    [ ! -e "$HOME/.config.toml" ]
}

@test "template with env var and default filter renders fallback" {
    create_pack "app"
    create_pack_file "app" "settings.tmpl" 'editor={{ env.DODOT_MISSING_VAR | default("nano") }}'

    # Make sure the probe variable is truly unset for this process.
    unset DODOT_MISSING_VAR

    run dodot up
    [ "$status" -eq 0 ]
    assert_file_contains "$HOME/.settings" "editor=nano"
}
