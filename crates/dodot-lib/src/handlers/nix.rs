//! Nix handler — runs `nix profile install --file <packages.nix>`
//! once per content hash, via the shared
//! [`crate::handlers::run_once`] machinery.
//!
//! The handler is the Linux counterpart to the existing Brewfile
//! handler: a per-pack `packages.nix` manifest at pack root declares
//! the packages the pack wants installed, and `dodot up` invokes
//! `nix profile install` against it. Sentinel + snapshot tracking,
//! the three-state notify-don't-rerun policy, and `dodot status`
//! integration are all inherited unchanged from
//! [`RunOnceHandler`](crate::handlers::run_once::RunOnceHandler).
//!
//! User-facing reference: `docs/user/handlers/nix.lex`.
//!
//! # Manifest shape
//!
//! `packages.nix` must evaluate to one of three forms (per
//! `docs/proposals/nix.lex` §5.2):
//!
//! - *List of derivations.* The canonical form.
//! - *Bare derivation.* Common case for a one-tool pack.
//! - *Attribute set of derivations.* Useful when a pack wants named
//!   attrs for tooling outside dodot.
//!
//! All three require the `{ pkgs ? import <nixpkgs> {} }:` function
//! wrapper with a default argument — that is what lets
//! `nix profile install --file <path>` work without dodot injecting
//! anything: Nix auto-applies functions with defaulted arguments at
//! evaluation time and resolves `pkgs` from the user's `NIX_PATH`.
//! A bare list literal with no function wrapper has no `pkgs` in
//! scope and fails to evaluate.
//!
//! # Pre-flight shape validation
//!
//! Before emitting a `Run` intent, [`NixCommand::validate`] invokes
//! `nix eval --file <path> --json --apply` with a small Nix
//! expression that classifies the manifest. Because `nix eval --file`
//! does *not* auto-apply functions (unlike `nix profile install
//! --file`), the probe expression first invokes the manifest's outer
//! function with `{}` when it sees one — that resolves the canonical
//! `{ pkgs ? import <nixpkgs> {} }: ...` wrapper to whichever shape
//! the body actually produces.
//!
//! v1 accepted shapes:
//!
//! - **`list`** → installed via `nix profile install --file <path>`.
//! - **`drv`** → installed via `nix profile install --file <path>`.
//! - **`set`** → *rejected in v1.* `nix profile install` against an
//!   attribute-set manifest requires an explicit `'.*'` selector
//!   argument, which means the install command shape depends on the
//!   probe result. Threading that per-shape dispatch through
//!   `command_for` is intentionally deferred to a later PR; for now,
//!   `validate` returns an error pointing the user at the
//!   `with pkgs; [ ... ]` list-literal form that is the spec's
//!   canonical shape anyway.
//! - **`unsupported`** (anything else — string, number, function that
//!   doesn't apply with defaults, etc.) → rejected with a manifest
//!   shape error.
//!
//! Delegating shape detection to Nix itself keeps dodot out of the
//! business of writing its own Nix parser.

use std::path::Path;

use crate::datastore::CommandRunner;
use crate::fs::Fs;
use crate::handlers::run_once::RunOnceCommand;
use crate::handlers::{ExecutionPhase, HANDLER_NIX};
use crate::{DodotError, Result};

/// The Nix expression used to classify `packages.nix` into one of the
/// supported shapes. Returned as a JSON string by `nix eval --apply`.
///
/// The expression is function-aware: the canonical manifest form is
/// `{ pkgs ? import <nixpkgs> {} }: ...`, and `nix eval --file <path>`
/// does *not* auto-apply functions (unlike `nix profile install
/// --file`). Without the `if builtins.isFunction f then f {} else f`
/// step, every recommended manifest would classify as `unsupported`.
const SHAPE_PROBE_EXPR: &str = r#"f:
  let
    x = if builtins.isFunction f then f {} else f;
  in
  if builtins.isList x then "list"
  else if builtins.isAttrs x && (x.type or "") == "derivation" then "drv"
  else if builtins.isAttrs x then "set"
  else "unsupported""#;

/// Defensive `--extra-experimental-features` argument passed on every
/// `nix` invocation. The flag is a no-op when the features are
/// already enabled in the user's `nix.conf`; it guards against the
/// case where a fresh Nix install hasn't opted into the new CLI yet.
const EXTRA_FEATURES_FLAG: &str = "--extra-experimental-features";
const EXTRA_FEATURES_VALUE: &str = "nix-command flakes";

/// Manifest shapes recognized by [`NixCommand::validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestShape {
    /// A list of derivations — accepted in v1.
    List,
    /// A bare derivation — accepted in v1.
    Drv,
    /// An attribute set of derivations — *rejected* in v1 with a
    /// targeted error. See module-level docs for the rationale and
    /// the migration path users have today.
    Set,
}

