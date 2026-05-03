#!/usr/bin/env bash
# dev-shell.sh — drop into an interactive sandbox with one of the
# secrets fixtures pre-installed.
#
# Per `docs/proposals/secrets-testing.lex` §7.1: the bats setup model
# is per-test, but development / debugging / AI-agent exploration
# wants a shell that already has the fixture initialised so commands
# like `pass show test/db_password` and `dodot up` Just Work.
#
# Usage:
#   ./tests/e2e/bats/helpers/dev-shell.sh [fixture-name]
#
# Available fixtures:
#   secrets-pass         pass stub on PATH + initialised store + 4 entries  (Phase S1)
#   secrets-bw-stub      bw stub binary on PATH + 4 seeded items            (Phase S2)
#   secrets-age          real age binary + fresh keypair + encrypted blob   (Phase S3)
#   secrets-gpg          real gpg binary + fresh keypair + encrypted blob   (Phase S3)
#
# Future fixtures (will be added by their respective phases):
#   secrets-sops         sops + age (Phase S2 tier-1 hermetic)
#   secrets-op-stub      op stub binary on PATH
#   secrets-op-real      real `op` CLI; needs OP_SERVICE_ACCOUNT_TOKEN
#   secrets-bw-real      real `bw` CLI; needs BW_CLIENT_ID + BW_CLIENT_SECRET
#
# On exit (Ctrl-D / `exit`), the sandbox is removed. Nothing leaks.

set -euo pipefail

fixture="${1:-secrets-pass}"

_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_PROJECT_ROOT="$(cd "$_SCRIPT_DIR/../../../.." && pwd)"

# Reuse the bats setup helpers. The order matches what setup() in a
# bats test would do: setup.bash first (so $SANDBOX, $HOME, etc. are
# initialised), then the fixture-specific helper.
# shellcheck source=setup.bash
source "$_SCRIPT_DIR/setup.bash"
# shellcheck source=secrets_stubs.bash
source "$_SCRIPT_DIR/secrets_stubs.bash"

sandbox_setup
trap 'sandbox_teardown' EXIT

case "$fixture" in
    secrets-pass)
        secrets_pass_stub_setup
        seed_pass_secret 'test/db_password' 'hunter2-from-fixture'
        seed_pass_secret 'test/api_key'     'fixture-api-key'
        seed_pass_secret 'test/github_token' 'ghp_fixture_token'
        seed_pass_secret 'test/tls_cert'    'fixture-cert-blob'
        secrets_enable_pass_in_root_config
        cat <<EOF
[sandbox: $SANDBOX]
[fixture: $fixture]
[pass entries seeded: test/db_password, test/api_key, test/github_token, test/tls_cert]
[\$DOTFILES_ROOT: $DOTFILES_ROOT]
[\$PASSWORD_STORE_DIR: $PASSWORD_STORE_DIR]
[\$HOME: $HOME]

Try:
  pass show test/db_password
  dodot up
  dodot status
EOF
        ;;
    secrets-bw-stub)
        secrets_bw_stub_setup
        seed_bw_secret 'gh-token'    'password' 'ghp_fixture_token'
        seed_bw_secret 'gh-token'    'username' 'debert+dodot'
        seed_bw_secret 'db'          'password' 'hunter2-from-fixture'
        seed_bw_secret 'api-key'     'password' 'fixture-api-key'
        seed_bw_secret 'tls-cert'    'notes'    'fixture-cert-blob'
        secrets_enable_bw_in_root_config
        cat <<EOF
[sandbox: $SANDBOX]
[fixture: $fixture]
[bw items seeded:
  gh-token (password, username),
  db (password),
  api-key (password),
  tls-cert (notes)]
[\$DOTFILES_ROOT: $DOTFILES_ROOT]
[\$HOME: $HOME]

Try:
  bw status
  bw get password gh-token
  bw get username gh-token
  dodot up
  dodot status
EOF
        ;;
    secrets-age)
        if ! secrets_age_setup; then
            echo "dev-shell: age / age-keygen not installed on this host" >&2
            echo "Install with: brew install age   (or apt install age)" >&2
            exit 2
        fi
        seed_age_encrypted_file 'ssh' 'id_ed25519' 'fixture-private-key-bytes'
        seed_age_encrypted_file 'vault' 'secret.txt' 'fixture-secret-content'
        secrets_enable_age_in_root_config
        cat <<EOF
[sandbox: $SANDBOX]
[fixture: $fixture]
[age identity: $AGE_IDENTITY]
[age recipient: $AGE_RECIPIENT]
[encrypted fixtures:
  ssh/id_ed25519.age   (recipient: $AGE_RECIPIENT)
  vault/secret.txt.age]
[\$DOTFILES_ROOT: $DOTFILES_ROOT]
[\$HOME: $HOME]

Try:
  age -d -i \$AGE_IDENTITY \$DOTFILES_ROOT/ssh/id_ed25519.age
  dodot up
  ls -la \$HOME/.config/ssh/   # → id_ed25519, mode 0600
EOF
        ;;
    secrets-gpg)
        if ! secrets_gpg_setup; then
            echo "dev-shell: gpg not installed on this host" >&2
            echo "Install with: brew install gnupg   (or apt install gnupg)" >&2
            exit 2
        fi
        seed_gpg_encrypted_file 'shell' 'Brewfile' 'brew "ripgrep"
brew "fd"'
        seed_gpg_encrypted_file 'vault' 'secret.txt' 'fixture-secret-content'
        secrets_enable_gpg_in_root_config
        cat <<EOF
[sandbox: $SANDBOX]
[fixture: $fixture]
[\$GNUPGHOME: $GNUPGHOME]
[gpg recipient: $GPG_RECIPIENT]
[encrypted fixtures:
  shell/Brewfile.gpg
  vault/secret.txt.gpg]
[\$DOTFILES_ROOT: $DOTFILES_ROOT]
[\$HOME: $HOME]

Try:
  gpg --decrypt \$DOTFILES_ROOT/shell/Brewfile.gpg
  dodot up
  ls -la \$HOME/.config/shell/   # → Brewfile, mode 0600
EOF
        ;;
    *)
        echo "dev-shell: unknown fixture '$fixture'" >&2
        echo "Available: secrets-pass, secrets-bw-stub, secrets-age, secrets-gpg" >&2
        exit 2
        ;;
esac

# Drop into an interactive subshell. The trap above tears the
# sandbox down on exit, regardless of how the shell exits.
exec "${SHELL:-/bin/bash}" -i
