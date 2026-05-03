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
}
