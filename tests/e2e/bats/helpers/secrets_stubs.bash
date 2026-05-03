#!/usr/bin/env bash
# Secrets E2E test helpers — stub provider binaries on PATH plus
# fixture catalog seeding.
#
# Phase S1 shipped `pass` (canonical reference impl). Phase S2 adds
# `bw` (Bitwarden CLI). The stubs cover the narrow command shape
# each `crates/dodot-lib/src/secret/<provider>.rs` calls so e2e
# tests exercise the full `dodot up` integration without requiring
# real CLIs / accounts on the host. Same precedent as the brew
# muzzle pattern (PR dodot#120) — a stub script on PATH, prepended
# in setup, scrubbed at teardown alongside the rest of $SANDBOX.

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

# Drop the pass stub AND make `pass` genuinely unspawnable from this
# point on — used by tests that exercise the "binary not installed"
# probe path. The provider distinguishes spawn-failure (Err →
# NotInstalled) from spawn-then-exit-non-zero (Ok → ProbeFailed), so
# we need `Command::new("pass").spawn()` to fail outright.
#
# Approach: remove our stub dir AND walk PATH stripping any entry
# that contains a `pass` binary. Without the strip, on a developer
# machine with pass installed via brew / apt / pacman / nix, the
# host's binary resolves through and the probe returns Ok — making
# the test pass-or-fail dependent on host config. After the strip,
# spawn fails everywhere with a real "command not found" → the
# probe walks the documented NotInstalled path.
secrets_drop_pass_stub() {
    if [[ -d "$SANDBOX/.secrets-stubs" ]]; then
        rm -rf "$SANDBOX/.secrets-stubs"
    fi
    local clean=""
    local IFS=':'
    local entry
    for entry in $PATH; do
        if [[ -n "$entry" && -x "$entry/pass" ]]; then
            continue
        fi
        if [[ -z "$clean" ]]; then
            clean="$entry"
        else
            clean="$clean:$entry"
        fi
    done
    export PATH="$clean"
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

# ── bw stub ─────────────────────────────────────────────────────

# Set up a stub `bw` binary on PATH that mimics the surface
# `crates/dodot-lib/src/secret/bw.rs` calls:
#   - `bw --version`            → exit 0 with version banner
#   - `bw status`               → JSON with `status` field
#   - `bw get <field> <item>`   → resolves from a per-field catalog
#
# Each field has its own catalog file under
# `$SANDBOX/.secrets-stubs/bw-<field>` so the stub doesn't need a
# multi-column data shape. Tests append entries with
# `seed_bw_secret <item> <field> <value>`.
secrets_bw_stub_setup() {
    local stub_dir="$SANDBOX/.secrets-stubs"
    mkdir -p "$stub_dir"
    : > "$stub_dir/bw-status"
    # Default to "unlocked" — tests that want to exercise the
    # locked / unauthenticated probe paths overwrite this file.
    echo 'unlocked' > "$stub_dir/bw-status"

    cat > "$stub_dir/bw" <<'STUB'
#!/usr/bin/env bash
# Stub `bw` for dodot e2e tests. Reads catalog files and the
# sidecar-style status file written by helpers in
# secrets_stubs.bash.
DIR="$(dirname "$0")"

case "$1" in
    --version)
        echo "2026.4.1-stub"
        exit 0
        ;;
    status)
        status="$(cat "$DIR/bw-status" 2>/dev/null || echo unlocked)"
        printf '{"serverUrl":null,"status":"%s"}\n' "$status"
        exit 0
        ;;
    get)
        field="$2"
        item="$3"
        catalog="$DIR/bw-$field"
        if [[ ! -f "$catalog" ]]; then
            echo "Not found." >&2
            exit 1
        fi
        line="$(awk -F '\t' -v r="$item" '$1 == r { print $2; found=1; exit } END { exit !found }' "$catalog")"
        rc=$?
        if [[ $rc -ne 0 ]]; then
            echo "Not found." >&2
            exit 1
        fi
        printf '%s\n' "$line"
        exit 0
        ;;
    *)
        echo "bw stub: unsupported command: $1" >&2
        exit 2
        ;;
esac
STUB
    chmod +x "$stub_dir/bw"
    export PATH="$stub_dir:$PATH"
}

# Append (item, field, value) to the bw stub catalog.
# Usage: seed_bw_secret "gh-token" "password" "ghp_xyz"
seed_bw_secret() {
    local item="$1"
    local field="$2"
    local value="$3"
    printf '%s\t%s\n' "$item" "$value" >> "$SANDBOX/.secrets-stubs/bw-$field"
}

# Override the bw status the stub reports. Valid: unlocked, locked,
# unauthenticated. Default is unlocked.
set_bw_stub_status() {
    echo "$1" > "$SANDBOX/.secrets-stubs/bw-status"
}

# Flip the bw provider on in the root .dodot.toml.
secrets_enable_bw_in_root_config() {
    cat > "$DOTFILES_ROOT/.dodot.toml" <<TOML
[secret]
enabled = true

[secret.providers.bw]
enabled = true
TOML
}

