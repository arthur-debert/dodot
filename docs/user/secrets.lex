Secrets

    Some of your config has values that don't belong in git. API tokens, database passwords, private keys, certificate blobs. dodot has two shapes for keeping those out of source while letting the deployed file Just Work:

    - **Value injection.** A template references a single secret value via `{{ secret("scheme:reference") }}`. dodot resolves it at deploy time through a configured provider (your password manager, vault CLI, OS keystore) and substitutes it into the rendered output. Source stays committable; deployed file has the real value.
    - **Whole-file decryption.** A pack file ending in `.age` or `.gpg` is encrypted at rest in the repo. dodot decrypts it at deploy time, writes the plaintext to the datastore at mode 0600, and the symlink handler links it to the home destination. No template expansion involved — the entire bytestream is the secret.

    Both shapes share the same trust posture: dodot does not own encryption or vault custody. It delegates to the provider tools you already use (`pass`, `op`, `bw`, `sops`, `gpg`, `age`, the macOS Keychain, freedesktop Secret Service) and stays out of the credential-handling business. dodot's job is keeping plaintext out of git and out of cleartext-on-disk longer than necessary.

    :: note :: For the design-level view, see [./../reference/pre-processors.lex] §6 and the historical proposal at [./../proposals/shipped/secrets.lex]. For internals, see [./../dev/secret.lex].

1. The Two Shapes — Which One Do I Want?

    Pick **value injection** when:
        - The secret is a single line: an API token, a password, a connection string fragment.
        - The surrounding config has many non-secret fields you want to keep readable in `git diff`.
        - You want the secret to live in your existing password manager / vault / keychain, not as a separate file in your repo.

        Example: a `.netrc` template with one `password` line, or a `kubeconfig` with a `token:` field.

    Pick **whole-file decryption** when:
        - The secret IS the file: an SSH private key, a TLS cert bundle, a service-account JSON.
        - There are no static fields worth preserving for `git diff` — the bytestream is opaque to you anyway.
        - You're OK with the existing decrypt → edit → re-encrypt → commit loop for changes (no auto-merge from the deployed side).

        Example: `ssh/id_ed25519.age`, `vault/service-account.json.gpg`.

    The two shapes can coexist in the same pack. Multi-line secrets in the value-injection path are refused at render time — the renderer steers you to the whole-file path with an actionable error.

2. Quickstart: Value Injection With `pass`

    `pass` (the standard Unix password manager) is the simplest provider to get going. Assuming you already have `pass init <gpg-key>` set up:

        $ pass insert dodot/db_password
        Enter password for dodot/db_password: hunter2
        Retype password for dodot/db_password: hunter2

    Enable the provider in your dotfiles root config:

        $ cat ~/dotfiles/.dodot.toml
        [secret]
        enabled = true

        [secret.providers.pass]
        enabled = true

    :: shell ::

    Now write a template that references it:

        $ cat ~/dotfiles/app/config.toml.tmpl
        db_password = "{{ secret("pass:dodot/db_password") }}"
        port = 5432

        $ dodot up app
        ... symlink:  app/config.toml -> ~/.config/app/config.toml: deployed

        $ cat ~/.config/app/config.toml
        db_password = "hunter2"
        port = 5432

    :: shell ::

    The committed source has `{{ secret("pass:dodot/db_password") }}` — no plaintext. The deployed file has the resolved value. `git diff` sees the source; `cat ~/.config/app/config.toml` sees the deployed.

3. The Six Providers

    dodot ships six built-in providers. Each has its own reference syntax and config block. Pick whichever your existing setup already supports — none of them are dodot-specific tools.

    | Scheme        | Tool                          | Reference                                  |
    | pass          | password-store                | `pass:path/to/entry`                       |
    | op            | 1Password CLI                 | `op://Vault/Item/Field`                    |
    | bw            | Bitwarden CLI                 | `bw:item-name[#field]`                     |
    | sops          | Mozilla SOPS                  | `sops:file.yaml#dot.path`                  |
    | keychain      | macOS Keychain                | `keychain:service[/account]`               |
    | secret-tool   | freedesktop Secret Service    | `secret-tool:service[/account]`            |
    :: table ::

    All providers default to `enabled = false` so a fresh install never shells out unprompted. Flip the switch when you're ready:

        [secret.providers.pass]
        enabled = true

        [secret.providers.op]
        enabled = true

        [secret.providers.bw]
        enabled = true

        [secret.providers.sops]
        enabled = true

        [secret.providers.keychain]
        enabled = true

        [secret.providers.secret_tool]
        enabled = true

    :: note ::
        The TOML key for the freedesktop provider is `secret_tool` (underscore), but the reference prefix you write inside `secret(...)` is `secret-tool:` (hyphen, matching the binary name). The mismatch is a Rust-side field-name constraint; everywhere user-facing dodot translates between the two.

    Each provider has its own quirks worth knowing:

        - **`pass`**: `pass:foo/bar` returns the first line of the entry at `~/.password-store/foo/bar.gpg`. Set `[secret.providers.pass] store_dir = "/custom/path"` to override the default.
        - **`op`**: requires `OP_SERVICE_ACCOUNT_TOKEN` to be set in your environment. dodot deliberately doesn't fall back to the desktop-app integration — that path can pop a biometric prompt mid-render, which violates the §7.4 Passive contract.
        - **`bw`**: needs an unlocked vault. Run `bw unlock`, export the printed `BW_SESSION` value, then run dodot. `bw:gh-token` resolves the password field by default; `bw:gh-token#username` picks a different first-class field (password / username / notes / totp / uri).
        - **`sops`**: file paths are anchored at the dotfiles root by default — `sops:secrets.yaml#db.password` decrypts `<dotfiles>/secrets.yaml`. Absolute paths bypass the anchor. The dot path translates to SOPS's bracket-notation `--extract` argument.
        - **`keychain`** (macOS): `keychain:GitHub` finds the first item whose service is `GitHub`; `keychain:GitHub/alice` matches a specific (service, account) pair. Probes via `security default-keychain`; never calls `unlock-keychain` itself.
        - **`secret-tool`** (Linux): `secret-tool:GitHub[/alice]` does a libsecret lookup against the user's session keyring (gnome-keyring, keepassxc with the SecretService plugin, KDE Wallet). The session daemon handles unlocking.

