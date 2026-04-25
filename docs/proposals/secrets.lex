Design Specification: Secret Handling

    This document specifies how dodot handles sensitive data — API tokens, credentials, private keys — so that the version-controlled dotfiles repository contains no plaintext secrets while the rendered configuration on disk remains functional.

    It builds on the Preprocessing Pipeline [./preprocessing-pipeline.lex] and extends the Template Expansion [./template-expansion.lex] preprocessor with a pluggable secret-provider layer. Secret handling is not a single feature: it is two related but distinct patterns sharing one pipeline.

1. Overview

    1.1. Two Use Cases

        Value injection:
            A config is mostly plaintext; a handful of fields hold secrets. The template references those fields via a `secret()` function, and dodot resolves them at render time.

            Template source (in git):
                [database]
                host = "localhost"
                password = "{{ secret('op://Personal/DB/password') }}"

            Rendered (in datastore, symlinked to user location):
                [database]
                host = "localhost"
                password = "s3cret-v4lue"

            This is an extension of template expansion. The only new construct is the `secret()` function.

        Whole-file deploy:
            The artifact is secret end-to-end. The source in the repo is encrypted; the deployed file is the decrypted plaintext. No template expansion is involved.

            Flow:
                ssh/id_ed25519.age  (encrypted, in git)
                    -> decrypt -> datastore -> symlink -> ~/.ssh/id_ed25519

            This is an Opaque preprocessor in the sense of the Preprocessing Pipeline spec (section 2.3).

    1.2. Why Both Patterns

        The patterns have fundamentally different shapes and forcing either into the other creates friction.

        A config with three secret fields and forty plaintext fields does not want to be encrypted wholesale — that forfeits readable diffs for the forty safe fields. Conversely, an SSH private key does not benefit from a template abstraction: the entire bytestream is the secret, and there are no static fields to preserve.

        Additionally, raw age and GPG are blob ciphers: they encrypt bytes, not key-value stores. To support value-level injection against age/GPG, dodot would have to invent a key-value wrapper over encrypted blobs. That is SOPS's job, not dodot's. Value injection therefore uses SOPS (or pass, or a cloud vault) as its KV source; raw age/GPG is reserved for whole-file deploy.

2. Threat Model

    2.1. Scope

        dodot's secret handling defends against a specific, asymmetric threat: disclosure through source control.

        In scope:
            - Plaintext secrets committed to git, accidentally or intentionally
            - Secrets retained in git history (reflog, remote mirrors, CI logs, forks, IDE local history, vendor backups)
            - Repository scope drift: private repos becoming public, orgs being acquired, collaborator access lapsing
            - Third-party breach risk at repository hosts (GitHub, GitLab, Bitbucket)
            - Side-channel leak through backup sync when the repo is included (iCloud, Dropbox, Time Machine)

        Out of scope:
            - User-level compromise on the local machine (code execution as the user)
            - Root or kernel compromise
            - Physical access to an unlocked, logged-in machine
            - Offline attacks on a powered-off disk (mitigated by full-disk encryption, not by dodot)

    2.2. The Source-Control Asymmetry

        Plaintext on disk and plaintext in a repository look similar — "plaintext somewhere an attacker could read" — but behave very differently, and this asymmetry is the justification for the entire design.

        Disk plaintext is bounded:
            One machine, one user, one filesystem. Destroyed when the disk is wiped or the key is rotated. An attacker needs access during the window the file exists.

        Repository plaintext is unbounded:
            Git history is forever unless rewritten and force-pushed. Forks, clones, CI logs, and collaborators' reflogs retain it. Vendor backups at GitHub/GitLab/Bitbucket are opaque. A secret committed for 30 seconds and immediately reverted is still leaked — rotation is mandatory. "Private" scope drifts over time.

        The mitigation dodot provides is therefore not "keep plaintext out of files" but "keep plaintext out of a replicating, append-only, third-party-hosted log with unclear deletion semantics." That is a qualitatively different risk class than a file on a laptop.

    2.3. The Internal Boundary

        Even under user-level compromise, one barrier usually survives: the root of trust itself.

        An attacker with code execution as the user during an active session can:
            - Read rendered configs in `<state>/dodot`
            - Use unlocked agent sockets (ssh-agent, gpg-agent, 1Password session)
            - Exfiltrate any secret values currently materialized on disk

        But they generally cannot exfiltrate:
            - A 1Password master password guarded by biometric unlock
            - An age private key on a YubiKey
            - A GPG key on a smartcard
            - A vault decryption key held in a hardware module

        After detection and cleanup, individual leaked secrets can be rotated without assuming the vault itself is compromised. This bounds blast radius. dodot preserves this property by not owning root-of-trust material: it delegates to the provider (op, age, pass, SOPS, etc.), which in turn delegates to OS and hardware layers.

    2.4. Honest Framing

        dodot's secret handling is primarily a supply-chain control — keep plaintext out of repositories, backups, and sync targets. It is secondarily a key-custody control by virtue of leaning on hardware-backed roots of trust in the provider ecosystem. It is only incidentally a runtime-security control: an attacker with code execution as your user is not stopped by dodot, by any competing dotfile manager, or by most secret managers on their own.

