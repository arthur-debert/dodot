#!/usr/bin/env bats
# Phase S2 secrets E2E — `bw` (Bitwarden CLI) provider.
#
# Mirrors `test_secrets.bats` (Phase S1 / pass) for the bw scheme.
# The stub binary lives in `helpers/secrets_stubs.bash` and only
# implements the surface `crates/dodot-lib/src/secret/bw.rs`
# touches: `bw --version`, `bw status`, `bw get <field> <item>`.

setup() {
    load helpers/setup
    sandbox_setup
    load helpers/secrets_stubs
    secrets_bw_stub_setup
}

teardown() {
    sandbox_teardown
}

@test "secret(bw:item) resolves the password field by default" {
    seed_bw_secret "gh-token" "password" "ghp_default_field"

    secrets_enable_bw_in_root_config
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'token = "{{ secret("bw:gh-token") }}"'

    run dodot up
    [ "$status" -eq 0 ]
    [ -L "$HOME/.config/app/config.toml" ]
    assert_file_contains "$HOME/.config/app/config.toml" 'token = "ghp_default_field"'
}

@test "secret(bw:item#field) routes to the explicit field" {
    seed_bw_secret "gh-token" "username" "debert+dodot"

    secrets_enable_bw_in_root_config
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'user = "{{ secret("bw:gh-token#username") }}"'

    run dodot up
    [ "$status" -eq 0 ]
    assert_file_contains "$HOME/.config/app/config.toml" 'user = "debert+dodot"'
}

@test "preflight blocks dodot up when the vault is locked" {
    set_bw_stub_status "locked"
    secrets_enable_bw_in_root_config
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'value = "{{ secret("bw:any") }}"'

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "secret provider"
    assert_output_contains "bw"
    assert_output_contains "locked"
    [ ! -e "$HOME/.config/app/cfg.toml" ]
}

@test "preflight blocks dodot up when bw is unauthenticated" {
    set_bw_stub_status "unauthenticated"
    secrets_enable_bw_in_root_config
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'value = "{{ secret("bw:any") }}"'

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "not logged in"
}

@test "missing bw item produces a clear render error" {
    secrets_enable_bw_in_root_config
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'value = "{{ secret("bw:does-not-exist") }}"'

    run dodot up
    assert_output_contains "does-not-exist"
    [ ! -e "$HOME/.config/app/cfg.toml" ]
}
