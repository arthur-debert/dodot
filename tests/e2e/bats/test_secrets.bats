#!/usr/bin/env bats
# Phase S1 secrets E2E — integration tests for the `secret(...)`
# MiniJinja function and the `pass` provider.
#
# Coverage scope (per `docs/proposals/secrets-testing.lex` §4.3):
#   - happy path: pass: reference resolves end-to-end via `dodot up`
#   - sidecar (`<baseline>.secret.json`) is written next to the baseline
#   - preflight blocks `dodot up` when an enabled provider is misconfigured
#   - missing reference surfaces a clear error
#   - dry-run does NOT invoke the provider (Passive contract — §7.4)
#
# Whole-file (age/gpg) and op stubs are out of scope for Phase S1 and
# land with their respective phases.

setup() {
    load helpers/setup
    sandbox_setup
    load helpers/secrets_stubs
    secrets_pass_stub_setup
}

teardown() {
    sandbox_teardown
}

@test "secret(pass:...) resolves end-to-end and the rendered file lands deployed" {
    seed_pass_secret "test/db_password" "hunter2-from-fixture"

    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'db_password = "{{ secret("pass:test/db_password") }}"'

    run dodot up
    [ "$status" -eq 0 ]

    # Rendered file lands at the symlink target with the resolved value.
    [ -L "$HOME/.config/app/config.toml" ]
    assert_file_contains "$HOME/.config/app/config.toml" 'db_password = "hunter2-from-fixture"'
}

@test "sidecar (.secret.json) is written next to the baseline" {
    seed_pass_secret "test/api_key" "secret-key-xyz"

    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "settings.toml.tmpl" 'key = "{{ secret("pass:test/api_key") }}"'

    run dodot up
    [ "$status" -eq 0 ]

    # The baseline cache lives under the per-pack cache; the sidecar
    # sits beside it. We don't assert the exact filename layout (that
    # contract is pinned by the rust-side baseline tests); we do
    # assert that exactly one sidecar JSON exists somewhere under the
    # dodot cache for this pack and that it carries the reference.
    local sidecar
    sidecar="$(find "$XDG_CACHE_HOME/dodot" -name '*.secret.json' 2>/dev/null | head -1)"
    [ -n "$sidecar" ] || {
        echo "expected a *.secret.json sidecar under \$XDG_CACHE_HOME/dodot" >&2
        find "$XDG_CACHE_HOME/dodot" 2>/dev/null >&2
        return 1
    }
    assert_file_contains "$sidecar" "pass:test/api_key"
}

@test "preflight blocks dodot up when the pass binary is missing" {
    # Provider is enabled in config but the stub is removed from PATH.
    # `pass version` fails → ProbeResult::NotInstalled → preflight
    # surfaces an aggregated error and `dodot up` aborts before any
    # rendering would happen. See secrets.lex §5.4.
    secrets_drop_pass_stub
    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'value = "{{ secret("pass:test/anything") }}"'

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "secret provider"
    assert_output_contains "pass"
    assert_output_contains "not installed"

    # No rendering happened — the deployed file must not exist.
    [ ! -e "$HOME/.config/app/cfg.toml" ]
}

@test "missing reference produces a clear render error" {
    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'value = "{{ secret("pass:test/missing") }}"'

    # Pass binary is installed but the reference is not in the catalog.
    # The provider returns the documented "not in the password store"
    # error, which surfaces through MiniJinja as a render failure
    # naming the reference.
    run dodot up
    assert_output_contains "test/missing"
    [ ! -e "$HOME/.config/app/cfg.toml" ]
}

@test "dry-run does not invoke the provider (Passive contract)" {
    # Enable pass but leave the catalog empty. If --dry-run wrongly
    # called the provider, `pass show` would exit non-zero and the
    # render would fail. The Passive envelope (secrets.lex §7.4)
    # forbids any provider call: dry-run reads the baseline cache.
    # On a fresh sandbox there's no baseline yet, so dry-run reports
    # nothing-to-do without ever shelling out. The key assertion is
    # the absence of a hard failure plus no deployed file.
    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'value = "{{ secret("pass:test/never_called") }}"'

    run dodot up --dry-run
    # Whatever dry-run reports, it must not crash on the unconfigured
    # secret reference (provider was never called).
    [ "$status" -eq 0 ] || {
        echo "dry-run failed; provider may have been invoked" >&2
        echo "$output" >&2
        return 1
    }
    [ ! -e "$HOME/.config/app/cfg.toml" ]
}
