//! `dodot secret` subcommands — Phase S5 ergonomics surface.
//!
//! Two read-only commands for inspecting the secrets configuration
//! and template references without running `dodot up`:
//!
//! - [`probe`] — runs `probe()` on every configured provider and
//!   returns one row per provider with the outcome. Useful as a
//!   "is my secrets setup healthy?" check before relying on a
//!   `dodot up` to surface the same diagnostics.
//! - [`list`] — scans every pack's templates for `secret(...)`
//!   references and returns one row per call. Read-only — never
//!   invokes a provider.
//!
//! Both commands take an [`ExecutionContext`] (already built by the
//! CLI handler), build the registry from root config (Phase S4
//! contract: `[secret]` is root-only — see `SecretSection` docs),
//! and return a serializable result for the standout renderer.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::Result;

/// One provider's row in `dodot secret probe` output.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeRow {
    pub scheme: String,
    /// Snake-case state suitable for template branching:
    /// `ok` / `not_installed` / `not_authenticated` / `misconfigured`
    /// / `probe_failed`.
    pub state: String,
    /// Human-readable hint (the provider's own string for
    /// non-Ok outcomes; empty for Ok).
    pub hint: String,
}

/// Aggregate result of `dodot secret probe`.
///
/// Always non-fatal — even a fully-broken provider lineup isn't
/// an error here, just information. The CLI exit code is 0 on
/// success of the *command* (we got results); the *contents*
/// surface via the rendered output.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub rows: Vec<ProbeRow>,
    pub ok_count: usize,
    pub failing_count: usize,
    /// True iff `[secret] enabled = false` or no provider is
    /// enabled. The renderer surfaces a different message in that
    /// case (no providers to probe vs. all-Ok).
    pub disabled: bool,
}

/// Run `dodot secret probe`. Builds the registry from root config
/// (`[secret]` is root-only per `SecretSection` docs), runs
/// `probe()` on each enabled provider, and returns a row per
/// provider. Never invokes `resolve()` and never reads template
/// content.
pub fn probe(ctx: &ExecutionContext) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    if !root_config.secret.enabled {
        return Ok(ProbeResult {
            rows: Vec::new(),
            ok_count: 0,
            failing_count: 0,
            disabled: true,
        });
    }
    let registry = match crate::preprocessing::build_secret_registry(
        &root_config.secret,
        ctx.command_runner.clone(),
        ctx.paths.dotfiles_root(),
    ) {
        Some(r) => r,
        None => {
            return Ok(ProbeResult {
                rows: Vec::new(),
                ok_count: 0,
                failing_count: 0,
                disabled: true,
            });
        }
    };

    use crate::secret::ProbeResult as P;
    let outcomes = registry.probe_all();
    let mut rows = Vec::with_capacity(outcomes.len());
    let mut ok_count = 0usize;
    let mut failing_count = 0usize;
    for (scheme, outcome) in outcomes {
        let (state, hint) = match outcome {
            P::Ok => {
                ok_count += 1;
                ("ok", String::new())
            }
            P::NotInstalled { hint } => {
                failing_count += 1;
                ("not_installed", hint)
            }
            P::NotAuthenticated { hint } => {
                failing_count += 1;
                ("not_authenticated", hint)
            }
            P::Misconfigured { hint } => {
                failing_count += 1;
                ("misconfigured", hint)
            }
            P::ProbeFailed { details } => {
                failing_count += 1;
                ("probe_failed", details)
            }
        };
        rows.push(ProbeRow {
            scheme,
            state: state.to_string(),
            hint,
        });
    }

    Ok(ProbeResult {
        rows,
        ok_count,
        failing_count,
        disabled: false,
    })
}