impl ManifestShape {
    fn from_probe(tag: &str) -> Option<Self> {
        match tag {
            "list" => Some(Self::List),
            "drv" => Some(Self::Drv),
            "set" => Some(Self::Set),
            _ => None,
        }
    }
}

/// [`RunOnceCommand`] for the `nix` handler.
///
/// Triggers on `packages.nix` at pack root and invokes
/// `nix profile install --file <path>` once per content hash.
/// Inherits the three-state notify-don't-rerun policy from
/// [`RunOnceHandler`](crate::handlers::run_once::RunOnceHandler).
pub struct NixCommand;

impl RunOnceCommand for NixCommand {
    fn handler_name(&self) -> &str {
        HANDLER_NIX
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Provision
    }

    fn command_for(&self, path: &Path) -> (String, Vec<String>) {
        // validate() has already rejected `set` and `unsupported`
        // shapes by the time we get here, so the install form is the
        // single canonical invocation for `list` and `drv` — no
        // selector argument needed.
        (
            "nix".into(),
            vec![
                "profile".into(),
                "install".into(),
                "--file".into(),
                path.to_string_lossy().into_owned(),
                EXTRA_FEATURES_FLAG.into(),
                EXTRA_FEATURES_VALUE.into(),
            ],
        )
    }

    fn validate(&self, _fs: &dyn Fs, runner: &dyn CommandRunner, path: &Path) -> Result<()> {
        // KNOWN GAP (#161 PR 2 candidate): this validator runs at
        // intent-planning time, before the executor consults
        // `DataStore::did_run`. That means an already-run
        // `packages.nix` that the user later edits into a broken
        // shape will fail planning here instead of reaching the
        // `RanDifferent` skip/notice path that the run-once
        // notify-don't-rerun policy promises. Closing the gap
        // requires either threading the datastore into `to_intents`
        // (broad trait change) or moving validation to the executor
        // (also broad). Deferred — for v1 the user gets a clear
        // shape error and can revert the edit; that's strictly
        // safer than installing a malformed manifest.
        let tag = probe_shape(runner, path)?;
        match ManifestShape::from_probe(&tag) {
            Some(ManifestShape::List) | Some(ManifestShape::Drv) => Ok(()),
            Some(ManifestShape::Set) => Err(DodotError::Fs {
                path: path.to_path_buf(),
                source: std::io::Error::other(
                    "packages.nix evaluates to an attribute set, which is not yet supported in \
                     v1 (the install would need `nix profile install --file <path> '.*'`, and \
                     per-shape install dispatch is deferred). Please use the list form: \
                     `{ pkgs ? import <nixpkgs> {} }: with pkgs; [ <packages> ]`",
                ),
            }),
            None => Err(DodotError::Fs {
                path: path.to_path_buf(),
                source: std::io::Error::other(format!(
                    "packages.nix has unsupported shape `{tag}` — must evaluate to a list of \
                     derivations or a bare derivation (see docs/user/handlers/nix.lex)"
                )),
            }),
        }
    }

    fn status_deployed(&self) -> &str {
        "nix packages installed"
    }

    fn status_pending(&self) -> &str {
        "nix packages not installed"
    }

    fn status_ran_different(&self) -> &str {
        "nix packages older version"
    }
}