3. Value Injection

    3.1. The `secret()` Template Function

        Inside any file processed by the template preprocessor, the `secret()` function resolves a provider reference to a string value at render time.

        URI form:
            {{ secret("op://Personal/GitHub/token") }}
            {{ secret("pass:github/token") }}
            {{ secret("sops:secrets.yaml#database.password") }}

        The argument is `<scheme>:<provider-specific-reference>`. The scheme selects the provider; the remainder is passed verbatim to that provider.

        Two-argument form:
            {{ secret("age", "secrets.age", "db_password") }}

        Used only for providers that have no natural URI form.

    3.2. Rendering and Storage

        Secret-bearing templates flow through the same pipeline as regular templates:

            1. Scan identifies a template file
            2. Template preprocessor renders it with the variable context
            3. Each `secret(...)` call dispatches to the appropriate provider
            4. Rendered output is written to the datastore as a regular file
            5. Downstream handler (symlink, shell, path, etc.) deploys as usual

        No new preprocessor is added. No new on-disk location is introduced for secrets specifically. The rendered file lives in the datastore next to non-secret rendered files, under the same permissions.

    3.3. Baseline, Sidecar, and Divergence

        Reverse-merge (Phase 2 of template expansion) applies to secret-bearing templates with one modification: lines whose rendered output came from a `secret()` call are tracked in a sidecar and skipped by the divergence diff.

        Sidecar path:
            ~/.cache/dodot/preprocessor/{pack}/{handler}/{filename}.secret.json

        Sidecar content:
            {
                "version": 1,
                "secret_line_ranges": [
                    { "start": 3, "end": 3 },
                    { "start": 11, "end": 14 }
                ]
            }

        The baseline file itself (the `.json` with `rendered_content`) is unchanged from the template-expansion spec: it stores the full rendered output including resolved secret values. This is defensible because the baseline lives in the same directory hierarchy as the rendered datastore file, under the same permissions. The plaintext secret is already on disk by the time it is also in the baseline — adding a second copy in the same security boundary does not meaningfully expand the attack surface.

        Divergence semantics:
            - Line inside a secret range: never diffed, never surfaced as a conflict
            - Line outside secret ranges: normal static/dynamic classification from template-expansion applies
            - `dodot transform status` reports "deployed, modified (N lines, M secret lines skipped)"

        This preserves reverse-merge's value for the common case where a handful of secret fields sit in a mostly-static config.

    3.4. Multi-Line Secret Values

        A single `secret()` expression may produce multiple rendered lines when the value contains newlines (e.g., a multi-line key embedded in a config). The template preprocessor records the full produced range — not just the template line — in the sidecar. Reverse-merge skips the whole range.

    3.5. Sentinel Hashing

        For templates that route to the install or homebrew handler, the sentinel must include resolved secret values in its hash (the same way it already includes the render context):

            sentinel = "{filename}-{hash(template_content + context_hash + secret_hash)}"

        Where `secret_hash` is the hash of the concatenated resolved secret values. When a secret rotates, the sentinel changes, and the script re-runs. The hash never reveals the value.

4. Whole-File Encrypted Artifacts

    4.1. Supported Formats

        Phase-1 set:
            - `.age`: age-encrypted (recipient or passphrase)
            - `.gpg`: GPG-encrypted

        Each is a distinct preprocessor implementing the Preprocessor trait from the pipeline spec. Both are Opaque transforms (no reverse path).

    4.2. Deployment Flow

        Example for age:
            1. Scan identifies `ssh/id_ed25519.age`
            2. The age preprocessor strips the suffix: expanded filename is `id_ed25519`
            3. `expand()` invokes age to decrypt the file
            4. Plaintext is written to the datastore as a regular file
            5. A virtual RuleMatch for `id_ed25519` enters the normal pipeline
            6. The symlink handler links it into `~/.ssh/id_ed25519`

        No template expansion occurs. The preprocessor is a decrypt-and-emit operation.

    4.3. File Permissions

        Rendered whole-file secrets are written with mode 0600 regardless of the source file's permissions. The preprocessor enforces this; the datastore layer does not override it.

    4.4. Divergence

        Opaque transform semantics apply (pipeline spec, section 2.3):
            - Divergence is reported (the deployed file was modified)
            - No reverse path is attempted
            - The user updates the source manually: decrypt, edit, re-encrypt, commit

        dodot does not own encryption. It delegates to age and gpg in the read direction; the write direction is the user's responsibility, outside dodot. (A `dodot secret edit` command in Phase S5 will provide a seamless wrapper over this manual loop).

