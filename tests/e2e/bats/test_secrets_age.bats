#!/usr/bin/env bats
# Phase S3 secrets E2E — `age` whole-file decryption.
#
# Tier-1 hermetic: each test generates a fresh age keypair under
# $SANDBOX, encrypts a fixture, runs `dodot up`, asserts the
# deployed file is plaintext at mode 0600. Skips when `age` /
# `age-keygen` aren't on the host (Linux CI without the mise
# install step yet, fresh macOS without `brew install age`).

setup() {
    load helpers/setup
    sandbox_setup
    load helpers/secrets_stubs
    if ! secrets_age_setup; then
        skip "age / age-keygen not installed on this host"
    fi
}

teardown() {
    sandbox_teardown
}

@test "age preprocessor decrypts a *.age file end-to-end and lands plaintext" {
    seed_age_encrypted_file "ssh" "id_ed25519" "test-private-key-bytes"
    secrets_enable_age_in_root_config

    run dodot up
    [ "$status" -eq 0 ]

    # The .age suffix is stripped; the deployed file is plaintext.
    [ -L "$HOME/.config/ssh/id_ed25519" ]
    assert_file_contents "$HOME/.config/ssh/id_ed25519" "test-private-key-bytes"
}

@test "age preprocessor enforces mode 0600 on the rendered datastore file" {
    # The rendered datastore copy (the symlink dereferences to it)
    # must land at exactly 0600 per `secrets.lex` §4.3 — even if
    # the umask would have produced a more permissive mode.
    seed_age_encrypted_file "ssh" "id_ed25519" "secret-bytes"
    secrets_enable_age_in_root_config

    run dodot up
    [ "$status" -eq 0 ]

    local rendered="$XDG_DATA_HOME/dodot/packs/ssh/preprocessed/id_ed25519"
    [ -f "$rendered" ] || {
        echo "expected rendered file at $rendered" >&2
        return 1
    }
    # `stat -f %A` on macOS / `stat -c %a` on Linux. Try both.
    local mode
    if stat -f %A "$rendered" >/dev/null 2>&1; then
        mode="$(stat -f %A "$rendered")"
    else
        mode="$(stat -c %a "$rendered")"
    fi
    [ "$mode" = "600" ] || {
        echo "expected mode 600, got $mode" >&2
        return 1
    }
}

@test "age preprocessor surfaces a recipient-mismatch with actionable hint" {
    # Encrypt with the real recipient, then point the config at a
    # different identity file. age fails with "no identity matched
    # any of the recipients"; we surface the §4.2 actionable hint.
    seed_age_encrypted_file "vault" "secret" "plaintext"
    # Generate a different identity and rewrite the config.
    age-keygen -o "$SANDBOX/age/wrong-identity.txt" 2>/dev/null
    cat > "$DOTFILES_ROOT/.dodot.toml" <<TOML
[preprocessor.age]
enabled = true
identity = "$SANDBOX/age/wrong-identity.txt"
TOML

    run dodot up
    # Per-pack errors don't fail the whole run; assert via output.
    assert_output_contains "no identity matched"
    assert_output_contains "Re-encrypt"
    [ ! -e "$HOME/.config/vault/secret" ]
}
