//! Compile-time descriptor registry for the provisioning handlers.
//!
//! `install`, `homebrew`, and `nix` differ from one another only in
//! the command they build and the copy they print. This module is
//! where that difference starts becoming *data*: one row per
//! provisioning handler, holding the handler's name and the position
//! of the manifest inside the command's arguments.
//!
//! # Why the manifest position is declared
//!
//! [`DataStore::run_and_record`](crate::datastore::DataStore::run_and_record)
//! has to know which file a provisioning command was pointed at: it
//! prints that filename in the run's progress header, echoes the
//! file's leading comment block, and snapshots its bytes next to the
//! sentinel so `dodot status --diff` can show what changed since the
//! run.
//!
//! It used to find that file by taking the command's *last*
//! argument. `install` (`bash -- <script>`) and `homebrew`
//! (`brew bundle --file <Brewfile>`) happen to satisfy that
//! convention; `nix` does not, because its command ends in
//! `--extra-experimental-features "nix-command flakes"`. So nix runs
//! took `"nix-command flakes"` for the manifest: they printed it as
//! the filename in the progress header, and — being unreadable as a
//! path — wrote no snapshot at all, which left `dodot status --diff`
//! permanently unavailable for `packages.nix`.
//!
//! A convention that two of three commands keep is not a contract.
//! Each descriptor states its manifest position outright, and
//! [`manifest_argument`] is the only way the datastore asks the
//! question.
//!
//! # Compile-time, never configuration
//!
//! These rows never come from
//! [`MappingsSection`](crate::config::MappingsSection). Mappings
//! resolve defaults → root → pack, so a pack's own `.dodot.toml`
//! overrides whatever they hold — and a descriptor describes an
//! executable and the arguments it receives. Which *filename* a
//! handler claims is ordinary user configuration and stays in
//! mappings; which *executable* runs, and how the manifest reaches
//! it, is not. See `docs/adr/0004-configuration-selects-files-never-executables.md`.
//!
//! # Why the environment is declared
//!
//! A provisioning command inherits dodot's environment, and until
//! this field existed that was all it could ever get. Homebrew needs one
//! variable set — `HOMEBREW_NO_AUTO_UPDATE`, so the first brew of the
//! day does not turn `dodot up` into a multi-second network update of
//! brew and its taps — and that variable belongs next to the argv it
//! qualifies, not in a branch at the spawn site. The descriptor's
//! `env` rows travel with the intent through
//! [`HandlerIntent::Run`](crate::operations::HandlerIntent::Run) and
//! [`Operation::RunCommand`](crate::operations::Operation::RunCommand)
//! to [`CommandRunner::run`](crate::datastore::CommandRunner::run),
//! which layers them onto the inherited environment. A handler with
//! no rows spawns exactly as it did before.
//!
//! # Keeping the rows honest
//!
//! A declared index is a claim about argv that lives in a different
//! file from the code that builds argv. `descriptor_matches_command`
//! in this module's tests pins every row against the argv its
//! handler's [`RunOnceCommand::command_for`](crate::handlers::run_once::RunOnceCommand::command_for)
//! actually produces, so the two cannot drift apart silently.

use crate::handlers::{HANDLER_HOMEBREW, HANDLER_INSTALL, HANDLER_NIX};

/// Which argument of a provisioning command carries the manifest —
/// the user's `install.sh`, `Brewfile`, or `packages.nix`.
///
/// A position, not a search: the handler that builds the arguments
/// states where it put the path, and callers read it from here
/// rather than guessing from argument shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestArgPosition(usize);

impl ManifestArgPosition {
    /// The manifest path is the argument at `index`, verbatim.
    pub const fn at(index: usize) -> Self {
        Self(index)
    }

    /// The declared index into the command's arguments.
    pub const fn index(self) -> usize {
        self.0
    }

