Design Specification: Testing Strategy for Secret Handling

    :: note ::
        **Status: implemented and shipped alongside [./secrets.lex].** Tier 0 (unit tests with `MockSecretProvider`) and Tier 1 (hermetic real-binary tests in `tests/e2e/bats/test_secrets_*.bats` for `pass`, `age`, `gpg`) shipped across PRs dodot#128–#132. Tier 2 (op / bw stub binaries on PATH for default CI; opt-in real-provider workflow) shipped its stub side via `tests/e2e/bats/helpers/secrets_stubs.bash`; the real-provider workflow is deferred. Tier-0 tests for AST pre-walk and per-provider batching are deferred — those features didn't ship in S2 (see [./secrets.lex] §9.1). The macOS / Linux dedicated CI runners for `keychain` and `secret-tool` per §5.3 are also deferred — those providers ship with tier-0 unit tests only; see [./secrets.lex] §9.1 for the rationale (real OS keystores aren't safe to write from automated tests without infra). This proposal is preserved as historical design context — *not* a maintained spec. The user-facing testing notes live alongside the providers in [./../../dev/secret.lex] §6.

    This document is the testing companion to [./secrets.lex]. It specifies how the secrets work is unit-tested, how integrations with external tooling (provider CLIs, gpg, age, sops) are exercised, and how the same tests run both locally and in CI without per-developer setup. Read [./secrets.lex] first for the feature design; this document only covers verification.

1. Why a Sibling Spec

    The secrets work is unusually integration-heavy. Above the dodot/burgertocow seam everything is rust-native and unit-testable. Below it, every supported provider is a subprocess against a tool dodot does not own (`gpg`, `age`, `sops`, `op`, `bw`, `pass`, `secret-tool`, `security`). Glue code is the dominant failure surface: command syntax drift, output-parsing changes between tool versions, error-mapping mismatches, auth-state assumptions that hold on the developer's box but not in CI.

    Testing for this kind of glue requires a different discipline from "unit-test the function." Bundling the strategy into [./secrets.lex] would either bloat that document or hide the integration discipline behind feature design. The sibling shape lets each document stay focused: secrets.lex defines what we ship; this document defines how we know it works.

2. The Testing Seam

    A single trait separates "dodot logic" from "what subprocess we shell out to":

        trait SecretProvider {
            fn scheme(&self)  -> &str;
            fn resolve(&self, reference: &str) -> Result<SecretString>;
            fn probe(&self) -> ProbeResult;
        }

    Above the trait — *everything* the tests of section 3 cover — is pure dodot:
        - Reference parsing per scheme
        - The `secret()` MiniJinja function and its caching
        - AST pre-walk for batched provider calls
        - Sidecar serialisation
        - Sentinel construction
        - `PreprocessMode::Active` / `Passive` gating (see secrets.lex §7.4)
        - Error UX (mapping `probe()` results to user-facing messages)
        - Multi-line refusal at render time (secrets.lex §3.4)

    Below the trait — what section 4 covers — is the per-provider subprocess work: command shape, env-var handling, output parsing, exit-code mapping.

    This is the same shape as our existing `Fs` and `CommandRunner` traits in dodot, and it carries the same testability discipline: above-trait code is unit-tested with a mock; below-trait code is integration-tested with the real binary.