5. Providers

    5.1. The Abstraction

        A provider is a small adapter that resolves a reference to a string value. The trait is intentionally minimal:

            trait SecretProvider:
                scheme()   -> &str                  ("op", "pass", "age", "sops", ...)
                resolve()  -> Result<SecretString>  (given a reference, fetch the value)
                probe()    -> ProbeResult           (is this provider installed and authenticated?)

        `probe()` enables the error UX in 5.4 — dodot can tell the user *why* a provider failed (not installed, not authenticated, session expired) rather than reporting an opaque resolution error.

        Secrets returned by providers are held in a `SecretString` wrapper that zeroes its buffer on drop and refuses Debug/Display formatting. This is defense in depth, not a guarantee — the rendered output ultimately lands on disk and inherits OS-level protections from there.

    5.2. Initial Provider Set

        Initial providers and their reference forms:

            | Scheme   | Tool           | Reference                            | Notes                                            |
            | op       | 1Password CLI  | op://Vault/Item/Field                | Native URI; biometric unlock via desktop app     |
            | pass     | password-store | pass:path/to/secret                  | First line of the entry is the password          |
            | sops     | SOPS           | sops:file.yaml#path.to.key           | Wraps age, GPG, or cloud KMS under the hood      |
            | age      | age CLI        | two-argument form                    | Raw age is whole-file only; value-level via SOPS |
        :: table ::

        Later phases add `bw` (Bitwarden), `keychain` (macOS Keychain via `security`), and `secret-tool` (freedesktop Secret Service).

    5.3. Provider Implementation Shape

        Every provider follows the same shape: parse the reference, invoke the external tool with the right arguments, read stdout, handle errors. No provider exceeds a few hundred lines of code.

        Variation between providers lives in three places, none of them architecturally significant:

            - Command syntax: different argument shapes per tool
            - Auth-state assumptions: op trusts the desktop-app biometric path; bw requires a session key in an env var; pass leans on gpg-agent; SOPS is transparent once keys are configured in `.sops.yaml`
            - Output handling: most providers emit the raw value on stdout; pass has a first-line-is-password convention; SOPS needs an explicit extraction path

        The cross-cutting concerns — batching calls per provider, caching resolved values within a single `dodot up` run, producing actionable errors — live above the provider layer and apply uniformly. Adding a new provider does not touch them.

    5.4. Error UX

        Provider failures map to actionable messages via `probe()`:

            Provider CLI not installed:
                error: secret provider `op` is not installed
                       install 1Password CLI: https://1password.com/downloads/command-line
                       or disable the provider: [secret.providers.op] enabled = false

            Not authenticated:
                error: secret provider `op` is not authenticated
                       run `op signin` and retry, or enable desktop-app integration

            Reference not found:
                error: secret `op://Personal/GitHub/token` not found
                       verify the reference: op item list --vault Personal

        Errors are produced before `resolve()` is attempted when `probe()` can determine the issue cheaply.

    5.5. Adding New Providers

        A new provider is added by implementing the trait and registering it. No change to the template preprocessor or the pipeline is required. Providers self-register via a compile-time registry keyed by scheme.

        A plugin mechanism for user-contributed providers is a future consideration, gated on whether the built-in set proves insufficient. The built-in set is deliberately small.

