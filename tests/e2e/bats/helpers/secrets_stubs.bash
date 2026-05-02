#!/usr/bin/env bash
# Secrets E2E test helpers — stub provider binaries on PATH plus
# fixture catalog seeding.
#
# Phase S1 ships the `pass` provider as the canonical reference impl
# (the real `pass` binary is hermetically testable in tier 1 — see
# `docs/proposals/secrets-testing.lex` §4). The stubs here cover the
# narrow command shape `crates/dodot-lib/src/secret/pass.rs` calls
# (`pass version`, `pass show <ref>`) so e2e tests exercise the full
# `dodot up` integration without requiring `gpg` / a real password
# store on the host. Same precedent as the brew muzzle pattern (PR
# dodot#120) — a stub script on PATH, prepended in setup, scrubbed at
# teardown alongside the rest of $SANDBOX.

# Set up a stub `pass` binary on PATH plus an initialised
# `$PASSWORD_STORE_DIR`. The store gets a `.gpg-id` file so the
# provider's probe (`crates/dodot-lib/src/secret/pass.rs`) treats it
# as initialised.
#
# Tests append entries with `seed_pass_secret`. Resolved values are
# printed verbatim by the stub on `pass show <ref>`; unknown
# references map to the documented `not in the password store`
# stderr (exit 1) so the provider's error mapping fires.
secrets_pass_stub_setup() {
    local stub_dir="$SANDBOX/.secrets-stubs"
    local store_dir="$SANDBOX/password-store"
    local catalog="$stub_dir/pass-catalog"

    mkdir -p "$stub_dir" "$store_dir"
    : > "$catalog"

    echo 'dodot-test@example.invalid' > "$store_dir/.gpg-id"

    cat > "$stub_dir/pass" <<'STUB'
#!/usr/bin/env bash
# Stub `pass` for dodot e2e tests.
# Catalog: tab-separated <ref>\t<value> lines, one per entry.
CATALOG="$(dirname "$0")/pass-catalog"

case "$1" in
    version)
        echo "pass-stub 1.0"
        exit 0
        ;;
    show)
        ref="$2"
        # Match on the full reference up to the first tab.
        line="$(awk -F '\t' -v r="$ref" '$1 == r { print $2; found=1; exit } END { exit !found }' "$CATALOG")"
        rc=$?
        if [[ $rc -ne 0 ]]; then
            echo "Error: $ref is not in the password store." >&2
            exit 1
        fi
        printf '%s\n' "$line"
        exit 0
        ;;
    *)
        echo "pass stub: unsupported subcommand: $1" >&2
        exit 2
        ;;
esac
STUB
    chmod +x "$stub_dir/pass"

    export PATH="$stub_dir:$PATH"
    export PASSWORD_STORE_DIR="$store_dir"
}

# Append a (reference, value) pair to the pass stub catalog.
# Usage: seed_pass_secret "test/db_password" "hunter2-from-fixture"
seed_pass_secret() {
    local ref="$1"
    local value="$2"
    printf '%s\t%s\n' "$ref" "$value" >> "$SANDBOX/.secrets-stubs/pass-catalog"
}

# Drop the pass stub from PATH for tests that need to exercise the
# "binary not installed" probe path. The store directory and env var
# stay set; the stub disappears, so `pass version` fails with
# "command not found" and the provider's NotInstalled probe fires.
secrets_drop_pass_stub() {
    if [[ -d "$SANDBOX/.secrets-stubs" ]]; then
        rm -rf "$SANDBOX/.secrets-stubs"
    fi
}

# Helper for the common case: enable the pass provider + point at the
# sandbox store + flip the master switch. Writes the root .dodot.toml.
# Tests can append further config before calling dodot.
secrets_enable_pass_in_root_config() {
    cat > "$DOTFILES_ROOT/.dodot.toml" <<TOML
[secret]
enabled = true

[secret.providers.pass]
enabled = true
store_dir = "$PASSWORD_STORE_DIR"
TOML
}
