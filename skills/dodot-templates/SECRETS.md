# Secrets reference

dodot keeps secrets out of git two ways. It does **not** own encryption or vault
custody — it delegates to tools you already use and only ensures plaintext doesn't
land in git or sit in cleartext on disk longer than necessary.

- **Value injection** — a template references one secret via
  `{{ secret("scheme:reference") }}`, resolved at deploy through a provider and
  substituted into the rendered output. Source stays committable; deployed file has
  the real value. Single-line only — multi-line values are refused at render time.
- **Whole-file decryption** — a `.age`/`.gpg` file is encrypted at rest; `dodot up`
  decrypts to a 0600 datastore file and the symlink handler links it. No template
  expansion; the whole bytestream is the secret.

Pick value-injection when the secret is one line inside a config with readable
non-secret fields; pick whole-file when the secret *is* the file (SSH key, cert,
service-account JSON).

## The six providers

| Scheme        | Tool                       | Reference syntax                  |
|---------------|----------------------------|-----------------------------------|
| `pass`        | password-store             | `pass:path/to/entry`              |
| `op`          | 1Password CLI              | `op://Vault/Item/Field`           |
| `bw`          | Bitwarden CLI              | `bw:item-name[#field]`            |
| `sops`        | Mozilla SOPS               | `sops:file.yaml#dot.path`         |
| `keychain`    | macOS Keychain             | `keychain:service[/account]`      |
| `secret-tool` | freedesktop Secret Service | `secret-tool:service[/account]`   |

Enable per scheme (all default to `enabled = false` so a fresh install never shells
out unprompted):

```toml
[secret]
enabled = true

[secret.providers.pass]
enabled = true
```

The TOML key for the freedesktop provider is `secret_tool` (underscore); the
reference prefix is `secret-tool:` (hyphen).

Provider quirks:

- **pass** — returns the first line of `~/.password-store/<entry>.gpg`; override with
  `[secret.providers.pass] store_dir`.
- **op** — requires `OP_SERVICE_ACCOUNT_TOKEN`; the desktop-app fallback is
  deliberately disabled (would pop a biometric prompt mid-render).
- **bw** — needs an unlocked vault (`bw unlock`, export `BW_SESSION`).
  `bw:item#field` picks password/username/notes/totp/uri.
- **sops** — file paths anchored at the dotfiles root; the dot path becomes SOPS
  `--extract` bracket-notation.
- **keychain** (macOS) — `keychain:Service[/account]`; never calls
  `unlock-keychain` itself.
- **secret-tool** (Linux) — libsecret lookup against the session keyring; the
  session daemon handles unlocking.

## Whole-file: `.age` / `.gpg`

```toml
[preprocessor.age]
enabled = true     # identity defaults to ~/.config/age/identity.txt

[preprocessor.gpg]
enabled = true     # picks up keys from gpg-agent / ~/.gnupg
```

Decrypted output lands at mode 0600 atomically. `.asc` is **not** in the default gpg
extension list (it's conventionally armored public keys/signatures); opt in with
`extensions = ["gpg", "asc"]` only if your repo stores armored payloads as `.asc`.
gpg runs with `--batch` (never prompts) — use a smartcard/YubiKey key or pre-cache
the passphrase in `gpg-agent`.

## The edit loop

- **Value injection** — same as any template: edit source, `dodot up`, the value
  re-resolves. Your vault is the source of truth; rotating a value flows in on the
  next `up`.
- **Whole-file** — dodot deliberately has **no `dodot secret edit`**. Do it by hand:

  ```bash
  age -d -i ~/.config/age/identity.txt ssh/id_ed25519.age > /tmp/key
  $EDITOR /tmp/key
  age -e -r age1... -o ssh/id_ed25519.age /tmp/key
  rm /tmp/key && git add ssh/id_ed25519.age && git commit
  ```

  Hand-editing the *deployed* plaintext is preserved with a "preserved" warning on
  next `up` (no auto-merge) — re-encrypt the source instead.

## Inspecting (read-only — never triggers auth prompts)

- `dodot secret list` — every `secret(...)` reference across the repo, flagging
  schemes with no provider enabled. Run before the first `up` to inventory needs.
- `dodot secret probe` — runs `probe()` on each enabled provider:
  ok / not installed / not authenticated / misconfigured. Run when something fails.
- `dodot transform status` — shows which secrets each cached baseline depends on.

`dodot up` runs one preflight pass over enabled providers and aggregates all
failures into a single error. Repeated references to the same secret in one `up`
resolve once. `--dry-run` and `status` skip preflight (they read the cached
baseline, never touching providers).

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `provider 'op' is not authenticated` | set `OP_SERVICE_ACCOUNT_TOKEN` (no desktop fallback) |
| `provider 'pass' is not installed` | install `pass`, or disable the provider |
| `provider 'pass' is misconfigured` | `pass init <gpg-key>`, or set `store_dir` |
| `secret '…' resolved to a multi-line value` | value injection is single-line; use whole-file `.age`/`.gpg` |
| `gpg: bad passphrase / session key` | `--batch` won't prompt; pre-cache passphrase in gpg-agent |
| `key path 'a.b' not found in …` | SOPS dot path wrong; verify with `sops -d file \| yq` |
| `secret(...)` silently doesn't render | need both `[secret] enabled` and `[secret.providers.<scheme>] enabled` |

## Trust boundary

dodot's property is "don't ship plaintext to git." It does **not** promise an
attacker with code execution as your user can't read secrets — the rendered file is
plaintext on disk by design. Encryption, key custody, and runtime security are the
provider tools' job, not dodot's.