/// One occurrence of a `secret(...)` call in a template source.
#[derive(Debug, Clone, Serialize)]
pub struct SecretRefRow {
    pub pack: String,
    /// Pack-relative path of the template source (e.g.
    /// `config.toml.tmpl`, `nested/db.toml.tmpl`).
    pub source_path: String,
    /// 1-indexed line number where the `secret(...)` call begins
    /// in the template source.
    pub line: usize,
    /// The full reference passed to `secret(...)`, with scheme
    /// prefix (e.g. `pass:test/db_password`,
    /// `op://Personal/GitHub/token`).
    pub reference: String,
    /// The scheme half (`pass`, `op`, `bw`, ...) — the part
    /// before the first `:` of the reference. Empty if the
    /// reference is malformed (we still surface the row so the
    /// user can see the broken call site).
    pub scheme: String,
    /// True iff a provider for this scheme is currently enabled
    /// in `[secret.providers.*]`. Lets the renderer flag
    /// references that would fail at render time today, so the
    /// user can decide whether to enable a provider or remove
    /// the call.
    pub provider_enabled: bool,
}

/// Aggregate result of `dodot secret list`.
#[derive(Debug, Clone, Serialize)]
pub struct ListResult {
    pub rows: Vec<SecretRefRow>,
    pub total_count: usize,
    /// Set of distinct schemes referenced across all rows,
    /// sorted for stable output. Useful for the
    /// "schemes referenced but not enabled" rollup.
    pub schemes_referenced: Vec<String>,
    /// Subset of `schemes_referenced` that does NOT have a
    /// provider enabled in the current config — these
    /// references would fail at render time today.
    pub schemes_without_provider: Vec<String>,
}

