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
//! [`WRAPPER_EXPR_TEMPLATE`]. The wrapper imports the manifest,
//! applies the outer function with defaults if present, and
//! collapses list / derivation / attribute-set shapes to a single
//! list of derivations Nix can install in one form.
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

use std::path::Path;

use crate::handlers::run_once::RunOnceCommand;
use crate::handlers::{ExecutionPhase, HANDLER_NIX};

/// Shape-normalizing Nix wrapper expression. `@PATH@` is replaced
/// at command-build time with the absolute path to `packages.nix`,
/// quoted as a Nix string literal so paths with hyphens, dots, or
/// spaces are unambiguous.
///
/// The wrapper:
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
/// command for every accepted shape.
const WRAPPER_EXPR_TEMPLATE: &str = r#"let
  raw = import @PATH@;
  m = if builtins.isFunction raw then raw {} else raw;
in
  if builtins.isList m then m
  else if builtins.isAttrs m && (m.type or null) == "derivation" then [ m ]
  else if builtins.isAttrs m then builtins.attrValues m
  else throw "packages.nix at @PATH@ evaluates to an unsupported shape (must be a list of derivations, a bare derivation, or an attribute set of derivations)""#;

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

    fn command_for(&self, path: &Path) -> (String, Vec<String>) {
        let expr = WRAPPER_EXPR_TEMPLATE.replace("@PATH@", &nix_path_literal(path));
        (
            "nix".into(),
            vec![
                "profile".into(),
                "install".into(),
                "--expr".into(),
                expr,
                EXTRA_FEATURES_FLAG.into(),
                EXTRA_FEATURES_VALUE.into(),
            ],
        )
    }

    // No `validate` override — see lifecycle-invariant note on
    // RunOnceCommand. Content-shape checks at planning time would
    // diverge nix from install / homebrew. Malformed manifests
    // surface at apply time via the `nix profile install` subprocess
    // exit code and stderr, the same way a broken Brewfile does.

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

/// Render an absolute path as a Nix double-quoted string literal,
/// escaping backslash and double-quote. Paths in practice cannot
/// contain a newline (the absolute path comes from a filesystem
/// walk), so the two-character escape set is sufficient.
fn nix_path_literal(path: &Path) -> String {
    let s = path.to_string_lossy();
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(args[0], "profile");
        assert_eq!(args[1], "install");
        assert_eq!(args[2], "--expr");
        // The wrapper expression embeds the path as a Nix string
        // literal and contains the shape-collapse branches.
        let expr = &args[3];
        assert!(
            expr.contains("import \"/p/tools/packages.nix\""),
            "expr should import the manifest path as a Nix string: {expr}"
        );
        assert!(expr.contains("builtins.isFunction"));
        assert!(expr.contains("builtins.isList"));
        assert!(expr.contains("builtins.attrValues"));
        assert_eq!(args[4], EXTRA_FEATURES_FLAG);
        assert_eq!(args[5], EXTRA_FEATURES_VALUE);
    }

    #[test]
    fn command_for_is_shape_agnostic() {
        // Same handler, different manifest paths — same command
        // shape every time. There is no per-content branching at
        // planning time. (This is the property the lifecycle
        // invariant on RunOnceCommand depends on for nix.)
        let (e1, a1) = NixCommand.command_for(Path::new("/a/packages.nix"));
        let (e2, a2) = NixCommand.command_for(Path::new("/b/packages.nix"));
        assert_eq!(e1, e2);
        assert_eq!(a1.len(), a2.len());
        // The argv structure is identical — only the path inside
        // the wrapper expression differs.
        assert_eq!(a1[0], a2[0]); // "profile"
        assert_eq!(a1[1], a2[1]); // "install"
        assert_eq!(a1[2], a2[2]); // "--expr"
        assert_eq!(a1[4], a2[4]); // features flag
        assert_eq!(a1[5], a2[5]); // features value
    }

    #[test]
    fn nix_path_literal_quotes_and_escapes() {
        assert_eq!(nix_path_literal(Path::new("/a/b.nix")), "\"/a/b.nix\"");
        // Embedded double-quote — escaped.
        assert_eq!(
            nix_path_literal(Path::new("/weird\"name.nix")),
            "\"/weird\\\"name.nix\""
        );
        // Embedded backslash — escaped.
        assert_eq!(
            nix_path_literal(Path::new("/with\\backslash.nix")),
            "\"/with\\\\backslash.nix\""
        );
    }

    #[test]
    fn validate_is_a_noop_inheriting_the_trait_default() {
        // Per the RunOnceCommand lifecycle invariant: nix does not
        // gatekeep planning on manifest content. validate uses the
        // trait's default no-op implementation; malformed content
        // surfaces at apply time.
        use crate::datastore::CommandRunner;
        use crate::testing::TempEnvironment;
        struct NeverCalledRunner;
        impl CommandRunner for NeverCalledRunner {
            fn run(
                &self,
                _e: &str,
                _a: &[String],
            ) -> crate::Result<crate::datastore::CommandOutput> {
                panic!("validate must not shell out — it's a no-op");
            }
        }
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("packages.nix", "anything at all — content is not checked")
            .done()
            .build();
        let abs = env.dotfiles_root.join("tools/packages.nix");
        NixCommand
            .validate(env.fs.as_ref(), &NeverCalledRunner, &abs)
            .expect("validate is a no-op for nix");
    }
}
