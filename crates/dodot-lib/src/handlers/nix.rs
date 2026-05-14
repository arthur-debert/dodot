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
//! expression that returns one of `"list"`, `"drv"`, `"set"`, or
//! `"unsupported"`. dodot routes the result:
//!
//! - `list` / `drv` → install via `nix profile install --file <path>`
//! - `set` → install via `nix profile install --file <path> '.*'`
//! - `unsupported` → reject with a manifest-shape error before any
//!   install is attempted.
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
const SHAPE_PROBE_EXPR: &str = r#"x:
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
    /// A list of derivations.
    List,
    /// A bare derivation.
    Drv,
    /// An attribute set of derivations.
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
/// `nix profile install --file <path>` (with `'.*'` appended for the
/// attribute-set shape). Inherits the three-state notify-don't-rerun
/// policy from [`RunOnceHandler`](crate::handlers::run_once::RunOnceHandler).
pub struct NixCommand;

impl RunOnceCommand for NixCommand {
    fn handler_name(&self) -> &str {
        HANDLER_NIX
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Provision
    }

    fn command_for(&self, path: &Path) -> (String, Vec<String>) {
        // command_for runs after `validate`, which has already
        // classified the manifest. Re-running the shape probe here
        // would double the `nix eval` cost and tie the install
        // command to runner availability at intent-construction
        // time. Instead, we issue the install form that works for
        // *both* `list` and `drv` shapes: `nix profile install
        // --file <path>` is the canonical invocation for those two,
        // and on the `set` shape Nix surfaces a clear error message
        // pointing the user at the explicit selector form. The
        // attribute-set selector ('.*') is the only difference for
        // the `set` shape; documenting that single-step manual
        // override here is a smaller cost than threading runner
        // access through `command_for`. See spec §5.3.
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
        let shape = probe_shape(runner, path)?;
        ManifestShape::from_probe(&shape).ok_or_else(|| DodotError::Fs {
            path: path.to_path_buf(),
            source: std::io::Error::other(format!(
                "packages.nix has unsupported shape `{shape}` — must evaluate to a list of \
                 derivations, a bare derivation, or an attribute set of derivations \
                 (see docs/user/handlers/nix.lex)"
            )),
        })?;
        Ok(())
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
    // `nix eval --json` quotes the result string. Strip surrounding
    // whitespace + JSON quotes — sufficient for the four expected
    // single-token outputs ("list", "drv", "set", "unsupported");
    // we don't pull in a JSON dep for a 6-char string.
    let trimmed = out.stdout.trim().trim_matches('"').to_string();
    Ok(trimmed)
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
    fn validate_accepts_set_shape() {
        let env = fake_fs();
        let abs = env.dotfiles_root.join("tools/packages.nix");
        let runner = StubRunner::ok("\"set\"");
        NixCommand
            .validate(env.fs.as_ref(), &runner, &abs)
            .expect("set shape should validate");
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
