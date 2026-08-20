//! Nix handler — runs `nix profile install` against a wrapped
//! `packages.nix` once per content hash, via the shared
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
//! `packages.nix` evaluates to one of:
//!
//! - *List of derivations* — the canonical form.
//! - *Bare derivation* — common case for a one-tool pack.
//! - *Attribute set of derivations* — useful when a pack wants
//!   named attrs for tooling outside dodot.
//!
//! All three are recommended to use the `{ pkgs ? import <nixpkgs>
//! {} }:` function wrapper with a default argument so the manifest
//! is self-contained and can resolve `pkgs` from the user's
//! `NIX_PATH`.
//!
//! # Shape-agnostic install
//!
//! Unlike `nix profile install --file <path>`, which requires a
//! `'.*'` selector for attribute-set manifests and bare-form for
//! lists / derivations, the handler invokes `nix profile install`
//! with a single shape-normalizing **wrapper expression** — see
//! [`WRAPPER_EXPR`]. The wrapper takes the manifest path as a Nix
//! function argument, imports it, applies the outer function with
//! defaults if present, and collapses list / derivation /
//! attribute-set shapes to a single list of derivations Nix can
//! install in one form.
//!
//! That keeps the install command identical for every manifest
//! shape and removes any need for dodot to classify the manifest
//! at planning time. Malformed content (syntax errors, unsupported
//! shapes, missing `pkgs`) surfaces at apply time as a `nix`-side
//! error, the same way a broken `Brewfile` surfaces a
//! `brew bundle` error — see the *Lifecycle invariant* section of
//! [`RunOnceCommand`](crate::handlers::run_once::RunOnceCommand)
//! for why dodot deliberately avoids planning-time content
//! validation for run-once handlers.
//!
//! # The manifest travels as an argument, not as text
//!
//! `--argstr manifest <abs path>` hands the path to the wrapper as
//! a Nix string argument, which Nix auto-applies to the
//! expression's `{ manifest }` formal. Two things follow from that
//! choice, and both are the reason for it:
//!
//! - The path is a plain element of the command's arguments, so the
//!   descriptor in [`crate::provisioners`] can name its position and
//!   the datastore can snapshot the manifest and print its name in
//!   the run's progress header. Interpolating the path into the
//!   expression instead left nix as the one provisioner whose
//!   command never names its own manifest.
//! - Nothing has to escape the path into Nix source. A path
//!   containing a quote, a backslash, or a `${` is just an argument.
//!
//! # Impure evaluation
//!
//! `nix profile install --expr` evaluates in *pure* mode, which
//! forbids reading an absolute path outside the store — both the
//! manifest itself and the `<nixpkgs>` its documented
//! `{ pkgs ? import <nixpkgs> {} }:` wrapper resolves from
//! `NIX_PATH`. Every manifest shape dodot documents therefore needs
//! `--impure`, which is why it is on every invocation. Verified
//! against Nix 2.35.2 (`nixos/nix:latest`, digest
//! `sha256:7a007c76…`): without the flag the install aborts with
//! *"access to absolute path … is forbidden in pure evaluation
//! mode"* before nix ever looks at the manifest.

use std::path::Path;

use crate::handlers::run_once::RunOnceCommand;
use crate::handlers::{ExecutionPhase, HANDLER_NIX};

/// Shape-normalizing Nix wrapper expression.
///
/// A function of one argument: `manifest`, the absolute path to
/// `packages.nix`, supplied at invocation time via
/// [`MANIFEST_ARG_NAME`] and auto-applied by Nix. The wrapper:
///
/// 1. `import`s the manifest.
/// 2. Applies the outer function with `{}` when present (this is
///    what makes `{ pkgs ? import <nixpkgs> {} }: ...` resolve
///    without dodot threading any argument).
/// 3. Collapses the resulting value to a list of derivations —
///    a list passes through unchanged; a bare derivation is
///    wrapped into a one-element list; an attribute set is
///    flattened via `builtins.attrValues`.
/// 4. Throws a clear error for any other shape.
///
/// `nix profile install --expr <expr>` against this expression
/// installs the resulting list directly with no selector. Same
/// expression, byte for byte, for every manifest and every
/// accepted shape.
const WRAPPER_EXPR: &str = r#"{ manifest }:
let
  raw = import manifest;
  m = if builtins.isFunction raw then raw {} else raw;
in
  if builtins.isList m then m
  else if builtins.isAttrs m && (m.type or null) == "derivation" then [ m ]
  else if builtins.isAttrs m then builtins.attrValues m
  else throw "packages.nix at ${manifest} evaluates to an unsupported shape (must be a list of derivations, a bare derivation, or an attribute set of derivations)""#;

/// Name of the Nix function argument [`WRAPPER_EXPR`] reads the
/// manifest path from, passed as `--argstr <name> <path>`.
const MANIFEST_ARG_NAME: &str = "manifest";

/// Forces impure evaluation. Required by every manifest shape dodot
/// documents — see the module-level *Impure evaluation* section.
const IMPURE_FLAG: &str = "--impure";

/// Defensive `--extra-experimental-features` argument passed on every
/// `nix` invocation. The flag is a no-op when the features are
/// already enabled in the user's `nix.conf`; it guards against the
/// case where a fresh Nix install hasn't opted into the new CLI yet.
const EXTRA_FEATURES_FLAG: &str = "--extra-experimental-features";
const EXTRA_FEATURES_VALUE: &str = "nix-command flakes";