    /// Read the manifest path out of a command's arguments.
    ///
    /// `None` when the arguments are shorter than the declared
    /// position — a mismatch between descriptor and command that the
    /// registry tests exist to prevent, and that callers treat the
    /// same way they treat a handler with no descriptor at all.
    pub fn resolve(self, arguments: &[String]) -> Option<&str> {
        arguments.get(self.0).map(String::as_str)
    }
}

/// One provisioning handler, as data.
///
/// Holds the fields the datastore and the spawn path need. The
/// remaining fields the provisioning-handlers Spec calls for —
/// availability, argv template, status copy, reachability — land in
/// later workstreams of the same epic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProvisionerDescriptor {
    /// Handler name, matching
    /// [`RunOnceCommand::handler_name`](crate::handlers::run_once::RunOnceCommand::handler_name).
    pub handler: &'static str,
    /// Environment variables set for this handler's command, layered
    /// onto the environment dodot itself runs with. Empty for a
    /// handler that needs nothing beyond what it inherits.
    pub env: &'static [(&'static str, &'static str)],
    /// Where the manifest path sits in the command's arguments.
    pub manifest_arg: ManifestArgPosition,
}

/// Every provisioning handler dodot ships.
///
/// The indices below mirror the `command_for` implementations in
/// [`crate::handlers::install`], [`crate::handlers::homebrew`], and
/// [`crate::handlers::nix`]:
///
/// - `install` — `<interpreter> -- <script>`
/// - `homebrew` — `brew bundle --no-upgrade --file <Brewfile>`
/// - `nix` — `nix profile install --impure --extra-experimental-features
///   <features> --argstr manifest <packages.nix> --expr <wrapper>`
pub const PROVISIONERS: &[ProvisionerDescriptor] = &[
    ProvisionerDescriptor {
        handler: HANDLER_INSTALL,
        env: &[],
        manifest_arg: ManifestArgPosition::at(1),
    },
    ProvisionerDescriptor {
        handler: HANDLER_HOMEBREW,
        // Without this, the first brew invocation of any day runs a
        // `brew update` first: several seconds of network traffic
        // upgrading brew and its taps, charged to whoever happened to
        // run `dodot up`. The Brewfile's own contents are what the
        // user asked dodot to apply.
        env: &[("HOMEBREW_NO_AUTO_UPDATE", "1")],
        manifest_arg: ManifestArgPosition::at(3),
    },
    ProvisionerDescriptor {
        handler: HANDLER_NIX,
        env: &[],
        manifest_arg: ManifestArgPosition::at(7),
    },
];

/// Look up a provisioning handler's descriptor by name.
///
/// `None` for every other handler — the symlink, shell, path, gate,
/// and external handlers do not run a manifest through a command and
/// have no row here.
pub fn descriptor_for(handler: &str) -> Option<&'static ProvisionerDescriptor> {
    PROVISIONERS.iter().find(|d| d.handler == handler)
}

/// The manifest path a provisioning command was pointed at.
///
/// `None` when `handler` has no descriptor, or when `arguments` is
/// too short for the declared position. Callers that use this for
/// display or for snapshotting treat `None` as "this run names no
/// manifest" and degrade rather than fail.
pub fn manifest_argument<'a>(handler: &str, arguments: &'a [String]) -> Option<&'a str> {
    descriptor_for(handler)?.manifest_arg.resolve(arguments)
}