3. Tier 0 — Unit Tests (default `cargo test`)

    Pure dodot logic. Runs on every PR. No external binaries. No environment dependencies. The mock provider used here is `MockSecretProvider`, a test-only impl that returns canned values from a `HashMap<reference, value>` and counts invocations.

    3.1. Coverage

        - Reference parsing: `op://Vault/Item/Field`, `pass:path/to/x`, `sops:f.yaml#k.path`, two-arg form for raw age.
        - `SecretString` zero-on-drop, refuses Debug / Display formatting.
        - Sidecar serialise / deserialise round-trip; missing-sidecar treated as "no secret ranges to skip" (empty mask).
        - The `secret()` MiniJinja function: dispatches to the right mock by scheme, returns the mocked value, caches within a single render pass (each unique reference resolves once even if used N times).
        - AST pre-walk: produces the expected reference set for a given template.
        - Batching: same-provider references collapse into one provider call per `dodot up`.
        - Mode gating: `secret()` invoked under `PreprocessMode::Passive` returns the §7.4 `[SECRET: <reference>]` placeholder and never invokes a provider. Pin this with a `PanickingProvider` mock that aborts on `resolve()`; status / dry-run runs against a templated pack must not panic. Same shape as the existing tests for `up_dry_run_does_not_write_to_datastore` (commands/tests.rs).
        - Multi-line refusal: a mock returning `"a\nb"` produces the documented render-time error (secrets.lex §3.4); whole-file path is suggested in the error body.
        - Sentinel: rendered-bytes hash changes when the mock returns a different value across runs; secret rotation flows through.
        - Error UX: each `probe()` outcome (`NotInstalled`, `NotAuthenticated`, `ReferenceNotFound`) maps to the documented user-facing message in secrets.lex §5.4.

    3.2. Mock Discipline

        `MockSecretProvider` is the only mock allowed in tier 0. Tests must NOT shell out, must NOT touch the network, must NOT depend on `$HOME` contents. A tier-0 test that needs a real provider belongs in tier 1.

