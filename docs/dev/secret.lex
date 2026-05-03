Secrets — Developer Guide

    Contributor reference for the secrets subsystem: the `SecretProvider` trait, the `SecretRegistry` + cache, the two pipelines (value injection through MiniJinja vs. whole-file decryption through preprocessors), the sidecar that ties them to the divergence guard, and the linear code paths a `dodot up` invocation walks. For the user-facing guide, see [./../user/secrets.lex]. For the original design rationale, see [./../proposals/shipped/secrets.lex].

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Module Layout

    The secrets layer lives in two adjacent subsystems:

        crates/dodot-lib/src/
        +-- secret/                          # Value-injection layer
        |   +-- mod.rs                         # re-exports
        |   +-- provider.rs                    # SecretProvider trait + ProbeResult
        |   +-- secret_string.rs               # zero-on-drop wrapper
        |   +-- registry.rs                    # SecretRegistry + within-run cache + scheme dispatch
        |   +-- error_render.rs                # ProbeResult -> user-facing string + preflight aggregator
        |   +-- pass.rs                        # password-store provider
        |   +-- op.rs                          # 1Password CLI provider
        |   +-- bw.rs                          # Bitwarden CLI provider
        |   +-- sops.rs                        # Mozilla SOPS provider
        |   +-- keychain.rs                    # macOS Keychain provider
        |   +-- secret_tool.rs                 # freedesktop Secret Service provider
        |   +-- test_support.rs                # MockSecretProvider + PanickingProvider (#[cfg(test)] only)
        |
        +-- preprocessing/
            +-- age.rs                         # Whole-file age preprocessor (Opaque)
            +-- gpg.rs                         # Whole-file gpg preprocessor (Opaque)
            +-- template.rs                    # secret() MiniJinja function (search for `secret_registry`)
            +-- baseline.rs                    # SecretsSidecar struct + persistence
            +-- reverse_merge.rs               # burgertocow mask integration (mask_deployed_lines)
            +-- mod.rs                         # ExpandedFile.deploy_mode + .secret_line_ranges fields
            +-- pipeline.rs                    # divergence-guard gate + chmod-via-deploy_mode

        crates/dodot-lib/src/commands/
        +-- secret.rs                        # `dodot secret probe` + `dodot secret list` library entry points
        +-- transform.rs                     # `transform status` reads the sidecar (search for `secret_references`)
        +-- up.rs                            # one-time preflight call (search for `preflight`)

    :: text ::

    The split is deliberate. Value injection uses MiniJinja's `secret()` callback to inject one string per call; that's the `secret/` module. Whole-file decryption uses the preprocessing pipeline's existing trait — `age` and `gpg` are just two more `Preprocessor` impls and live next to `template.rs` / `unarchive.rs` for that reason. The two paths share the per-render `<baseline>.secret.json` sidecar, which lives in `preprocessing/baseline.rs` because the baseline cache is the natural seam for any per-render artifact.

2. The `SecretProvider` Trait

    The trait is intentionally tiny — three methods, all object-safe:

        pub enum ProbeResult {
            Ok,
            NotInstalled    { hint: String },
            NotAuthenticated { hint: String },
            Misconfigured    { hint: String },
            ProbeFailed      { details: String },
        }

        pub trait SecretProvider: Send + Sync {
            fn scheme(&self) -> &str;
            fn probe(&self) -> ProbeResult;
            fn resolve(&self, reference: &str) -> Result<SecretString>;
        }

    :: text ::

    Defined at `secret/provider.rs:24` (enum) and `secret/provider.rs:89` (trait). Each impl does three things and three things only: parse a reference, talk to its tool, return a `SecretString`. Cross-cutting concerns — caching, batching, error UX, mode gating — live above the trait, in the registry and in the `secret()` MiniJinja callback.

    `probe()` is the diagnostic entry point. It runs once per `dodot up` (preflight) and once per `dodot secret probe` invocation. Probes must be cheap — never block on a network round-trip, never read the secret itself. The implementations follow a "binary on PATH, then auth state" two-step:

        match self.runner.run("op", &["--version".into()]) {
            Err(_)     => return ProbeResult::NotInstalled { hint: ... },
            Ok(out) if out.exit_code != 0 => return ProbeResult::ProbeFailed { ... },
            Ok(_) => {}
        }
        // ... auth-state check ...

    :: text ::

    `resolve()` is the wire-hitting entry point. The registry's cache lives ABOVE the trait — `resolve()` itself never consults a cache and never deduplicates. That's by design: it makes `resolve_call_count()` meaningful in tests and lets the registry's caller decide when to bypass the cache.