/// Run `nix eval --file <path> --json --apply <SHAPE_PROBE_EXPR>` and
/// return the JSON-decoded shape tag (`"list"`, `"drv"`, `"set"`, or
/// `"unsupported"`).
///
/// A non-zero exit code or unparseable stdout is surfaced as an
/// `Fs`-flavored error so it propagates the same way validation
/// errors for other handlers do; the executor renders it back to the
/// user via the standard intent-error path.
fn probe_shape(runner: &dyn CommandRunner, path: &Path) -> Result<String> {
    let args: Vec<String> = vec![
        "eval".into(),
        "--file".into(),
        path.to_string_lossy().into_owned(),
        "--json".into(),
        "--apply".into(),
        SHAPE_PROBE_EXPR.into(),
        EXTRA_FEATURES_FLAG.into(),
        EXTRA_FEATURES_VALUE.into(),
    ];
    let out = runner.run("nix", &args)?;
    if out.exit_code != 0 {
        return Err(DodotError::Fs {
            path: path.to_path_buf(),
            source: std::io::Error::other(format!(
                "`nix eval` failed while validating packages.nix shape (exit {}): {}",
                out.exit_code,
                out.stderr.trim()
            )),
        });
    }
    // `nix eval --json` returns a JSON string (`"list"`, `"drv"`, …).
    // serde_json correctly handles surrounding whitespace and any
    // string escapes — manual trimming + quote-stripping breaks on
    // either.
    serde_json::from_str::<String>(out.stdout.trim()).map_err(|e| DodotError::Fs {
        path: path.to_path_buf(),
        source: std::io::Error::other(format!(
            "`nix eval` returned non-string JSON for the shape probe ({e}): {}",
            out.stdout.trim()
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner};
    use std::sync::Mutex;

    struct StubRunner {
        stdout: String,
        exit_code: i32,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl StubRunner {
        fn ok(stdout: &str) -> Self {
            Self {
                stdout: stdout.to_string(),
                exit_code: 0,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn failing(exit_code: i32) -> Self {
            Self {
                stdout: String::new(),
                exit_code,
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for StubRunner {
        fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput> {
            let mut call = vec![executable.to_string()];
            call.extend(arguments.iter().cloned());
            self.calls.lock().unwrap().push(call);
            Ok(CommandOutput {
                exit_code: self.exit_code,
                stdout: self.stdout.clone(),
                stderr: "stub stderr".into(),
            })
        }
    }

    fn fake_fs() -> crate::testing::TempEnvironment {
        crate::testing::TempEnvironment::builder()
            .pack("tools")
            .file(
                "packages.nix",
                "{ pkgs ? import <nixpkgs> {} }: [ pkgs.ripgrep ]",
            )
            .done()
            .build()
    }

    #[test]
    fn nix_command_identity() {
        assert_eq!(NixCommand.handler_name(), HANDLER_NIX);
        assert_eq!(NixCommand.phase(), ExecutionPhase::Provision);
        assert_eq!(NixCommand.status_deployed(), "nix packages installed");
        assert_eq!(NixCommand.status_pending(), "nix packages not installed");
        assert_eq!(
            NixCommand.status_ran_different(),
            "nix packages older version"
        );
    }

    #[test]
    fn command_for_returns_profile_install_with_features_flag() {
        let (exe, args) = NixCommand.command_for(Path::new("/p/tools/packages.nix"));
        assert_eq!(exe, "nix");
        assert_eq!(args[0], "profile");
        assert_eq!(args[1], "install");
        assert_eq!(args[2], "--file");
        assert_eq!(args[3], "/p/tools/packages.nix");
        assert_eq!(args[4], EXTRA_FEATURES_FLAG);
        assert_eq!(args[5], EXTRA_FEATURES_VALUE);
    }

    #[test]
    fn validate_accepts_list_shape() {
        let env = fake_fs();
        let abs = env.dotfiles_root.join("tools/packages.nix");
        let runner = StubRunner::ok("\"list\"\n");
        NixCommand
            .validate(env.fs.as_ref(), &runner, &abs)
            .expect("list shape should validate");
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(call[0], "nix");
        assert_eq!(call[1], "eval");
        assert_eq!(call[2], "--file");
        assert_eq!(call[3], abs.to_string_lossy());
        assert_eq!(call[4], "--json");
        assert_eq!(call[5], "--apply");
        assert!(call[6].contains("builtins.isList"));
        // Function-wrapper handling: the probe must call `f {}` so
        // the canonical `{ pkgs ? ... }: ...` manifest classifies by
        // the inner shape rather than as a bare lambda.
        assert!(call[6].contains("builtins.isFunction"));
        assert!(call[6].contains("f {}"));
        assert_eq!(call[7], EXTRA_FEATURES_FLAG);
        assert_eq!(call[8], EXTRA_FEATURES_VALUE);
    }

    #[test]
    fn validate_accepts_drv_shape() {
        let env = fake_fs();
        let abs = env.dotfiles_root.join("tools/packages.nix");
        let runner = StubRunner::ok("\"drv\"");
        NixCommand
            .validate(env.fs.as_ref(), &runner, &abs)
            .expect("drv shape should validate");
    }

    #[test]
    fn validate_rejects_set_shape_with_v1_message() {
        let env = fake_fs();
        let abs = env.dotfiles_root.join("tools/packages.nix");
        let runner = StubRunner::ok("\"set\"");
        let err = NixCommand
            .validate(env.fs.as_ref(), &runner, &abs)
            .expect_err("set shape is rejected in v1");
        let msg = format!("{err}");
        assert!(
            msg.contains("attribute set"),
            "error should name the rejected shape, got: {msg}"
        );
        assert!(
            msg.contains("list"),
            "error should point at the list-form workaround, got: {msg}"
        );
    }

    #[test]
    fn validate_rejects_unsupported_shape() {
        let env = fake_fs();
        let abs = env.dotfiles_root.join("tools/packages.nix");
        let runner = StubRunner::ok("\"unsupported\"");
        let err = NixCommand
            .validate(env.fs.as_ref(), &runner, &abs)
            .expect_err("unsupported shape must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("unsupported"),
            "error should mention unsupported, got: {msg}"
        );
    }

    #[test]
    fn validate_propagates_nix_eval_failure() {
        let env = fake_fs();
        let abs = env.dotfiles_root.join("tools/packages.nix");
        let runner = StubRunner::failing(1);
        let err = NixCommand
            .validate(env.fs.as_ref(), &runner, &abs)
            .expect_err("non-zero nix exit must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("nix eval"),
            "error should mention nix eval, got: {msg}"
        );
    }
}