/// [`RunOnceCommand`] for the `nix` handler.
///
/// Triggers on `packages.nix` at pack root and invokes
/// `nix profile install --expr <wrapper>` once per content hash.
/// Inherits the three-state notify-don't-rerun policy from
/// [`RunOnceHandler`](crate::handlers::run_once::RunOnceHandler).
///
/// Carries no content-shape validation — see the module-level
/// "Shape-agnostic install" section and the lifecycle-invariant
/// note on [`RunOnceCommand`].
pub struct NixCommand;

impl RunOnceCommand for NixCommand {
    fn handler_name(&self) -> &str {
        HANDLER_NIX
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Provision
    }

    /// `nix profile install --impure --extra-experimental-features
    /// <features> --argstr manifest <path> --expr <wrapper>`.
    ///
    /// The manifest path is argument 7. That position is declared in
    /// [`crate::provisioners::PROVISIONERS`] and pinned by a test
    /// there; reordering these arguments means updating the row.
    fn command_for(&self, path: &Path) -> (String, Vec<String>) {
        (
            "nix".into(),
            vec![
                "profile".into(),
                "install".into(),
                IMPURE_FLAG.into(),
                EXTRA_FEATURES_FLAG.into(),
                EXTRA_FEATURES_VALUE.into(),
                "--argstr".into(),
                MANIFEST_ARG_NAME.into(),
                path.to_string_lossy().into_owned(),
                "--expr".into(),
                WRAPPER_EXPR.into(),
            ],
        )
    }

    // No `validate` override — see the lifecycle-invariant note on
    // `RunOnceCommand`.

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Argument positions in the nix command, mirrored by the row in
    /// [`crate::provisioners::PROVISIONERS`].
    const MANIFEST_INDEX: usize = 7;

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
    fn command_for_emits_profile_install_with_wrapper_expression() {
        let (exe, args) = NixCommand.command_for(Path::new("/p/tools/packages.nix"));
        assert_eq!(exe, "nix");
        assert_eq!(
            args,
            vec![
                "profile",
                "install",
                "--impure",
                "--extra-experimental-features",
                "nix-command flakes",
                "--argstr",
                "manifest",
                "/p/tools/packages.nix",
                "--expr",
                WRAPPER_EXPR,
            ]
        );
    }

    #[test]
    fn wrapper_expression_takes_the_manifest_as_an_argument() {
        // The expression is a function of `manifest` and never names
        // a path itself — that is what lets the path travel as an
        // ordinary argument the descriptor can point at, and what
        // removes any need to escape it into Nix source.
        assert!(WRAPPER_EXPR.starts_with("{ manifest }:"));
        assert!(WRAPPER_EXPR.contains("import manifest"));
        assert!(WRAPPER_EXPR.contains("builtins.isFunction"));
        assert!(WRAPPER_EXPR.contains("builtins.isList"));
        assert!(WRAPPER_EXPR.contains("builtins.attrValues"));
    }

    #[test]
    fn manifest_path_is_passed_verbatim_however_it_is_spelled() {
        // Quotes, backslashes, and `${` used to need escaping into a
        // Nix string literal. As an argument, a path is just bytes.
        for path in [
            "/p/tools/packages.nix",
            "/weird\"name/packages.nix",
            "/with\\backslash/packages.nix",
            "/p/${weird}/packages.nix",
            "/p/with space/packages.nix",
        ] {
            let (_, args) = NixCommand.command_for(Path::new(path));
            assert_eq!(args[MANIFEST_INDEX], path);
        }
    }

    #[test]
    fn command_for_is_shape_agnostic() {
        // Same handler, different manifest paths — same command
        // shape every time, differing only in the manifest argument.
        // There is no per-content branching at planning time. (This
        // is the property the lifecycle invariant on RunOnceCommand
        // depends on for nix.)
        let (e1, a1) = NixCommand.command_for(Path::new("/a/packages.nix"));
        let (e2, a2) = NixCommand.command_for(Path::new("/b/packages.nix"));
        assert_eq!(e1, e2);
        for (i, (x, y)) in a1.iter().zip(a2.iter()).enumerate() {
            if i == MANIFEST_INDEX {
                assert_ne!(x, y);
            } else {
                assert_eq!(x, y);
            }
        }
        assert_eq!(a1.len(), a2.len());
    }

    #[test]
    fn every_invocation_forces_impure_evaluation() {
        // `nix profile install --expr` evaluates in pure mode, which
        // rejects both the absolute manifest path and the `<nixpkgs>`
        // every documented manifest shape resolves from NIX_PATH.
        let (_, args) = NixCommand.command_for(Path::new("/p/tools/packages.nix"));
        assert!(args.iter().any(|a| a == "--impure"));
    }

    #[test]
    fn the_command_is_the_whole_specialization() {
        // Per the RunOnceCommand lifecycle invariant: nix does not
        // gatekeep planning on manifest content, and the "is nix
        // installed?" question is not a handler method at all — it is
        // `provisioners::availability`, which the planner asks. What
        // is left here is the command and its copy.
        use crate::provisioners::{descriptor_for, ExecutableLocation};
        let ExecutableLocation::Candidates(candidates) =
            descriptor_for(HANDLER_NIX).unwrap().location
        else {
            panic!("nix is located at fixed candidates");
        };
        assert!(candidates.len() > 1);
    }
}
