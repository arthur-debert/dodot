#!/usr/bin/env bats
# Phase S3 secrets E2E — `gpg` whole-file decryption.
#
# Tier-1 hermetic: each test generates a fresh no-passphrase gpg
# keypair under $SANDBOX/gnupg as a self-contained $GNUPGHOME,
# encrypts a fixture, runs `dodot up`, asserts the deployed file
# is plaintext at mode 0600. Skips when `gpg` isn't on the host.

setup() {
    load helpers/setup
    sandbox_setup
    load helpers/secrets_stubs
    if ! secrets_gpg_setup; then
        skip "gpg not installed on this host"
    fi
}

teardown() {
    sandbox_teardown
}

@test "gpg preprocessor decrypts a *.gpg file end-to-end and lands plaintext" {
    # Use a generic filename here — `Brewfile` is special-cased by
    # dodot's homebrew handler and would route differently. This
    # test isolates the preprocessor + symlink path; the homebrew-
    # routing case lands separately if needed.
    seed_gpg_encrypted_file "config" "secrets.toml" 'token = "ghp_xyz"
api_url = "https://example.invalid/v1"'
    secrets_enable_gpg_in_root_config

    run dodot up
    [ "$status" -eq 0 ]

    [ -L "$HOME/.config/config/secrets.toml" ]
    assert_file_contains "$HOME/.config/config/secrets.toml" 'token = "ghp_xyz"'
    assert_file_contains "$HOME/.config/config/secrets.toml" 'api_url'
}

@test "gpg preprocessor preserves an edited deployed file on the next dodot up" {
    # `secrets.lex` §4.4: with the §6.4 divergence guard, a user
    # who hand-edited the deployed plaintext (without re-encrypting
    # the source) must NOT have that edit silently overwritten on
    # the next `dodot up`. Phase S3's whole-file secrets opt into
    # the guard via `deploy_mode = Some(0o600)`.
    seed_gpg_encrypted_file "config" "secrets.toml" 'token = "ORIGINAL"'
    secrets_enable_gpg_in_root_config

    run dodot up
    [ "$status" -eq 0 ]
    local rendered="$XDG_DATA_HOME/dodot/packs/config/preprocessed/secrets.toml"
    assert_file_contains "$rendered" 'token = "ORIGINAL"'

    # User edits the deployed plaintext directly (forgot to
    # re-encrypt — common rotation mistake).
    echo 'token = "USER_EDITED"' > "$rendered"

    # Next `dodot up` must preserve the edit, not clobber it.
    run dodot up
    [ "$status" -eq 0 ]
    assert_file_contains "$rendered" 'USER_EDITED'
    assert_output_contains "preserved"
}

@test "gpg preprocessor enforces mode 0600 on the rendered datastore file" {
    seed_gpg_encrypted_file "vault" "secret" "private-data"
    secrets_enable_gpg_in_root_config

    run dodot up
    [ "$status" -eq 0 ]

    local rendered="$XDG_DATA_HOME/dodot/packs/vault/preprocessed/secret"
    [ -f "$rendered" ] || {
        echo "expected rendered file at $rendered" >&2
        return 1
    }
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

@test "gpg preprocessor decrypts ASCII-armored *.asc files when explicitly enabled" {
    # `.asc` is conventionally used for armored *public keys* and
    # *detached signatures*, not encrypted payloads — so it's no
    # longer in the default extension list. Users whose repos do
    # store encrypted artifacts as `.asc` opt in by setting
    # `extensions = ["gpg", "asc"]` explicitly.
    local plaintext='private notes'
    local out="$DOTFILES_ROOT/notes/secret.txt.asc"
    mkdir -p "$(dirname "$out")"
    printf '%s' "$plaintext" | \
        gpg --encrypt --armor --recipient "$GPG_RECIPIENT" \
            --batch --trust-model always --output "$out" 2>/dev/null

    cat > "$DOTFILES_ROOT/.dodot.toml" <<'TOML'
[preprocessor.gpg]
enabled = true
extensions = ["gpg", "asc"]
TOML

    run dodot up
    [ "$status" -eq 0 ]
    [ -L "$HOME/.config/notes/secret.txt" ]
    assert_file_contents "$HOME/.config/notes/secret.txt" "$plaintext"
}