4. Whole-File: `*.age` and `*.gpg`

    Drop an encrypted file into a pack, enable the matching preprocessor, and `dodot up` decrypts at deploy time:

        $ age-keygen -o ~/.config/age/identity.txt
        Public key: age1example...

        $ echo 'private key bytes' | age -r age1example... -o ~/dotfiles/ssh/id_ed25519.age

        $ cat ~/dotfiles/.dodot.toml
        [preprocessor.age]
        enabled = true
        # identity defaults to ~/.config/age/identity.txt; override here if needed.

        $ dodot up ssh
        ... symlink:  ssh/id_ed25519 -> ~/.config/ssh/id_ed25519: deployed

        $ ls -la ~/.local/share/dodot/packs/ssh/preprocessed/id_ed25519
        -rw-------  ...  id_ed25519

    :: shell ::

    The rendered file lands at mode 0600 atomically — no race window between write and chmod where another user could read the plaintext. This applies regardless of what mode the encrypted source had.

    For gpg, no `identity` config is needed — gpg picks up its key from `gpg-agent` and your existing `~/.gnupg/` setup:

        [preprocessor.gpg]
        enabled = true

    `*.asc` (ASCII-armored) is **not** in the default extension list because `.asc` is conventionally used for armored public keys and detached signatures — neither of which `gpg --decrypt` handles. Opt in explicitly only if your repo stores armored encrypted payloads as `.asc`:

        [preprocessor.gpg]
        enabled = true
        extensions = ["gpg", "asc"]

    :: warning ::
        gpg runs with `--batch`, which means it never prompts for a passphrase. You either need to be using a smartcard / YubiKey-backed key, or have your passphrase already cached in `gpg-agent` (decrypt one file interactively before running dodot to populate the cache). Locked-keyring failures surface with a clear "cache the passphrase first" hint.

