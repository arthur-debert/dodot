Design Specification: Secret Handling

    :: note ::
        **Status: implemented and shipped.** Phases S1–S5 of this proposal landed in PRs dodot#128 (S1: SecretProvider trait, SecretString, pass + op providers, secret() MiniJinja function, sidecar generation, probe-based preflight UX), #129 (S2: bw + sops providers, within-run cache, reverse-merge sidecar mask via burgertocow 0.4), #130 (S3: age + gpg whole-file Opaque preprocessors, deploy_mode 0o600 enforcement), #131 (S4: keychain + secret-tool OS-level providers), and #132 (S5: dodot secret probe + dodot secret list + transform status sidecar surfacing). The user-facing guide lives in [./../../user/secrets.lex]; the developer guide lives in [./../../dev/secret.lex]. This proposal is preserved as historical design context — *not* a maintained spec. Where this document and the user/dev docs disagree about behavior, those are authoritative; where they disagree with the source, the source wins. See "Implementation Notes vs. Spec" at the bottom for the deviations and deferrals accepted during implementation.

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

        Sidecar path (next to the baseline):
            ~/.cache/dodot/preprocessor/{pack}/preprocessed/{filename}.secret.json

        Note the hard-coded `preprocessed` segment matches the on-disk shape that shipped, see preprocessing-pipeline.lex §11.2. The `{handler}` token in the original sketch was never realised; one segment now serves every downstream handler.

        Sidecar content:
            {
                "version": 1,
                "secret_line_ranges": [
                    { "start": 3, "end": 3, "reference": "op://Personal/DB/password" },
                    { "start": 11, "end": 14, "reference": "sops:secrets.yaml#tls.cert" }
                ]
            }

        The `reference` field is the original `secret()` argument string — needed by §7.4's dry-run placeholder rendering, which uses the sidecar (not a re-render with stub providers) to mask resolved values.

        Mask mechanism. Reverse-merge skip is implemented in burgertocow (the reverse-diff engine), not in dodot. burgertocow gains a `mask_deployed_lines: &[Range<usize>]` parameter; deployed-line indices that fall in any masked range are treated as already-matching the cached render, regardless of actual content. dodot reads the sidecar at reverse-merge time and passes the ranges through. See [burgertocow#13](https://github.com/arthur-debert/burgertocow/issues/13) for the API contract and the corner cases (out-of-bounds clamping, empty-mask byte-equivalence to the legacy entry point, conflict-block straddling).

        The baseline file itself (the `.json` with `rendered_content`) is unchanged from the template-expansion spec: it stores the full rendered output including resolved secret values. This is defensible because the baseline lives in the same directory hierarchy as the rendered datastore file, under the same permissions. The plaintext secret is already on disk by the time it is also in the baseline — adding a second copy in the same security boundary does not meaningfully expand the attack surface.

        Divergence semantics:
            - Line inside a secret range: never diffed, never surfaced as a conflict (burgertocow mask handles this)
            - Line outside secret ranges: normal static/dynamic classification from template-expansion applies
            - `dodot transform status` reports "deployed, modified (N lines, M secret lines skipped)"

        This preserves reverse-merge's value for the common case where a handful of secret fields sit in a mostly-static config.

        No baseline migration. The sidecar is a net-new file. The existing baseline schema (`rendered_hash`, `source_hash`, `context_hash`, `rendered_content`) is unchanged. Pre-secrets-shipping baselines simply have no sidecar; the absence is treated as "no secret ranges to skip" (empty mask, byte-identical to current reverse-merge behavior). Nothing on disk needs to be rewritten on upgrade. This guardrail exists because the magic-track shipping experience (PR #118 → #122 walk-back) demonstrated that bundling a baseline-format migration with a feature change is the most reliable way to grow a 22-pass review.

    3.4. Multi-Line Secret Values — Explicitly Refused

        Value injection (`{{ secret(...) }}`) is single-line only. At resolution time, if a `secret()` call returns a value containing a newline, the template preprocessor errors out:

            error: secret `op://Personal/TLS/cert` resolved to a multi-line value
                   value-injection (`{{ secret(...) }}`) is single-line only.
                   For multi-line secret material (TLS / SSH keys, GPG armored keys,
                   service-account JSON files), use the whole-file deploy path:
                       1. encrypt the file (e.g. `age -e -r <recipient> -o cert.pem.age cert.pem`)
                       2. drop the encrypted blob into a pack
                       3. reference the deployed path from your config:
                              tls.cert_path = "{{ dodot.home }}/.config/myapp/cert.pem"
                   See §4 for the whole-file deploy spec.

        Why we refuse rather than support:

        - **The multi-line use case is whole-file, not value-injection.** SSH keys, TLS keys, GPG keys, and service-account JSON files are *secret end-to-end* — the entire bytestream is the secret, not a few fields embedded in a mostly-static config. §1.2 already explains why those belong on the whole-file path (§4): no static fields to preserve, no value-level abstraction worth introducing. The remaining "multi-line embedded in a templated config" cases (CA bundles in envoy/k8s configs, PEM blocks as YAML string fields) are a long tail and are equally well served by deploy-as-file + reference-by-path.

        - **The failure mode of mishandling is qualitatively worse than UX friction.** If the sidecar's range tracking misses lines beyond the first — a real risk in any multi-line implementation, especially across template-engine versions — burgertocow's mask doesn't cover them. A user-side edit on those lines surfaces as a reverse-merge that rewrites the template with the literal cert/key bytes. That leaks the secret into git: the exact threat model dodot exists to mitigate (§2.2). The single-line refusal turns this from a silent corruption risk into a loud, recoverable error at render time.

        - **Single-line + whole-file is a complete cover.** Every multi-line scenario the value-injection path was claiming is reachable via §4 + a path reference in the config. The whole-file path already enforces mode 0600, already integrates with the §6.4 divergence guard, and already has a clear edit story (decrypt → edit → re-encrypt → commit). Carrying a parallel multi-line story in §3 doubles the surface for a use case that's already covered.

        Sidecar consequence: each `secret_line_ranges` entry has `end == start` (one line per `secret()` call). The `end` field is preserved in the schema for future-proofing but the renderer never produces a wider range. A sidecar containing `end > start` is a bug, not an upgraded capability.

        This refusal is the spec's permanent position, not a phase boundary. Future "support multi-line secrets" requests should be redirected at the whole-file path; the spec rejected the parallel implementation deliberately.

    3.5. Sentinel Hashing

        For templates that route to the install or homebrew handler, the sentinel must reflect resolved secret values so that rotation triggers a re-run.

        The shipped install / homebrew sentinel format is `{filename}-{8-byte SHA-256 of rendered_bytes}` (see `crates/dodot-lib/src/handlers/install.rs::file_checksum`). Because the rendered bytes already include resolved secret values, secret rotation flows through naturally: rotated value → different rendered bytes → different sentinel → script re-runs. No separate `secret_hash` component is required.

        The original sketch (`hash(template_content + context_hash + secret_hash)`) was more elaborate than necessary — the three-component formula would only matter if we wanted to detect rotation *without rendering*, which is incompatible with §7.4's "passive commands MUST NOT trigger template evaluation" anyway. Active mode renders before hashing; the rendered-bytes hash is sufficient. The sentinel never reveals the value.

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

    4.4. Divergence and the Edit Loop

        Opaque transform semantics apply (pipeline spec, section 2.3):
            - Divergence is reported (the deployed file was modified)
            - No reverse path is attempted
            - The user updates the source manually: decrypt, edit, re-encrypt, commit

        With the §6.4 divergence guard now shipped (issue dodot#110, slim base via dodot#122), `dodot up` will not overwrite an edited deployed plaintext file — the user's edit is preserved and a warning surfaces. Recovery for whole-file secrets remains the manual decrypt/edit/re-encrypt loop; there is no auto-merge.

    4.5. dodot Will Not Wrap the Edit Loop — Explicitly Refused

        dodot does not own encryption, and it will not wrap the write direction either. There is no `dodot secret edit` command, and there will not be one. Future "ergonomics wrappers around gpg/age editing" requests should be redirected at the user's existing tooling.

        Why we refuse rather than ship the wrapper:

        - **Internal contract.** dodot's threat model (§2.4) explicitly disclaims runtime-security responsibility: *"primarily a supply-chain control... only incidentally a runtime-security control."* A safe edit wrapper IS a runtime-security feature in the most exacting sense — its job is keeping plaintext off persistent storage during an interactive editor session. Putting it in dodot would push us into territory the spec says we don't own.

        - **Cross-platform secure ephemeral storage is genuinely hard.** A correct implementation needs at least three platform-specific paths: tmpfs / `/dev/shm` on Linux (with `$EDITOR` swap concerns); APFS data volumes or mode-0700 tempfiles with `unlink`-on-exit on macOS (no native tmpfs in `/tmp`); WSL bridging. None of these survive `SIGKILL` / power loss / panic — `unlink`-on-exit is a polite suggestion, not a guarantee. The plaintext landing on persistent storage during a crash is the exact failure mode dodot has no business introducing into the most security-critical step of the user's workflow.

        - **The wrapper doesn't make expert workflows easier.** Users who reach for `age` or `gpg` already have established loops:

              gpg --decrypt foo.gpg > /tmp/foo
              $EDITOR /tmp/foo
              gpg -e -r me /tmp/foo > foo.gpg.new && mv foo.gpg.new foo.gpg
              shred -u /tmp/foo

          Yes, ugly. But every step is the user's tool, the user's audit trail, and the user's failure mode. A `dodot secret edit` wrapper hides those steps behind a binary the user has to trust to be doing the right thing — at the moment they care most about getting it right.

        - **The reference doc covers the gap that matters.** What dodot can usefully provide without introducing its own edit machinery is documentation: a reference-doc section walking through the recommended decrypt/edit/re-encrypt loop with notes on `$TMPDIR` placement, `memlock` / `mlockall`, and `shred`. The user's tooling stays canonical; dodot's role is to point at the right pattern, not to reimplement it in our process.

        This refusal is the spec's permanent position. The `dodot secret list` and `dodot secret probe` commands (Phase S5) are pure read-side ergonomics and do not introduce a write path; they are not a slippery slope toward an edit wrapper.

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

        :: note ::
            **The Active / Passive contract shipped ahead of this feature.** PR arthur-debert/dodot#126 (Wave 4 of the magic-track polish, closing #121) introduced `PreprocessMode::{Active, Passive}` and threaded it through `preprocess_pack` and `plan_pack`. `dodot status` and `dodot up --dry-run` are now Passive; `dodot up` (real runs) is Active. The §7.4 contract below is no longer aspirational — it is the contract every preprocessing call site already honors. The `secret()` function MUST inspect the mode and refuse to invoke providers in Passive (returning the §7.4 placeholder instead).

        Evaluating `secret()` templates forces provider invocations, which can trigger interactive prompts (Touch ID, 1Password modal, gpg-agent passphrase). If passive commands like `dodot status` evaluate templates on every invocation, users suffer severe auth fatigue.

        dodot distinguishes between execution envelopes:

        - *Active (`dodot up`)*: Pre-walks each template's AST to collect all `secret()` references before rendering, batches calls per provider, and submits them together. The user authenticates once per run; resolved values are held in-memory for the remainder of the run. MiniJinja's introspection API (already relied on by the reverse-merge heuristics in template-expansion) makes this pre-walk cheap.
        - *Passive (`dodot status`, `dodot up --dry-run`)*: Must NOT trigger template evaluation. All drift detection uses the baseline cache, which carries `rendered_hash`, `source_hash`, and `context_hash`. That is enough to detect local drift on the deployed file, source/template changes, and non-secret context changes — fully offline, no provider calls. Implemented via `PreprocessMode::Passive` reading from `baseline.rendered_content` instead of running `preprocessor.expand()`.

        What passive cannot detect without provider calls is *upstream rotation*: the vault value changed but nothing else did. Users who want this check opt in explicitly:

            dodot status --refresh-secrets

        Dry-run placeholder rendering. For `dodot up --dry-run`, the preview shows the cached baseline content (which has resolved secret values from the last `up`). To avoid disclosing those values in the preview output, dodot reads the sidecar (§3.3) and replaces each line range with `[SECRET: <reference>]` before display — for example `[SECRET: op://Personal/DB/password]`. This is sidecar-driven masking; no template evaluation, no provider calls, no second round-trip. The reference is visible so the user can tell *which* secret would resolve, but the value is not.

        The placeholder mechanism intentionally does NOT round-trip through the template engine with stub providers. That alternative would put template evaluation in the Passive path — exactly the §7.4 violation we just shipped a fix for.

8. Implementation Phases

    Testing strategy for every phase below — unit / tier-1 hermetic / tier-2 stub-or-real / dev-shell helper for AI-agent and human exploration — lives in the sibling document [./secrets-testing.lex]. Each phase's PR is expected to land its corresponding tests; the testing doc is the contract those tests satisfy.

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
        - First-use onboarding hint (detect first `secret()` call, prompt provider install/auth). This plugs into the existing post-`up` install ladder shipped in PR arthur-debert/dodot#125 (`magic.install_ladder` — see `magic.lex` §6.6 and `crates/dodot-cli/src/handlers.rs::maybe_prompt_install_ladder`). A new rung "secret provider X needed" surfaces on the same Y/n the user already answers for hook / plist filter / template filter; component-level dismissal goes via a per-provider catalog key (`secret.provider.op`, etc.) so an opt-out on one provider doesn't suppress prompts for other unrelated rungs.
        - `dodot secret list` — enumerate references across the repo
        - `dodot secret probe` — validate all configured providers
        - `dodot transform status` enhancements for secret-bearing files

9. Implementation Notes vs. Spec

    The shipped feature follows the spec closely, but a handful of deliberate departures and deferrals were accepted during implementation. Listed here so future readers don't have to diff source against spec to figure out where the two diverge.

    9.1. Deferred to follow-up

        - **First-use install-ladder rung (§S5 first bullet).** Shipped: `dodot secret probe` and `dodot secret list` as the discovery surface — they tell the user which providers are needed and which aren't enabled. Deferred: the auto-prompt rung that nudges the user during `dodot up`. Reason: the existing `LadderRung` struct uses `&'static str` for `component_key`, but per-provider rungs need a dynamic `secret.provider.<scheme>` key — a structural refactor. The "Yes" action is also a different shape than the existing rungs (write `[secret.providers.X] enabled = true` to `.dodot.toml`, not write a hook file). Reviewable in isolation as a separate PR.

        - **Per-provider batching + AST pre-walk (§5.3 last paragraph, §7.4 "Active... Pre-walks each template's AST to collect all `secret()` references before rendering, batches calls per provider").** Shipped: within-run cache only — a reference resolved through one `secret()` call is reused by every other call to the same reference within the same `dodot up`. Deferred: the AST pre-walk that gathers references upfront and submits batched calls to providers that support them (op, sops). The cache covers the user-visible §7.4 contract ("user authenticates once per run") because tools like `op` cache their auth between subprocess invocations. Adding batched-call support to the trait and providers is a measurable perf win for repos with 20+ secret references; until that exists, serial subprocesses with cache hits is the shipped behavior.

        - **`dodot status --refresh-secrets` flag (§7.4).** The spec proposes an opt-in flag that probes upstream rotation (a vault value changed but nothing else did). Not shipped. The everyday workflow uses `dodot up` to re-resolve; users who want a non-mutating "did anything rotate?" check can run `dodot secret probe` to verify provider state and inspect their vault directly.

        - **Dry-run `[SECRET: <reference>]` placeholder rendering (§7.4).** Doc strings in the source mention this placeholder shape, but no code emits it: `dodot up --dry-run` honors the Passive contract by reading from the baseline cache (which already contains resolved values from the last `up`), and serves the bytes verbatim. The sidecar's line ranges are written but only consumed at reverse-merge time (where they correctly mask rotation from `transform check`). A future "preview with masked secrets" command can layer on top of the sidecar without changing what the pipeline writes.

        - **AST pre-walk tier-0 tests (testing-spec §3.1).** The testing spec calls out "AST pre-walk: produces the expected reference set for a given template" and "Batching: same-provider references collapse into one provider call per `dodot up`" as tier-0 coverage. Those tests don't exist because the AST pre-walk feature itself isn't shipped (above bullet). Will land alongside the batching work.

        - **Cross-pack within-run cache sharing.** Within-run caching ships per-pack today: `default_registry` (`crates/dodot-lib/src/preprocessing/mod.rs::default_registry`) constructs a fresh `SecretRegistry` for each pack the pipeline visits, so a reference resolved in pack A doesn't cache-hit when pack B references the same secret. The cache architecture (`Arc<Mutex<HashMap>>`) was chosen to make this refactor cheap — `commands::up` would build the registry once at preflight and thread the same `Arc<SecretRegistry>` into every per-pack `default_registry` call instead of letting it rebuild. Per-pack scope is fine for the common case (one pack with secrets, or duplicate refs inside one pack); cross-pack sharing matters when many packs reference the same secret and would benefit from one shared cache. Not a correctness issue, a perf/UX optimization.

    9.2. Deviations from the planned UX

        - **`secret-tool` config key is `secret_tool` (underscore), not `secret-tool` (hyphen).** Confique's `Config` derive maps each TOML key 1:1 to a Rust field name, and Rust identifiers can't contain hyphens. The scheme prefix in `secret(...)` references stays hyphenated (matching the binary name `secret-tool` and the spec); only the TOML key uses the underscore. A `scheme_to_config_key` helper translates at user-facing error-message edges so a "no provider for scheme `secret-tool`" hint suggests the correct `[secret.providers.secret_tool]` block. Code: `crates/dodot-lib/src/secret/registry.rs::scheme_to_config_key`.

        - **`[secret]` is root-only.** Spec §6.2 places `[secret]` under the standard 3-layer config hierarchy (compiled defaults < root `.dodot.toml` < pack `.dodot.toml`). Shipped: root-only — `default_registry`, `commands::up`, and `commands::status` all read from `root_config().secret`, never `config_for_pack().secret`. Reason: secret tooling is a property of the user's environment (`$PASSWORD_STORE_DIR`, `$OP_SERVICE_ACCOUNT_TOKEN`, the binaries themselves), not of any individual pack — a per-pack override would invalidate the once-per-run preflight contract (§5.4) and surface as confusing "secret X probed under config A but resolved under config B" failures. Documented at `crates/dodot-lib/src/config/mod.rs::SecretSection`.

        - **`cache_within_run` is hard-coded behavior, not a config field.** Spec §6.1 example shows `cache_within_run = true` as a tunable. Shipped: every `secret()` call goes through `cache_get`/`cache_put` unconditionally. There's no realistic reason to disable the cache in production; the tier-0 tests have a `clear_cache()` for re-exercising the provider path.

        - **`age` two-argument value-injection form silently dropped.** Spec §3.1 advertises `{{ secret("age", "secrets.age", "db_password") }}`. Not shipped — and §1.2 already justifies why (raw age is a blob cipher, not a key-value store; value-level injection against age belongs to SOPS). The `age` provider exists only as a whole-file preprocessor (§4); no `age:` scheme is registered for value injection. Spec §3.1 would benefit from a one-line "see §1.2; raw age is whole-file only" note, but the user-facing guide already steers users to `sops:` for KV-shaped age payloads.

        - **`ProbeResult` has more variants than §5.4 documents.** Spec §5.4 names three outcomes (NotInstalled, NotAuthenticated, ReferenceNotFound). Shipped: NotInstalled, NotAuthenticated, Misconfigured, ProbeFailed (plus Ok). ReferenceNotFound was folded into resolve-time errors instead of probe-time — provider probes don't have a known reference to look up, so "is this reference reachable?" can't answer at probe time. Misconfigured / ProbeFailed are additive defenses for "the binary is here but not behaving" cases the spec didn't enumerate.

        - **Provider-level reference shapes for bw / keychain / secret-tool.** Spec §5.2 left these as TBD ("later phases"). Shipped:
            - `bw:<item>[#<field>]` where `<field>` ∈ {password (default), username, notes, totp, uri}
            - `keychain:<service>[/<account>]`
            - `secret-tool:<service>[/<account>]`

    9.3. Decisions that match the spec but are worth pinning

        - **Multi-line refusal (§3.4)** ships exactly as written — `secret()` resolutions containing `\n` raise a render error pointing at the whole-file deploy path. Pinned by tier-0 tests.
        - **`dodot secret edit` permanently refused (§4.5)** — no such command exists, no plans to add one.
        - **`PreprocessMode::Passive` contract (§7.4)** ships — `dodot status` and `dodot up --dry-run` never invoke `resolve()`. Pinned with a `PanickingProvider` test double whose `resolve()` panics; Passive flows complete without firing it.
        - **Mode 0600 enforcement (§4.3)** ships via `Fs::write_file_with_mode` and `DataStore::write_rendered_file_with_mode` — the rendered datastore file is created at 0600 atomically (no race window between umask-default write and chmod).
