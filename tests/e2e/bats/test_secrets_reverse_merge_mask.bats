#!/usr/bin/env bats
# Phase S2 secrets E2E — sidecar mask integration with reverse-merge.
#
# Pins the `secrets.lex` §3.3 contract end-to-end: when a secret
# value is rotated in the deployed file, `dodot transform check`
# does NOT propagate that rotation back into the template source as
# a literal value (which would defeat the `secret(...)` abstraction).
# The sidecar (`<baseline>.secret.json`) lists the secret line
# range, and burgertocow's mask treats that line as already-matching
# regardless of bytes.

setup() {
    load helpers/setup
    sandbox_setup
    load helpers/secrets_stubs
    secrets_pass_stub_setup
}

teardown() {
    sandbox_teardown
}

@test "transform check does not propagate rotated secret values back to the template" {
    # Step 1: render once with the original secret.
    seed_pass_secret "test/db_password" "ORIGINAL"
    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'pw = "{{ secret("pass:test/db_password") }}"'

    run dodot up
    [ "$status" -eq 0 ]
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    assert_file_contains "$rendered" 'pw = "ORIGINAL"'

    # Sanity: sidecar exists.
    sidecar="$(find "$XDG_CACHE_HOME/dodot" -name '*.secret.json' 2>/dev/null | head -1)"
    [ -n "$sidecar" ] || {
        echo "expected secrets sidecar to be written" >&2
        return 1
    }

    # Step 2: simulate a vault rotation on the deployed file.
    # (`dodot up` would render this by re-resolving the secret;
    # here we substitute by hand so the test isolates the mask
    # behavior, not the resolution path.)
    echo 'pw = "ROTATED"' > "$rendered"

    # Step 3: `dodot transform check` must NOT rewrite the source
    # template to the literal "ROTATED" value. The sidecar marks
    # line 0 as a secret; the mask treats the rotation as
    # invisible.
    run dodot transform check
    [ "$status" -eq 0 ]

    # Source template still references via `secret()` — the
    # `{{ secret("...") }}` expression is intact, no literal value.
    src="$DOTFILES_ROOT/app/config.toml.tmpl"
    assert_file_contains "$src" 'secret("pass:test/db_password")'
    assert_file_contents "$src" 'pw = "{{ secret("pass:test/db_password") }}"'
}

@test "transform check still propagates non-secret line edits" {
    # Counter-test: the mask is per-line, not a blanket "no
    # reverse-merge" switch. A static-line edit on a *different*
    # line should still propagate to the template source.
    seed_pass_secret "test/api_key" "tok-xyz"
    secrets_enable_pass_in_root_config
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'port = 5432
key = "{{ secret("pass:test/api_key") }}"'

    run dodot up
    [ "$status" -eq 0 ]
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    assert_file_contains "$rendered" 'port = 5432'
    assert_file_contains "$rendered" 'key = "tok-xyz"'

    # Edit the unmasked static line (port). Leave the secret line
    # alone.
    cat > "$rendered" <<'EDITED'
port = 9999
key = "tok-xyz"
EDITED

    run dodot transform check
    [ "$status" -eq 0 ]

    # The static-line edit should propagate to the source template:
    src="$DOTFILES_ROOT/app/config.toml.tmpl"
    assert_file_contains "$src" 'port = 9999'
    # The secret() expression should remain intact.
    assert_file_contains "$src" 'secret("pass:test/api_key")'
}