6. Configuration

    6.1. Schema

        The `[secret]` section is top-level, separate from `[preprocessor.*]`. Value injection uses `[secret]` directly; whole-file decryption uses `[preprocessor.age]` and `[preprocessor.gpg]` in the preprocessor tree, consistent with the pipeline spec.

        Example:
            [secret]
            enabled = true
            cache_within_run = true

            [secret.providers.op]
            enabled = true

            [secret.providers.pass]
            enabled = true
            store_dir = "~/.password-store"

            [secret.providers.sops]
            # no extra config; SOPS uses .sops.yaml in-tree

            [preprocessor.age]
            extensions = ["age"]
            identity = "~/.config/age/identity.txt"

            [preprocessor.gpg]
            extensions = ["gpg", "asc"]

    6.2. Inheritance

        `[secret]` and `[preprocessor.age|gpg]` follow the 3-layer hierarchy (compiled defaults < root `.dodot.toml` < pack `.dodot.toml`), like all dodot configuration. Per-pack overrides allow a pack to use a different provider or disable secret handling entirely.

    6.3. Kill Switches

        Global:
            [secret]
            enabled = false

        Per-provider:
            [secret.providers.op]
            enabled = false

        Per-file (via the `mappings.ignore` filter handler):
            [mappings]
            ignore = ["config.toml.tmpl"]

        When secrets are globally disabled, any `secret()` call in a rendered template produces a render-time error rather than a silent empty string.

7. Operational Notes

    7.1. Backup and Sync Hygiene

        The datastore and baseline cache contain rendered plaintext. Documentation will instruct users to:
            - Exclude `<state>/dodot` from iCloud Drive, Dropbox, Google Drive, and similar sync targets
            - Exclude it from Time Machine if strict confinement is desired
            - Rely on full-disk encryption for the at-rest case

        dodot enforces mode 0700 on the state root and mode 0600 on rendered whole-file secrets. It does not attempt to encrypt the baseline.

    7.2. Logging

        The render pipeline never logs resolved secret values. Errors reference the URI (`op://Personal/GitHub/token`) but not the value. `dodot transform status` never prints secret lines.

    7.3. Rotation

        Rotation is handled by the provider (change the value in 1Password, pass, SOPS, etc.). On the next `dodot up`, the changed value flows into the rendered output and, for install/homebrew handlers, bumps the sentinel hash to trigger re-execution.

    7.4. Auth Fatigue and Passive Commands

        Evaluating `secret()` templates forces provider invocations, which can trigger interactive prompts (Touch ID, 1Password modal, gpg-agent passphrase). If passive commands like `dodot status` evaluate templates on every invocation, users suffer severe auth fatigue.

        dodot distinguishes between execution envelopes:

        - *Active (`dodot up`)*: Pre-walks each template's AST to collect all `secret()` references before rendering, batches calls per provider, and submits them together. The user authenticates once per run; resolved values are held in-memory for the remainder of the run. MiniJinja's introspection API (already relied on by the reverse-merge heuristics in template-expansion) makes this pre-walk cheap.
        - *Passive (`dodot status`, `dodot up --dry-run`)*: Must NOT trigger template evaluation. All drift detection uses the baseline cache, which carries `rendered_hash`, `source_hash`, and `context_hash`. That is enough to detect local drift on the deployed file, source/template changes, and non-secret context changes — fully offline, no provider calls.

        What passive cannot detect without provider calls is *upstream rotation*: the vault value changed but nothing else did. Users who want this check opt in explicitly:

            dodot status --refresh-secrets

        For `dodot up --dry-run`, template previews that reference a secret render as a `[SECRET: op://...]` placeholder rather than the resolved value, preserving the no-prompt guarantee while still making the reference visible in the preview.

8. Implementation Phases

    Phase S1: Value injection — minimal set
        - `SecretProvider` trait and `SecretString` type
        - Providers: `pass`, `op`
        - `secret()` function integrated with MiniJinja
        - Sidecar generation for secret line ranges
        - `probe()`-based error UX

    Phase S2: Value injection — complete
        - Providers: `sops`, `bw`
        - Within-run caching and per-provider batching
        - Divergence integration with sidecar (skip secret ranges in reverse-merge)
        - Sentinel hash includes resolved secret values

    Phase S3: Whole-file deploy
        - `age` preprocessor (Opaque)
        - `gpg` preprocessor (Opaque)
        - Mode 0600 enforcement

    Phase S4: OS-level providers
        - `keychain` (macOS `security` command)
        - `secret-tool` (freedesktop Secret Service)

    Phase S5: Ergonomics
        - First-use onboarding hint (detect first `secret()` call, prompt provider install/auth)
        - `dodot secret list` — enumerate references across the repo
        - `dodot secret probe` — validate all configured providers
        - `dodot transform status` enhancements for secret-bearing files
        - `dodot secret edit <path>` — edits whole-file opaque secrets without the manual decrypt/edit/re-encrypt loop. Plaintext stays in an ephemeral location (tmpfs on Linux, mode-0700 tempfile with unlink-on-exit on macOS, or anonymous mmap where in-memory-only is feasible); exact mechanism is platform-dependent.