/// Run `dodot secret list`. Walks every pack's template source
/// files, extracts `secret(...)` calls with the byte-wise
/// scanner in [`scan_secret_calls`], and returns one row per
/// occurrence. Read-only — never invokes a provider and never
/// reads sidecars (sidecars only exist post-render; `list` is
/// meant to be useful BEFORE the first `dodot up`).
///
/// "Template" here means files whose name matches
/// `[preprocessor.template] extensions` per the root config —
/// same set the template preprocessor would expand. Other
/// preprocessors (age / gpg) are deliberately not scanned: a
/// `secret(...)` call inside an encrypted file isn't visible
/// without decrypting first.
pub fn list(ctx: &ExecutionContext) -> Result<ListResult> {
    use crate::packs::orchestration::prepare_packs;

    let root_config = ctx.config_manager.root_config()?;
    let template_extensions: Vec<String> = root_config
        .preprocessor
        .template
        .extensions
        .iter()
        .map(|e| e.trim_start_matches('.').to_string())
        .collect();

    // Compute the set of currently-enabled provider schemes once
    // — used to set `provider_enabled` per row without rebuilding
    // the registry per call.
    let enabled_schemes: std::collections::HashSet<String> = {
        let mut s = std::collections::HashSet::new();
        if root_config.secret.enabled {
            let p = &root_config.secret.providers;
            if p.pass.enabled {
                s.insert("pass".into());
            }
            if p.op.enabled {
                s.insert("op".into());
            }
            if p.bw.enabled {
                s.insert("bw".into());
            }
            if p.sops.enabled {
                s.insert("sops".into());
            }
            if p.keychain.enabled {
                s.insert("keychain".into());
            }
            if p.secret_tool.enabled {
                s.insert("secret-tool".into());
            }
        }
        s
    };

    let packs = prepare_packs(None, ctx)?;
    let mut rows: Vec<SecretRefRow> = Vec::new();
    let scanner = crate::rules::Scanner::new(ctx.fs.as_ref());

    for pack in &packs {
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        let entries = scanner.walk_pack(&pack.path, &pack_config.pack.ignore)?;
        for entry in entries {
            if entry.is_dir {
                continue;
            }
            // Only scan template-shaped files. Other extensions
            // can't contain `secret(...)` calls in a way the
            // template preprocessor would resolve.
            let filename = entry
                .relative_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let is_template = template_extensions.iter().any(|ext| {
                filename
                    .strip_suffix(ext.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
            });
            if !is_template {
                continue;
            }
            let bytes = match ctx.fs.read_file(&entry.absolute_path) {
                Ok(b) => b,
                Err(_) => continue, // unreadable file → silently skip
            };
            let text = match std::str::from_utf8(&bytes) {
                Ok(s) => s,
                Err(_) => continue, // non-UTF-8 template → skip
            };
            for occ in scan_secret_calls(text) {
                let scheme = match occ.reference.split_once(':') {
                    Some((s, _)) => s.to_string(),
                    None => String::new(),
                };
                let provider_enabled = !scheme.is_empty() && enabled_schemes.contains(&scheme);
                rows.push(SecretRefRow {
                    pack: pack.display_name.clone(),
                    source_path: entry.relative_path.to_string_lossy().to_string(),
                    line: occ.line,
                    reference: occ.reference,
                    scheme,
                    provider_enabled,
                });
            }
        }
    }

    let mut schemes_referenced: Vec<String> = rows
        .iter()
        .filter(|r| !r.scheme.is_empty())
        .map(|r| r.scheme.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    schemes_referenced.sort();

    let schemes_without_provider: Vec<String> = schemes_referenced
        .iter()
        .filter(|s| !enabled_schemes.contains(s.as_str()))
        .cloned()
        .collect();

    let total_count = rows.len();
    Ok(ListResult {
        rows,
        total_count,
        schemes_referenced,
        schemes_without_provider,
    })
}

/// One match from [`scan_secret_calls`].
#[derive(Debug, Clone)]
struct SecretCallOccurrence {
    line: usize,
    reference: String,
}

/// Find every `secret(...)` call in a template source. Matches
/// the canonical MiniJinja shapes:
///
///     {{ secret("op://Vault/Item/Field") }}
///     {{ secret('pass:path/to/secret') }}
///     {%- if secret("op://...") -%} ... {%- endif -%}
///
/// Whitespace between `secret`, the parens, and the string is
/// allowed. Both single- and double-quoted strings work; the
/// quote character must match. Escape sequences inside the
/// string are NOT honored — references in dotfiles don't
/// contain backslash escapes in practice, and the simpler
/// "everything between matching quotes is the reference" rule
/// keeps the scanner predictable.
///
/// The scanner is deliberately a hand-rolled byte-wise state
/// machine rather than a real MiniJinja AST walk: this command
/// runs BEFORE the first `dodot up`, so we can't rely on a
/// baseline cache, and a false positive here just lists a
/// string the user already typed in the template — they can
/// verify by opening the file at the reported line. Actual
/// rendering still goes through MiniJinja's parser. Skipping
/// the regex crate keeps this off the dependency footprint and
/// makes the matching rule grep-able from one place.
fn scan_secret_calls(text: &str) -> Vec<SecretCallOccurrence> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let needle = b"secret";
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }
        // Must be at a word boundary on the left — otherwise
        // `mysecret(...)` would match.
        let left_ok = i == 0 || {
            let prev = bytes[i - 1];
            !prev.is_ascii_alphanumeric() && prev != b'_'
        };
        if !left_ok {
            i += 1;
            continue;
        }
        // Walk past `secret`, optional whitespace, expect `(`.
        let mut j = i + needle.len();
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'(' {
            i += 1;
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || (bytes[j] != b'"' && bytes[j] != b'\'') {
            i += 1;
            continue;
        }
        let quote = bytes[j];
        j += 1;
        let ref_start = j;
        while j < bytes.len() && bytes[j] != quote {
            j += 1;
        }
        if j >= bytes.len() {
            // Unterminated — bail; whoever rendered this would
            // get a MiniJinja parse error anyway.
            break;
        }
        let reference = std::str::from_utf8(&bytes[ref_start..j])
            .unwrap_or("")
            .to_string();
        // Compute 1-indexed line number for the reported
        // position (the start of `secret`).
        let line = bytes[..i].iter().filter(|&&b| b == b'\n').count() + 1;
        out.push(SecretCallOccurrence { line, reference });
        i = j + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Fs;
    use crate::testing::TempEnvironment;

    fn make_ctx(env: &TempEnvironment, root_config_toml: Option<&str>) -> ExecutionContext {
        if let Some(toml) = root_config_toml {
            let path = env.dotfiles_root.join(".dodot.toml");
            env.fs.write_file(&path, toml.as_bytes()).unwrap();
        }
        ExecutionContext::production(&env.dotfiles_root, false).expect("test context build")
    }

    #[test]
    fn probe_reports_disabled_when_master_switch_off() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env, Some("[secret]\nenabled = false\n"));
        let r = probe(&ctx).unwrap();
        assert!(r.disabled);
        assert!(r.rows.is_empty());
        assert_eq!(r.ok_count, 0);
        assert_eq!(r.failing_count, 0);
    }

    #[test]
    fn probe_reports_disabled_when_no_provider_is_enabled() {
        // Master switch on, but every provider block is opt-in
        // (`enabled = false` by default) — same observable shape
        // as the master switch off, just a different reason.
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env, Some("[secret]\nenabled = true\n"));
        let r = probe(&ctx).unwrap();
        assert!(r.disabled);
        assert!(r.rows.is_empty());
    }

    // Note: tests that exercise the live registry path (with a
    // mock CommandRunner injected) belong in the e2e suite —
    // production()'s ExecutionContext owns the runner and there's
    // no tier-0 seam to substitute it. The error-render tests
    // already pin the per-row mapping shape; this command is the
    // shallow aggregator that calls into them.

    // ── scan_secret_calls ───────────────────────────────────────

    #[test]
    fn scan_finds_double_quoted_call() {
        let text = r#"value = "{{ secret("pass:test/k") }}""#;
        let r = scan_secret_calls(text);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].line, 1);
        assert_eq!(r[0].reference, "pass:test/k");
    }

    #[test]
    fn scan_finds_single_quoted_call() {
        let text = r#"value = "{{ secret('pass:test/k') }}""#;
        let r = scan_secret_calls(text);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].reference, "pass:test/k");
    }

    #[test]
    fn scan_tolerates_whitespace_between_secret_paren_and_string() {
        let text = r#"{{ secret  (   "op://V/I/F"   ) }}"#;
        let r = scan_secret_calls(text);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].reference, "op://V/I/F");
    }

    #[test]
    fn scan_reports_correct_line_number_in_multiline_template() {
        let text = "header\nport = 5432\nkey = {{ secret(\"pass:k\") }}\nfooter\n";
        let r = scan_secret_calls(text);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].line, 3);
    }

    #[test]
    fn scan_finds_multiple_calls_in_one_template() {
        let text = r#"a = "{{ secret("pass:a") }}"
b = "{{ secret('op://V/I/F') }}"
c = "{{ secret("bw:gh-token") }}""#;
        let r = scan_secret_calls(text);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].reference, "pass:a");
        assert_eq!(r[1].reference, "op://V/I/F");
        assert_eq!(r[2].reference, "bw:gh-token");
    }

    #[test]
    fn scan_does_not_match_word_with_secret_prefix() {
        // `mysecret(...)` must not match — left word boundary
        // matters.
        let text = r#"x = mysecret("not-this")"#;
        let r = scan_secret_calls(text);
        assert!(r.is_empty());
    }

    #[test]
    fn scan_does_not_match_word_with_secret_suffix() {
        // `secrets(...)` (plural) must not match either.
        let text = r#"x = secrets("not-this")"#;
        let r = scan_secret_calls(text);
        assert!(r.is_empty());
    }

    #[test]
    fn scan_skips_unterminated_string_and_does_not_panic() {
        // Malformed input shouldn't crash the scanner; the user
        // would get a MiniJinja parse error at render time.
        let text = r#"x = {{ secret("unterminated"#;
        let _ = scan_secret_calls(text); // doesn't panic; we don't care what comes back
    }

    #[test]
    fn scan_handles_mismatched_quote_styles_independently() {
        // `secret("...')` is broken — opening double, closing
        // single. The scanner should walk to the next double
        // quote, which doesn't exist, and bail without surfacing
        // a misleading row.
        let text = r#"x = {{ secret("pass:k') }}"#;
        let r = scan_secret_calls(text);
        // Either zero rows (preferred) or one with garbage —
        // the contract is "don't crash, don't fabricate".
        for row in &r {
            assert!(!row.reference.is_empty());
        }
    }
}