3. `SecretString`

    `secret/secret_string.rs:45`. A wrapper around `Vec<u8>` that:

        - Zeroes its buffer on `Drop` (via the `zeroize` crate).
        - Implements `Debug` with a redacted shape: `SecretString(<redacted>, len=N)` — the length is safe to log and lets you tell "empty value resolved" apart from "no value resolved" in traces, but the bytes themselves never reach the formatter.
        - Has no `Display`, `Serialize`, or `Clone` impl. Printing a `SecretString` with `{value}` fails to compile rather than silently formatting the bytes; serializing it (e.g. into a baseline cache JSON, into a tracing field) is rejected at compile time.
        - Refuses to copy itself implicitly. Callers that need shared access wrap in `Arc<SecretString>` (the registry cache does exactly this).
        - Provides `expose(&self) -> Result<&str, Utf8Error>` for the value-injection path and `expose_bytes(&self) -> &[u8]` for whole-file binary payloads. These are the only ways to read the bytes; calling either is a deliberate choice the reader has to make.
        - Provides `contains_newline(&self) -> bool` for the §3.4 multi-line refusal gate without exposing the bytes.

    The protection is best-effort. The rendered template content lands on disk in plaintext (that's the whole point of the deploy step) and the OS's filesystem permissions take over from there. `SecretString`'s value is the brief window between provider response and template rendering — and the within-run cache, where the `Arc<SecretString>` lives across multiple template renders within one `dodot up`.

4. `SecretRegistry`

    `secret/registry.rs:35`. The piece that ties providers, scheme dispatch, and the within-run cache together. Holds:

        pub struct SecretRegistry {
            providers: HashMap<String, Arc<dyn SecretProvider>>,
            cache: Arc<Mutex<HashMap<String, Arc<SecretString>>>>,
        }

    :: text ::

    Both fields live behind `Arc` so cloning the registry shares state. The cache scope today is **per-pack**, not per-run: `default_registry` (`preprocessing/mod.rs:260`) constructs a fresh `SecretRegistry` for each pack the pipeline visits, so cross-pack cache hits don't happen. Within one pack — across multiple template files in that pack — clones of the same registry instance share cache state because of the `Arc<Mutex<HashMap>>` shape. The `Arc` wrapping is what makes a future cross-pack-sharing refactor cheap (build once at `commands::up`, thread the same `Arc<SecretRegistry>` through every per-pack call); that refactor isn't shipped today and is captured as a deferred item in [./../proposals/shipped/secrets.lex] §9.1.

    The cache is hand-split into `cache_get` (returns `Option<Arc<SecretString>>`) and `cache_put` (takes `Arc<SecretString>`) rather than a single `resolve_cached`. The reason is co-location: the rich §3.4 multi-line / non-UTF-8 error messages live with the rendering surface (the `secret()` callback in `template.rs`), and the cache is dumb storage. The callback's flow is:

        1. cache_get(reference) -> Option<Arc<SecretString>>
           |
           +-- Some -> use it
           |
           +-- None -> registry.resolve() -> SecretString
                        |
                        +-- contains_newline -> raise §3.4 error (don't cache)
                        |
                        +-- expose() fails  -> raise UTF-8 error (don't cache)
                        |
                        +-- happy path      -> wrap in Arc, cache_put, use it

    :: text ::

    Reference parsing lives at `secret/registry.rs:139` (the helper) — `split_scheme(reference)` returns `(&str, &str)` for `(scheme, suffix)`. The provider gets the suffix verbatim; per-provider parsers (e.g. `keychain.rs:67`) take it from there.

    There's a shipping wart documented at `secret/registry.rs:200` worth knowing about: the helper `scheme_to_config_key`. The `secret-tool` scheme appears in references with a hyphen (`secret-tool:`) but in TOML config with an underscore (`[secret.providers.secret_tool]`) because confique's `Config` derive maps TOML keys 1:1 to Rust field names and Rust identifiers can't contain hyphens. The helper translates at user-facing error-message edges so the "no provider for scheme" hint suggests the loadable config key.

5. Wiring the Registry — `default_registry` + `build_secret_registry`

    `preprocessing/mod.rs:260`. Called per-pack from `packs::orchestration` and `commands::status` to build the preprocessor registry that the pipeline dispatches against. Returns a tuple:

        pub fn default_registry(
            preprocessor_config: &PreprocessorSection,
            secret_config:       &SecretSection,
            pather:              &dyn Pather,
            command_runner:      Arc<dyn CommandRunner>,
        ) -> Result<(PreprocessorRegistry, Option<Arc<SecretRegistry>>)>

    :: text ::

    Three preprocessors get registered unconditionally (unarchive, template); `age` and `gpg` register only when their `[preprocessor.<scheme>] enabled = true` block is set; the `SecretRegistry` is built when at least one `[secret.providers.*]` block is enabled. The template preprocessor gets the secret registry wired in via `with_secret_registry` so its `secret()` callback can dispatch.

    `build_secret_registry` (`mod.rs:341`) is the inner helper. Public because `commands::up::up()` calls it directly to build a registry for run-level preflight without going through `default_registry` (it doesn't want a preprocessor registry, just the secret one).

    The `[secret]` block is read from the **root** config, never from per-pack config. See `SecretSection` doc at `config/mod.rs::SecretSection` for the rationale.

6. The `secret()` MiniJinja Function

    `preprocessing/template.rs`, search for `with_secret_registry`. Two installations: with-registry and without. Without (default), every `secret(...)` call raises a render-time error pointing at `[secret] enabled = true` and a per-provider block. With:

        env.add_function("secret", move |reference: &str| -> Result<String, MjError> {
            // Within-run cache: cache_get -> Some(Arc<SecretString>) on hit.
            let secret = if let Some(cached) = registry.cache_get(reference) {
                cached
            } else {
                let value = registry.resolve(reference)?;
                if value.contains_newline() { return Err(...); }  // §3.4 refusal
                value.expose()?;                                   // UTF-8 check
                let arc = Arc::new(value);
                registry.cache_put(reference, Arc::clone(&arc));
                arc
            };

            // Sentinel-based line tracking: secret() returns a unique
            // private-use sentinel string, NOT the resolved value. After
            // rendering, finalize_secrets() walks the output to find each
            // sentinel, records its line range, and substitutes the real
            // value. Avoids the substring-collision failure mode where
            // a secret value happens to also appear elsewhere in the
            // rendered template.
            let mut entries = sidecar.lock().unwrap();
            let sentinel = make_secret_sentinel(render_id, entries.len());
            entries.push(SecretCallEntry { sentinel, reference, value });
            sentinel
        });

    :: text ::

    The sentinel format is `\u{E000}DSEC.<render_id>.<call_idx>\u{E001}` — both bracket characters live in Unicode's private-use area and don't appear in real dotfile content. The `render_id` is a process-wide monotonic counter so two concurrent renders can't observe each other's sentinels.

    After `tracker.render()` returns, `finalize_secrets()` (search in `template.rs`) walks the output to:
        1. Build line ranges from sentinel positions.
        2. Substitute every sentinel with its real value in BOTH `rendered` and `tracked` (the marker stream burgertocow uses for reverse-merge).

7. Whole-File Preprocessors

    `preprocessing/age.rs` and `preprocessing/gpg.rs` are conventional `Preprocessor` impls. The Phase S3 contract is a single field on `ExpandedFile`:

        pub struct ExpandedFile {
            pub relative_path:     PathBuf,
            pub content:           Vec<u8>,
            pub is_dir:            bool,
            pub tracked_render:    Option<String>,
            pub context_hash:      Option<[u8; 32]>,
            pub secret_line_ranges: Vec<SecretLineRange>,
            pub deploy_mode:       Option<u32>,    // <-- Phase S3 addition
        }

    :: text ::

    `deploy_mode = Some(0o600)` does two things at the pipeline level:
        1. The pipeline routes the write through `DataStore::write_rendered_file_with_mode` (`preprocessing/pipeline.rs`, search for `deploy_mode`), which uses `Fs::write_file_with_mode` — atomic create-with-mode + chmod-empty + write. The plaintext bytes never sit on disk under a permissive mode; the leak window is zero.
        2. The pipeline's divergence-guard gate (`preprocessing/pipeline.rs`, search for `participates_in_divergence_guard`) flips on for whole-file secrets. A user edit to the deployed plaintext is preserved on the next `dodot up` (the §6.4 guard fires); auto-merge isn't attempted because Opaque transforms have no reverse path.

    Both providers use `CommandRunner::run_bytes` rather than `run`. The default `run` decodes stdout via `String::from_utf8_lossy`, which corrupts non-UTF-8 plaintext (binary key blobs, X.509 DER, etc.). `run_bytes` returns `Vec<u8>` end-to-end; the dedicated impl in `ShellCommandRunner` reads stdout as raw bytes from the start.

8. The Sidecar — `<baseline>.secret.json`

    `preprocessing/baseline.rs`, search for `SecretsSidecar`. Schema:

        pub struct SecretsSidecar {
            pub version: u32,                            // = 1
            pub secret_line_ranges: Vec<SecretLineRange>,
        }

        pub struct SecretLineRange {
            pub start: usize,        // 0-indexed, inclusive
            pub end:   usize,        // 0-indexed, exclusive (always start + 1 in S1)
            pub reference: String,   // e.g. "pass:test/db_password"
        }

    :: text ::

    Lives at `<cache_dir>/preprocessor/<pack>/<handler>/<filename>.secret.json` — same dir as the baseline JSON, suffix differs. `SecretsSidecar::write` is a no-op + remove-stale when `secret_line_ranges` is empty (templates without secrets don't drop empty files). `SecretsSidecar::load` returns `Ok(None)` when no file exists — that's the documented "no secrets to mask" state.

    The sidecar is consumed in two places:
        - **Reverse-merge** (`preprocessing/reverse_merge.rs`, search for `secret_ranges`): the line ranges become the `mask_deployed_lines` argument to `burgertocow::generate_diff_with_markers_opts`. Lines listed in the mask are treated as already-matching the cached render regardless of actual content, so a rotated secret value in the deployed file doesn't propagate back to the template source as a literal.
        - **`dodot transform status`** (`commands/transform.rs`, search for `secret_references`): each entry's `secret_references: Vec<String>` is populated from the sidecar so the rendered status surfaces which secrets each baseline depends on without re-rendering.

9. Linear Code Walk: `dodot up` With a `secret()` Call

    User runs `dodot up` against a pack containing `app/config.toml.tmpl` with `{{ secret("pass:db") }}`. Trace:

        $ dodot up

    :: shell ::

        1. `crates/dodot-cli/src/main.rs` dispatches to `commands::up::up()`.

        2. `commands/up.rs:55` discovers packs via `prepare_packs()`.

        3. `commands/up.rs:65-76` runs preflight ONCE for the whole run.
              - `build_secret_registry(root_config.secret, ctx.command_runner, dotfiles_root)` builds the `SecretRegistry` with one provider per `[secret.providers.*] enabled = true` block.
              - `secret::preflight(&registry)` calls `registry.probe_all()` (`registry.rs:183`), which walks providers and runs `provider.probe()` on each.
              - On any non-Ok outcome, `error_render::render_probe_outcome` formats each failing row and `preflight()` aggregates into a single error. `dodot up` aborts before any rendering — user sees every fix-it pointer at once.
              - Skipped on `--dry-run` per the §7.4 Passive contract.

        4. For each pack, `orchestration::plan_pack` calls `default_registry()` to build a fresh `(PreprocessorRegistry, Option<Arc<SecretRegistry>>)` pair. The `SecretRegistry` built here is independent of the one preflight just exercised — `default_registry` rebuilds from root config each call rather than threading a shared instance. Cache scope is "per pack" today (see §4 above); cross-pack cache sharing is captured as a deferred item in [./../proposals/shipped/secrets.lex] §9.1.

        5. The pipeline dispatches `app/config.toml.tmpl` to `TemplatePreprocessor::expand` (`preprocessing/template.rs`).

        6. `expand()` builds a fresh burgertocow `Tracker` and installs the `secret()` MiniJinja function (with-registry path).

        7. MiniJinja parses the template. When it hits `{{ secret("pass:db") }}`, it invokes the callback:
              - `registry.cache_get("pass:db")` returns `None` on first call.
              - `registry.resolve("pass:db")` -> `split_scheme` -> dispatches to `PassProvider::resolve` (`secret/pass.rs`).
              - `PassProvider::resolve` -> `validate_reference` (rejects `..` segments) -> `runner.run("pass", &["show", "db"])` -> reads first line -> `SecretString::new(...)`.
              - Back in the callback: `value.contains_newline()` checked, `value.expose()` checked, wrap in `Arc`, `cache_put`.
              - Generate sentinel `\u{E000}DSEC.<id>.0\u{E001}`, push `SecretCallEntry { sentinel, reference, value }` to the per-render accumulator, return the sentinel string to MiniJinja.

        8. MiniJinja substitutes the sentinel into the rendered output. Every other `{{ secret("pass:db") }}` in this template (or any other rendered through the same registry instance) hits the cache — one provider call, N references.

        9. After `tracker.render()` returns, `finalize_secrets()` (`template.rs`):
              - Walks `rendered` to find each sentinel; records its line range.
              - Substitutes every sentinel with the real value in BOTH `rendered` and `tracked_str`. After this, the rendered output is the final deployed bytes; the tracked stream is what the baseline cache stores.
              - Returns `(rendered, tracked_str, secret_line_ranges)`.

        10. `expand()` returns an `ExpandedFile` with `secret_line_ranges` populated. `deploy_mode = None` (templates aren't whole-file secrets).

        11. The pipeline (`preprocessing/pipeline.rs`):
              - Runs the divergence guard (gates on `tracked_render.is_some()` for templates).
              - Writes the rendered bytes via `datastore.write_rendered_file` (the no-mode path).
              - Persists the baseline next to the cache, with `tracked_render` populated.
              - Persists the sidecar via `SecretsSidecar::new(secret_line_ranges).write(...)`.

        12. The symlink handler links the rendered file to `~/.config/app/config.toml`. Done.

    On the next `dodot up` (or `git status` for the clean filter, or `dodot transform check`): the sidecar is loaded, the line ranges become the burgertocow mask, rotated secret values are invisible to reverse-merge.

10. Linear Code Walk: `dodot up` With `*.age`

    User runs `dodot up` against a pack containing `ssh/id_ed25519.age`. Same start (preflight, `default_registry`), then:

        1. `[preprocessor.age] enabled = true` is set in root config, so `default_registry` registers an `AgePreprocessor` with the configured identity path.

        2. The pipeline dispatches `ssh/id_ed25519.age` to `AgePreprocessor::expand` (`preprocessing/age.rs`).

        3. `expand()` calls `runner.run_bytes("age", &["--decrypt", "--identity", <path>, <source>])`. **`run_bytes`, not `run`** — preserves binary plaintext byte-for-byte.

        4. On non-zero exit, `expand()` maps documented stderr shapes ("no identity matched", "identity does not exist") to actionable hints; surfaces unrecognized stderr verbatim with `exit N` context.

        5. On success, returns:
              - `relative_path`: source filename with `.age` stripped (`id_ed25519`).
              - `content`: raw plaintext bytes from `out.stdout` (Vec<u8>, no decode).
              - `tracked_render: None`, `context_hash: None`, `secret_line_ranges: vec![]`.
              - **`deploy_mode: Some(0o600)`** — the §4.3 contract.

        6. The pipeline:
              - Divergence-guard gate fires for whole-file secrets too (gate: `tracked_render.is_some() || deploy_mode.is_some()`).
              - Routes the write through `datastore.write_rendered_file_with_mode(.., 0o600)` -> `fs.write_file_with_mode(.., 0o600)` -> `OpenOptions::mode(0o600)` + chmod-empty + write. Plaintext never sits on disk at the umask default.
              - Persists a baseline (rendered_hash + source_hash, no tracked_render). Future `dodot up` calls compare deployed hash vs. rendered_hash to detect divergence.

        7. The symlink handler links `<datastore>/preprocessed/id_ed25519` to `~/.config/ssh/id_ed25519`.

    If the user hand-edits `~/.config/ssh/id_ed25519` later, the next `dodot up`'s divergence guard fires (the deployed hash no longer matches the baseline's `rendered_hash`), the write is skipped, a "preserved" warning surfaces. The user re-encrypts manually, commits the new `.age`, and the next `up` re-renders.

11. Inspection Surface — `dodot secret probe` / `list` / `transform status`

    `commands/secret.rs::probe` (`secret.rs:59`):
        - Builds the registry from root config (`build_secret_registry`).
        - Calls `registry.probe_all()`.
        - Maps each `(scheme, ProbeResult)` pair into a `ProbeRow` with snake-case state and the rendered hint.
        - Returns a `ProbeResult` (the command-result struct, not the trait enum) with rollup counts for the renderer.

    `commands/secret.rs::list` (`secret.rs:182`):
        - `prepare_packs` walks every pack.
        - For each pack, `Scanner::walk_pack` enumerates pack files; the function filters to template-extension matches.
        - For each template, reads the bytes and runs `scan_secret_calls(text)` — a hand-rolled byte-wise state machine (`secret.rs::scan_secret_calls`). Matches `secret(...)` with single or double quotes, whitespace tolerance, word-boundary anchoring (so `mysecret(...)` / `secrets(...)` don't match), and graceful handling of malformed input.
        - For each occurrence: extracts the scheme prefix, looks up whether the scheme has an enabled provider (computed once from root config), produces a `SecretRefRow`.
        - Aggregates `schemes_referenced` and `schemes_without_provider` for the rollup.

    `commands/transform.rs::status` (`transform.rs::status`):
        - The pre-Phase-S5 path collects divergence reports for every cached baseline.
        - Phase S5 addition: for each report, `SecretsSidecar::load(...)` reads the sidecar; the `secret_line_ranges` are flattened to a `Vec<String>` of references and stored on the entry. Best-effort: a parse error leaves the row's references empty rather than failing the report.
        - The `transform-status.jinja` template indents each reference under the parent row.

12. Tests

    Three tiers per [./../proposals/shipped/secrets-testing.lex] §3-§5:

    - **Tier 0 (unit, `cargo test`).** Pure dodot logic. `MockSecretProvider` (`secret/test_support.rs`) returns canned values from a `HashMap`; `PanickingProvider` (same file) panics on `resolve()` to pin the §7.4 Passive contract. Provider impls have their own `ScriptedRunner` mock per file (`pass.rs`, `op.rs`, etc.) for command-shape and stderr-mapping coverage. Roughly 1059 lib tests at the time of writing.

    - **Tier 1 (hermetic real-binary, `bats` on every PR).** Each `tests/e2e/bats/test_secrets_*.bats` file builds a fresh sandbox and exercises the real binary against fixture data. `pass`, `age`, `gpg` fixtures generate keypairs in `$SANDBOX` and seed catalog entries; `bw` uses a stub binary on PATH (real bw needs an account); `op` doesn't have a tier-1 yet because it always needs an account. Tests skip when the binary isn't on the host.

    - **Tier 2 (real cloud / OS keystore).** Deferred. The op / bw real-provider workflow lands separately when the dedicated CI runners exist. The `keychain` and `secret-tool` providers are tier-0-only because writing to the user's real OS keystore from automated tests is unsafe — see [./../proposals/shipped/secrets-testing.lex] §5.3.

    Tier-0 test patterns to copy when adding a new provider:
        - Reference parsing: every shape, every rejection (empty, malformed, etc.).
        - Probe: Ok / NotInstalled / each documented auth state.
        - Resolve: happy path with trailing-newline strip, every documented stderr shape, runner-level "command not found".
        - Binary safety (for whole-file): `0xff 0xfe 0x80` round-trips verbatim.

13. Adding a New Provider

    Steps that should match every existing provider in `secret/<name>.rs`:

        1. Add `secret/<name>.rs` with a struct holding `Arc<dyn CommandRunner>` and any provider-specific state. Implement `SecretProvider`. `from_env(runner)` for the default constructor; `new(runner, ...)` for tests.

        2. Reference parsing in a private fn `parse_reference`. Reject empty / malformed up front. The registry has already stripped the `<scheme>:` prefix; you parse the suffix.

        3. `probe()`: binary check first, then auth state. NotInstalled gets the install hint; NotAuthenticated gets the actionable fix. Don't probe with a known reference — there isn't one at probe time.

        4. `resolve()`: call the tool, map exit codes / stderr shapes to actionable errors. Strip exactly one trailing newline if the tool adds one. If your tool emits binary, use `runner.run_bytes` and have the provider take `Vec<u8>` — see `age.rs` / `gpg.rs`.

        5. Wire into the module root: `secret/mod.rs` `pub mod <name>; pub use <name>::<Name>Provider;`.

        6. Add a config block to `config/mod.rs`: `pub struct SecretProvider<Name> { pub enabled: bool, ... }`, default `enabled = false`. Add a `<name>: SecretProvider<Name>` field on `SecretProvidersSection`. **If the scheme has a hyphen, see §4 above for the underscore TOML key wart.**

        7. Wire into `build_secret_registry` (`preprocessing/mod.rs:341`): `if config.providers.<name>.enabled { reg.register(Arc::new(<Name>Provider::from_env(runner.clone()))); any_enabled = true; }`.

        8. Add tier-0 tests with `ScriptedRunner` covering every shape from §12.

        9. Add a tier-1 bats file at `tests/e2e/bats/test_secrets_<name>.bats` if the binary is hermetically testable. Skip-on-missing-binary follows the existing pattern in `test_secrets_age.bats`.

        10. Document in [./../user/secrets.lex] §3 (add a row to the table + a quirks bullet).

14. Cross-References

    - User-facing guide: [./../user/secrets.lex]
    - Conceptual overview (where secrets fit in the preprocessor pipeline): [./../reference/pre-processors.lex] §6
    - Original design proposal (preserved as historical context): [./../proposals/shipped/secrets.lex]
    - Testing strategy (sibling proposal): [./../proposals/shipped/secrets-testing.lex]
    - Divergence guard reference: [./../proposals/shipped/preprocessing-pipeline.lex] §6.4
    - Magic-stack design (for the `transform check` / clean-filter context the sidecar plugs into): [./../proposals/shipped/magic.lex]