5. The Edit Loop

    For value injection, the workflow is the same as any other dodot template: edit the template source, run `dodot up`, the new value is resolved and rendered. Your password manager is the source of truth; rotating a vault value flows automatically into the next `up`.

    For whole-file secrets, dodot deliberately does NOT wrap the edit loop. There is no `dodot secret edit` and there will not be one. The expert workflow is:

        $ age -d -i ~/.config/age/identity.txt ~/dotfiles/ssh/id_ed25519.age > /tmp/key
        $ $EDITOR /tmp/key
        $ age -e -r age1example... -o ~/dotfiles/ssh/id_ed25519.age /tmp/key
        $ rm /tmp/key
        $ git add ssh/id_ed25519.age && git commit

    :: shell ::

    Why no wrapper: a safe edit-and-re-encrypt loop needs platform-specific secure ephemeral storage (`tmpfs` on Linux, encrypted ramdisks on macOS, `unlink`-on-exit semantics that don't survive crashes). dodot's threat model treats runtime security as out of scope — your existing tools handle that better than dodot would. See the proposal at [./../proposals/shipped/secrets.lex] §4.5 for the full rationale.

    The same applies to the deployed plaintext: if you hand-edit `~/.config/ssh/id_ed25519` directly, `dodot up` will preserve the edit (the §6.4 divergence guard fires for whole-file secrets too) and surface a "preserved" warning on the next run. dodot won't auto-merge — re-encrypt the source and commit, same as any other manual edit.

6. Inspecting Your Setup

    Two read-only commands help you understand state without running `dodot up`:

    `dodot secret probe` runs `probe()` on every configured provider and reports the outcome — Ok / not installed / not authenticated / misconfigured. Useful when something fails: probe tells you which provider is broken before you spend time hunting through individual `secret(...)` calls.

        $ dodot secret probe
        2 ok, 1 need attention

        ✓ pass     ok
        ✗ op       not authenticated
            set OP_SERVICE_ACCOUNT_TOKEN
            (https://developer.1password.com/docs/service-accounts/)
        ✓ keychain ok

    :: shell ::

    `dodot secret list` walks every pack's templates and lists every `secret(...)` reference it finds, with a per-row warning when the referenced scheme has no provider enabled. Useful BEFORE the first `dodot up` to inventory which providers a repo needs:

        $ dodot secret list
        3 secret references across 2 schemes

        · app/config.toml.tmpl:1     pass:dodot/db_password
        · app/config.toml.tmpl:2     op://Personal/api/token (provider not enabled)
        · infra/secrets.tmpl:5       bw:gh-token (provider not enabled)

        Schemes referenced but not enabled in your config: bw, op
        Run `dodot secret probe` to inspect provider state, or enable a scheme via [secret.providers.<key>] enabled = true.

    :: shell ::

    `dodot transform status` (read-only) also surfaces secret references inline under each baseline so you can see WHICH secrets each cached file depends on without re-rendering:

        $ dodot transform status
        1 synced, 0 diverged, 0 missing

        · synced  app/config.toml ~/.config/app/config.toml
            secret: pass:dodot/db_password

    :: shell ::

7. What Happens on `dodot up`

    Before any rendering starts, dodot runs a single preflight pass over your enabled providers — runs `probe()` on each one and aggregates failures into a single error. If `op` isn't authenticated and `pass` has a missing keystore, you see both fix-it pointers in one message instead of one error per failing template:

        $ dodot up
        error: 2 secret provider(s) need attention before `dodot up` can resolve secrets:

        secret provider `op` is not authenticated
          set OP_SERVICE_ACCOUNT_TOKEN ...

        secret provider `pass` is misconfigured
          password store not initialised at /home/x/.password-store ...

    :: shell ::

    Preflight only runs in the active path — `dodot up --dry-run` and `dodot status` skip it (they read from the cached baseline instead, never touching providers). This is the §7.4 Passive contract: read-only commands never trigger auth prompts.

    Within a single `dodot up` run, repeated references to the same secret across templates resolve once. If `{{ secret("op://Personal/db") }}` appears in five templates, dodot calls `op` once and reuses the cached value for the other four — your password manager doesn't pop up five times.

8. Trust Boundaries — What dodot Doesn't Do

    Worth knowing what's NOT in dodot's lane:

    - **dodot doesn't own encryption.** The provider tools (`age`, `gpg`, `op`, `pass`, `sops`, `bw`, the OS keystores) handle key custody, vault access, and decryption. dodot delegates and stays out.
    - **dodot doesn't try to keep plaintext out of memory beyond best-effort.** `SecretString` zeroes its buffer on drop and refuses `Debug` / `Display` formatting, but the rendered template content lands on disk in plaintext (that's the entire point of the deploy step). Defense in depth, not a guarantee.
    - **dodot doesn't run editors on encrypted files.** No `dodot secret edit` — see §5 above.
    - **dodot's threat model is supply-chain control, not runtime security.** "Don't ship plaintext to git" is the property dodot upholds; "an attacker with code execution as your user can't read your secrets" is not, and never was. The `secrets.lex` §2.4 threat model is the reference.

9. Troubleshooting

    Common failure modes and what they mean:

    `secret provider 'op' is not authenticated`
        Set `OP_SERVICE_ACCOUNT_TOKEN`. The desktop-app fallback is deliberately disabled — dodot won't let `dodot up` block on a biometric prompt.

    `secret provider 'pass' is not installed`
        The `pass` binary isn't on `PATH`. Install via `brew install pass` / `apt install pass`. If you don't actually use `pass`, set `[secret.providers.pass] enabled = false`.

    `secret provider 'pass' is misconfigured`
        `pass` is installed but the password store isn't initialised. Run `pass init <gpg-key-id>`, or set `[secret.providers.pass] store_dir` to point at an existing store.

    `secret 'pass:foo/bar' resolved to a multi-line value`
        Value injection is single-line only. The full message points you at the whole-file deploy path — encrypt the file with `age` or `gpg`, drop it in a pack with the `.age` / `.gpg` suffix, and reference the deployed path from your config instead of injecting the value.

    `gpg: bad passphrase / session key`
        gpg's `--batch` mode doesn't prompt for passphrases. Decrypt one file interactively first to populate `gpg-agent`'s cache, then retry.

    `key path 'a.b' not found in '/path/to/secrets.yaml'`
        SOPS reference is pointing at a key that isn't there. Verify with `sops --decrypt /path/to/secrets.yaml | yq` (or `jq` for JSON files). The error reports the user-facing dot path you wrote, not the internal bracket form.

    `dodot up` says nothing about secrets but a `{{ secret(...) }}` call doesn't render
        Check that `[secret] enabled = true` is set AND the matching `[secret.providers.<scheme>]` block is enabled. Without either, `secret()` raises a render-time error pointing you at the right config block.