# ── age + gpg whole-file fixtures ───────────────────────────────
#
# Phase S3 brings two whole-file decryption preprocessors. Unlike
# the value-injection providers above, these need real binaries —
# stubbing age/gpg's crypto would defeat the purpose. The bats
# tests skip when the binary isn't available on the host, mirroring
# the precedent in `test_brew_probe.bats`.

# Set up a no-passphrase age keypair under $SANDBOX/age, write the
# identity to $AGE_IDENTITY, and export the public recipient string
# for the test to encrypt with.
#
# Skips the calling test (returns 1) when `age` isn't on PATH.
secrets_age_setup() {
    if ! command -v age >/dev/null 2>&1 || ! command -v age-keygen >/dev/null 2>&1; then
        return 1
    fi
    local dir="$SANDBOX/age"
    mkdir -p "$dir"
    age-keygen -o "$dir/identity.txt" 2> "$dir/keygen.stderr"
    # age-keygen prints `# public key: <recipient>` to stderr.
    AGE_RECIPIENT="$(awk -F': ' '/public key/ { print $2 }' "$dir/keygen.stderr")"
    if [[ -z "$AGE_RECIPIENT" ]]; then
        echo "secrets_age_setup: failed to extract age recipient" >&2
        return 1
    fi
    export AGE_IDENTITY="$dir/identity.txt"
    export AGE_RECIPIENT
}

# Encrypt $3 (literal plaintext bytes) into the pack at $1/$2 with
# the .age suffix appended. Usage:
#     seed_age_encrypted_file "ssh" "id_ed25519" "<key bytes>"
# produces $DOTFILES_ROOT/ssh/id_ed25519.age.
seed_age_encrypted_file() {
    local pack="$1"
    local rel_path="$2"
    local plaintext="$3"
    local out="$DOTFILES_ROOT/$pack/$rel_path.age"
    mkdir -p "$(dirname "$out")"
    printf '%s' "$plaintext" | age -r "$AGE_RECIPIENT" -o "$out"
}

# Flip on the age preprocessor in the root config and point at the
# sandbox identity file generated by `secrets_age_setup`.
secrets_enable_age_in_root_config() {
    cat > "$DOTFILES_ROOT/.dodot.toml" <<TOML
[preprocessor.age]
enabled = true
identity = "$AGE_IDENTITY"
TOML
}

# Set up a no-passphrase gpg keypair under $SANDBOX/gnupg as a
# self-contained $GNUPGHOME. Generates a deterministic UID
# `dodot-test@example.invalid` and exports `GPG_RECIPIENT` for
# encryption.
#
# Skips when `gpg` isn't on PATH.
secrets_gpg_setup() {
    if ! command -v gpg >/dev/null 2>&1; then
        return 1
    fi
    local dir="$SANDBOX/gnupg"
    mkdir -p "$dir"
    chmod 700 "$dir"
    export GNUPGHOME="$dir"
    cat > "$dir/keygen.batch" <<'BATCH'
%no-protection
Key-Type: RSA
Key-Length: 2048
Name-Real: Dodot Test
Name-Email: dodot-test@example.invalid
Expire-Date: 0
%commit
BATCH
    gpg --batch --gen-key "$dir/keygen.batch" 2>/dev/null
    export GPG_RECIPIENT='dodot-test@example.invalid'
}

# Encrypt plaintext into a pack file with the .gpg suffix appended.
seed_gpg_encrypted_file() {
    local pack="$1"
    local rel_path="$2"
    local plaintext="$3"
    local out="$DOTFILES_ROOT/$pack/$rel_path.gpg"
    mkdir -p "$(dirname "$out")"
    printf '%s' "$plaintext" | gpg --encrypt --recipient "$GPG_RECIPIENT" --batch \
        --trust-model always --output "$out" 2>/dev/null
}

# Flip on the gpg preprocessor in the root config. gpg picks up its
# identity from gpg-agent + $GNUPGHOME (already set by setup).
secrets_enable_gpg_in_root_config() {
    cat > "$DOTFILES_ROOT/.dodot.toml" <<TOML
[preprocessor.gpg]
enabled = true
TOML
}

# ── keychain (macOS) + secret-tool (Linux) fixtures ─────────────
#
# Phase S4 OS-level providers (`keychain` / `secret-tool`)
# deliberately have NO bats / dev-shell fixtures here. Both
# providers talk to the user's live OS keystore — macOS Keychain
# Access prompts on first write, freedesktop Secret Service
# leaves entries persisting across daemon restarts — and any
# leftover state from a botched test cleanup ends up in the
# real keystore.
#
# Per `secrets-testing.lex` §5.3 the e2e for these lands when
# the dedicated CI runners arrive: macOS CI with a sandboxed
# `security create-keychain` against an isolated keychain DB,
# Linux CI with `dbus-daemon` + `gnome-keyring-daemon
# --start --components=secrets` against a per-test session.
# Until then, Phase S4 ships with tier-0 unit tests only and
# documents the e2e gap.