/// The environment variables a provisioning handler's command runs
/// with, on top of the environment dodot inherited.
///
/// Empty for a handler with no descriptor and for a descriptor that
/// declares no variables — both mean "spawn with dodot's own
/// environment, unmodified".
pub fn environment_for(handler: &str) -> &'static [(&'static str, &'static str)] {
    descriptor_for(handler).map_or(&[], |d| d.env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::homebrew::BrewfileCommand;
    use crate::handlers::install::InstallCommand;
    use crate::handlers::nix::NixCommand;
    use crate::handlers::run_once::RunOnceCommand;
    use std::path::Path;

    #[test]
    fn registry_holds_one_row_per_provisioning_handler() {
        let names: Vec<&str> = PROVISIONERS.iter().map(|d| d.handler).collect();
        assert_eq!(names, vec![HANDLER_INSTALL, HANDLER_HOMEBREW, HANDLER_NIX]);
    }

    #[test]
    fn only_homebrew_declares_an_environment() {
        assert_eq!(environment_for(HANDLER_INSTALL), &[]);
        assert_eq!(environment_for(HANDLER_NIX), &[]);
        assert_eq!(
            environment_for(HANDLER_HOMEBREW),
            &[("HOMEBREW_NO_AUTO_UPDATE", "1")]
        );
    }

    #[test]
    fn a_handler_with_no_descriptor_declares_no_environment() {
        // Same answer as a descriptor with empty rows: spawn with
        // dodot's own environment, unmodified.
        assert_eq!(environment_for("symlink"), &[]);
        assert_eq!(environment_for(""), &[]);
    }

    #[test]
    fn descriptor_for_is_none_outside_provisioning() {
        assert!(descriptor_for("symlink").is_none());
        assert!(descriptor_for("shell").is_none());
        assert!(descriptor_for("external").is_none());
        assert!(descriptor_for("").is_none());
    }

    /// The guard that keeps the registry honest: every declared
    /// position is checked against the argv the handler actually
    /// builds. If a `command_for` grows or reorders an argument
    /// without its row moving with it, this fails — which is what
    /// `homebrew`'s `--no-upgrade` would otherwise have done
    /// silently.
    #[test]
    fn descriptor_matches_command() {
        let manifest = Path::new("/dotfiles/tools/manifest");
        let cases: Vec<(&str, Vec<String>)> = vec![
            (
                HANDLER_INSTALL,
                InstallCommand
                    .command_for(&manifest.with_file_name("install.sh"))
                    .1,
            ),
            (
                HANDLER_HOMEBREW,
                BrewfileCommand
                    .command_for(&manifest.with_file_name("Brewfile"))
                    .1,
            ),
            (
                HANDLER_NIX,
                NixCommand
                    .command_for(&manifest.with_file_name("packages.nix"))
                    .1,
            ),
        ];

        for (handler, arguments) in cases {
            let expected = manifest
                .with_file_name(match handler {
                    HANDLER_INSTALL => "install.sh",
                    HANDLER_HOMEBREW => "Brewfile",
                    _ => "packages.nix",
                })
                .to_string_lossy()
                .into_owned();
            assert_eq!(
                manifest_argument(handler, &arguments),
                Some(expected.as_str()),
                "descriptor for {handler} does not point at the manifest in {arguments:?}"
            );
        }
    }

    /// The other half of keeping the rows honest: each command's
    /// `environment()` is the descriptor's rows, so a variable
    /// declared here is a variable the handler actually spawns with.
    #[test]
    fn descriptor_environment_reaches_each_command() {
        assert!(InstallCommand.environment().is_empty());
        assert!(NixCommand.environment().is_empty());
        assert_eq!(
            BrewfileCommand.environment(),
            vec![("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())]
        );
    }

    #[test]
    fn nix_manifest_is_not_the_last_argument() {
        // The whole point of declaring the position: nix's command
        // ends in the wrapper expression, so the old
        // `arguments.last()` convention would have snapshotted that
        // instead of `packages.nix`.
        let (_, arguments) = NixCommand.command_for(Path::new("/dotfiles/tools/packages.nix"));
        assert_ne!(
            arguments.last().map(String::as_str),
            Some("/dotfiles/tools/packages.nix")
        );
        assert_eq!(
            manifest_argument(HANDLER_NIX, &arguments),
            Some("/dotfiles/tools/packages.nix")
        );
    }

    #[test]
    fn short_arguments_resolve_to_none() {
        let arguments = vec!["--".to_string()];
        assert_eq!(manifest_argument(HANDLER_INSTALL, &arguments), None);
        assert_eq!(manifest_argument(HANDLER_INSTALL, &[]), None);
    }
}
