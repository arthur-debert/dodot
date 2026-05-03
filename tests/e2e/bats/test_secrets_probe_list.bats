#!/usr/bin/env bats
# Phase S5 secrets E2E — `dodot secret probe` and `dodot secret list`.
#
# Both commands are read-only and deliberately don't touch any
# real OS keystore. They exercise the same `pass` stub seam used
# by `test_secrets.bats` so coverage runs everywhere `pass` does
# (which is "everywhere" — the stub is per-sandbox).

setup() {
    load helpers/setup
    sandbox_setup
    load helpers/secrets_stubs
}

teardown() {
    sandbox_teardown
}

# ── secret probe ────────────────────────────────────────────────

@test "secret probe reports disabled when no providers are configured" {
    # Default config (no [secret] block) → disabled branch.
    run dodot secret probe
    [ "$status" -eq 0 ]
    assert_output_contains "No secret providers configured"
    assert_output_contains "[secret.providers.pass]"
}

@test "secret probe reports disabled when master switch is off" {
    cat > "$DOTFILES_ROOT/.dodot.toml" <<'TOML'
[secret]
enabled = false

[secret.providers.pass]
enabled = true
TOML
    run dodot secret probe
    [ "$status" -eq 0 ]
    assert_output_contains "No secret providers configured"
}

@test "secret probe reports ok for a healthy pass provider" {
    secrets_pass_stub_setup
    secrets_enable_pass_in_root_config
    run dodot secret probe
    [ "$status" -eq 0 ]
    assert_output_contains "1 ok"
    assert_output_contains "pass"
    assert_output_contains "ok"
}

@test "secret probe surfaces NotInstalled when the binary is missing" {
    secrets_pass_stub_setup
    secrets_enable_pass_in_root_config
    secrets_drop_pass_stub
    run dodot secret probe
    [ "$status" -eq 0 ]
    assert_output_contains "1 need attention"
    assert_output_contains "not installed"
}

# ── secret list ─────────────────────────────────────────────────

@test "secret list reports the empty case for a repo with no templates" {
    run dodot secret list
    [ "$status" -eq 0 ]
    assert_output_contains "No"
    assert_output_contains "secret"
}

@test "secret list enumerates references and groups by scheme" {
    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'db = "{{ secret("pass:test/db_password") }}"
api = "{{ secret("op://Personal/api/token") }}"'
    create_pack "infra"
    create_pack_file "infra" "secrets.tmpl" 'gh = "{{ secret("bw:gh-token") }}"'

    run dodot secret list
    [ "$status" -eq 0 ]
    # Three references found across two schemes-without-provider
    # plus one with the provider enabled (pass).
    assert_output_contains "3 secret references"
    assert_output_contains "pass:test/db_password"
    assert_output_contains "op://Personal/api/token"
    assert_output_contains "bw:gh-token"
    # `op` and `bw` are referenced but not enabled — the rollup
    # at the bottom names them.
    assert_output_contains "Schemes referenced but not enabled"
    assert_output_contains "op"
    assert_output_contains "bw"
}

@test "secret list reports correct line numbers for multi-line templates" {
    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'header
port = 5432
db = "{{ secret("pass:db_password") }}"
footer'

    run dodot secret list
    [ "$status" -eq 0 ]
    # The reference is on line 3 of the template.
    assert_output_contains "config.toml.tmpl:3"
}