4. Tier 1 — Self-Contained Integration (default CI)

    Real provider impls + real binaries + hermetic per-test fixtures. Runs on every PR on the Linux CI runner. No accounts, no network, no developer-machine config bleed-through.

    4.1. Hermetic Fixtures

        Each test gets a sandbox with isolated environment:
            $SANDBOX/gnupg                -> $GNUPGHOME
            $SANDBOX/password-store       -> $PASSWORD_STORE_DIR
            $SANDBOX/sops-keys/age.txt    -> $SOPS_AGE_KEY_FILE
            $SANDBOX/dotfiles             -> $DOTFILES_ROOT
            $SANDBOX/data                 -> $XDG_DATA_HOME
            $SANDBOX/cache                -> $XDG_CACHE_HOME
            $SANDBOX/config               -> $XDG_CONFIG_HOME

        Fixture setup runs in `setup()`:
            - Generate a no-passphrase test gpg keypair via `gpg --batch --gen-key` with a deterministic UID like `dodot-test@example.invalid`.
            - Initialise pass: `pass init <key-id>`; insert known fixture entries.
            - Generate an age keypair via `age-keygen`; write to `$SOPS_AGE_KEY_FILE`.
            - Encrypt a fixture YAML with sops to exercise the sops provider.

        Fixture teardown runs in `teardown()`: `rm -rf $SANDBOX`. No state survives between tests.

        Helpers live in `tests/e2e/bats/helpers/secrets_fixtures.bash`. The brew muzzle pattern from PR dodot#120 is the precedent: reusable bash functions, sourced from each `.bats` file's `setup()`.

    4.2. Provider Coverage

        | Provider              | Binary                  | Hermetic? | What gets tested                                                                                    |
        | pass                  | `pass`, `gpg`           | Yes       | Reference resolution, missing-entry error, gpg-agent passphrase-less unlock                         |
        | sops                  | `sops`, `age`           | Yes       | YAML key extraction (`sops:file.yaml#path.to.key`), missing-key error, decrypt failure              |
        | age (whole-file)      | `age`                   | Yes       | Decrypt, mode 0600 enforcement, plaintext-passes-through-symlink                                    |
        | gpg (whole-file)      | `gpg`                   | Yes       | Same shape as age, different cipher; uses the shared test keypair                                   |
        :: table ::

        These four providers cover the full secrets.lex story for value-injection (pass, sops) and whole-file deploy (age, gpg). They are the secrets work's primary regression net.

    4.3. End-to-End Tests

        Beyond per-provider unit-style tests, three full-pipeline `bats` files exercise dodot from the user's perspective:

            test_secrets_value_injection.bats   - dodot up renders a templated config with `{{ secret("pass:...") }}` and `{{ secret("sops:...") }}`, deploys via symlink, asserts the rendered file contains the resolved value.
            test_secrets_wholefile.bats         - dodot up deploys ssh/id_ed25519.age and a Brewfile.gpg, asserts symlinks land at the right paths with mode 0600.
            test_secrets_passive_contract.bats  - status + up --dry-run against a templated pack with the panicking-provider crate-level test fixture (see §6.1) — no provider invoked, no datastore writes.

        These run with the real provider binaries against fixture data. Pin the §7.4 contract end-to-end in addition to the trait-level tests in tier 0.

    4.4. CI Setup

        On the default Linux runner, tier 1 needs four binaries: `gpg`, `pass`, `age`, `sops`. Two acceptable install paths:

            (a) `apt install gnupg pass age` + manual `sops` from a release tarball (sops isn't in Debian stable). One-liner in the workflow.
            (b) `mise install` against a pinned `.tool-versions` file. Survives ubuntu-image bumps better; costs ~30s of CI per run.

        Recommendation: (b). The version-pinning matters because tier 1 catches output-parsing drift, and an ubuntu-image bump that quietly rotates one of the binaries could surface as a flaky test rather than as the upgrade signal it is. (a) is the fallback if `mise` infra is more friction than the version-pin is worth.

5. Tier 2 — Service-Backed Integration

    Real provider CLIs that need an account or system service.

    5.1. Stub-First Default Coverage

        For `op` and `bw` — the two cloud-vault providers — the default CI runs against a *stub binary* placed on PATH:

            tests/fixtures/stubs/op
            tests/fixtures/stubs/bw

        Each stub is a small bash script that accepts the subset of the real CLI's command shape dodot uses, reads from a fixture file under `$SANDBOX/stub-vault/`, and exits with the same codes the real binary would (0 for success, distinct non-zero codes for "not authenticated", "ref not found", etc.).

        `tests/e2e/bats/helpers/secrets_stubs.bash` prepends `tests/fixtures/stubs` to PATH for tier-2 tests. The same pattern shipped in PR dodot#120 (the brew muzzle); it's known to work.

        What this gets us:
            - Coverage of dodot's op / bw provider implementations against a representative command shape on every PR.
            - No account dependency, no flakiness from cloud-service availability.
            - When the real binary's CLI shape drifts (a major version bumps a flag), the stub model goes stale — caught by §5.2's opt-in workflow.

    5.2. Real-Provider Opt-In Workflow

        A separate workflow (`.github/workflows/secrets-real-providers.yml`) runs the same `.bats` files with the stubs *removed* from PATH and the real CLIs in their place, authenticated against fixture vaults via:
            - `op`: `OP_SERVICE_ACCOUNT_TOKEN` (GitHub repo secret, scoped to a dedicated test vault)
            - `bw`: `BW_CLIENT_ID` + `BW_CLIENT_SECRET` (API-key auth, dedicated test vault)

        Trigger: weekly cron + manual `workflow_dispatch`. Not on every PR — keeps PR latency down and avoids burning provider rate limits on every push.

        The same assertions run against both stub and real-binary modes. A test that passes against the stub but fails against the real binary tells us the stub model is stale; a test that passes against the real binary but fails against the stub tells us the stub is over-permissive. Both are valuable failure shapes.

    5.3. OS-Level Providers (S4)

        macOS Keychain (via `security`) and the freedesktop Secret Service (via `secret-tool` against `dbus-daemon` + `gnome-keyring` or `keepassxc`) are deferred to Phase S4 of secrets.lex. When that phase ships:
            - Keychain: dedicated macOS CI runner, since the `security` command isn't usefully mockable across platforms. Manual interactive prompts can't run hermetically; tests gate on a pre-unlocked keychain set up by the workflow.
            - Secret Service: dedicated Linux CI job that brings up `dbus-daemon` + a headless secret backend. Self-contained but heavier than tier 1.

        Until S4, neither provider has a test target. Mark the providers as "manual testing only" in their probe output.

6. Test Doubles

    6.1. `MockSecretProvider`

        Test-only `SecretProvider` impl. Exposed under `#[cfg(test)]`. Constructor takes a `HashMap<String, String>` of `reference -> value` pairs and an optional `Vec<String>` of "panic on these references" entries. Tracks invocation count for batching assertions. Lives in `crates/dodot-lib/src/secret/test_support.rs` alongside the real registry.

    6.2. `PanickingProvider`

        A `SecretProvider` whose `resolve()` calls `panic!()`. Used to pin the §7.4 Passive contract: a status / dry-run flow that touches a templated pack with this provider registered must complete without panicking — proves no provider call happened.

    6.3. Stub Binaries

        Bash scripts under `tests/fixtures/stubs/`. Self-contained, reviewable as plain text. Each begins with a documentation block listing the subset of CLI shape it covers and the corresponding real-binary version it was modelled against. When a stub is updated to track a new real-binary version, the doc block is updated in the same commit. Easy review surface.

7. AI Agent / Local Developer Workflow

    Implementation work on this track will be split across long sessions, much of it driven by an AI coding agent. The agent needs to be able to drop into a sandbox with all fixtures initialised and explore against the real toolchain — running `dodot up`, `pass show`, `sops --decrypt`, etc., to debug behaviour. Bats's "set up sandbox per test" model is the right primitive but isn't directly invocable outside a test run.

    7.1. dev-shell.sh

        A new helper `tests/e2e/bats/helpers/dev-shell.sh <fixture-name>` reuses the same setup functions a `.bats` file would source, then drops the user / agent into an interactive subshell with all the env vars set:

            $ ./tests/e2e/bats/helpers/dev-shell.sh secrets-pass
            [sandbox: /tmp/dodot-dev-XXX]
            [gpg key id: <auto-generated>]
            [pass entries seeded: db_password, github_token]
            $ pass show test-secrets/db_password
            hunter2-from-fixture
            $ dodot up
            ...

        Available fixtures:
            secrets-pass         pass + gpg, no passphrase, fixture entries
            secrets-sops         sops + age, encrypted YAML
            secrets-age          age whole-file, fixture encrypted file
            secrets-gpg          gpg whole-file, fixture encrypted file
            secrets-op-stub      op stub binary on PATH, fixture vault
            secrets-bw-stub      bw stub binary on PATH, fixture vault
            secrets-op-real      requires OP_SERVICE_ACCOUNT_TOKEN; uses real op CLI
            secrets-bw-real      requires BW_CLIENT_ID + BW_CLIENT_SECRET

        Cleanup runs on subshell exit (`trap "rm -rf $SANDBOX" EXIT`). The agent never accumulates state.

8. Decisions and Open Questions

    Decided up-front in this document:
        - SecretProvider trait is the single mock seam.
        - Tier 0 uses MockSecretProvider only — no real binaries.
        - Tier 1 covers pass / sops / age / gpg with real binaries on default CI.
        - Tier 2 (op / bw) ships stubs by default + opt-in real-provider workflow.
        - Linux is the default CI substrate; macOS / dbus jobs land with their corresponding S4 phase.

    Decisions still to make at implementation time:
        - Pin tool versions via `mise` (recommended) or `apt` direct (fallback).
        - Real-provider workflow trigger: weekly cron + workflow_dispatch (recommended) vs. nightly cron (more flake noise) vs. manual-only (fewer signals from drift).
        - Stub binary model fidelity: how much of `op` / `bw` do we actually reproduce? Start with the read paths dodot uses (`op read`, `bw get item`); add commands as the implementation needs them.

9. Cross-References

    - [./secrets.lex] §5.1 — `SecretProvider` trait surface.
    - [./secrets.lex] §7.4 — Passive contract; tier-0 mode-gating tests pin it.
    - [./secrets.lex] §3.4 — Multi-line refusal; tier-0 test asserts the documented error.
    - [./secrets.lex] §4.5 — Refusal of `dodot secret edit`; no test for this since the feature is permanently out of scope.
    - dodot#120 (merged) — brew muzzle pattern; precedent for stub binaries on PATH in bats setup.
    - dodot#126 (merged) — `PreprocessMode::{Active, Passive}`; the runtime contract tier-0 / tier-1 tests pin.
    - burgertocow#13 — the masking API; integration tests for the mask path live in tier 1 once burgertocow v0.4.0 lands and the dodot side wires it up.
