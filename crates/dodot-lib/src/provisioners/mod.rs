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
//! # Keeping the rows honest
//!
//! A declared index is a claim about argv that lives in a different
//! file from the code that builds argv. `descriptor_matches_command`
//! in this module's tests pins every row against the argv its
//! handler's [`RunOnceCommand::command_for`](crate::handlers::run_once::RunOnceCommand::command_for)
//! actually produces, so the two cannot drift apart silently.
//!
//! The same guard covers the candidate paths: `homebrew`'s rows are
//! pinned against [`crate::shell::homebrew::DEFAULT_PREFIXES`], so
//! the provisioner probe and the shell bootstrap cannot come to
//! disagree about where brew lives.
//!
//! # Where the executable is
//!
//! [`ProvisionerDescriptor::location`] states how dodot resolves the
//! program a row runs, and [`availability`] turns that into the
//! present / absent / probe-failed answer both `dodot up`'s planner
//! and `dodot status` read. See
//! `docs/adr/0007-locate-a-provisioner-at-fixed-paths.md`.

pub mod availability;

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

/// Where dodot looks for the program a provisioning row runs.
///
/// The field names *how to resolve* the executable, not the
/// executable: `install` picks its interpreter from the matched
/// file's extension, so its program is a property of the file rather
/// than of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutableLocation {
    /// An ordered list of absolute candidate paths. The first that is
    /// a regular file carrying an execute bit wins, and is the path
    /// the run spawns; the probe stats and never spawns itself. This
    /// is how `homebrew` and `nix` are found, and it is what lets an
    /// absent manager become a skip that names the locations it
    /// looked in.
    Candidates(&'static [CandidatePath]),
    /// Resolved by the OS through `PATH` at spawn time, with no
    /// pre-flight probe — the `install` exception.
    ///
    /// `install` spawns the bare `bash` or `zsh` its script's
    /// extension selects. The trust argument for fixed candidates is
    /// about a *pack* choosing which executable runs, and a pack
    /// cannot choose your shell: shells legitimately live in `/bin`,
    /// `/usr/bin`, `/opt/homebrew/bin`, and nix profiles, and dodot
    /// enumerating that list would refuse to run scripts on hosts
    /// where the interpreter is sitting right there. See
    /// `docs/adr/0007-locate-a-provisioner-at-fixed-paths.md`.
    Path,
}

/// One place a provisioner's executable may live, before the caller's
/// home directory and environment are known.
///
/// Resolved into a concrete absolute path by
/// [`availability::ProvisionHost::detect`]. Every variant produces an
/// absolute path or nothing — there is no relative candidate and no
/// `PATH` search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidatePath {
    /// An absolute path, taken verbatim.
    Absolute(&'static str),
    /// `<home>/<suffix>`, against the caller's
    /// [`Pather::home_dir`](crate::paths::Pather::home_dir) rather
    /// than a fresh `$HOME` reading — an embedder or an isolated test
    /// that supplies its own home gets *that* user's paths probed.
    UnderHome(&'static str),
    /// `<$var>/<suffix>`, dropped when the variable is unset or
    /// empty. The user's own record of where they put a manager;
    /// dodot has no other. A stale value does not short-circuit —
    /// the candidate simply fails to hold an executable and the next
    /// one gets its turn.
    UnderEnv {
        var: &'static str,
        suffix: &'static str,
    },
}

/// One provisioning handler, as data.
///
/// Seeded with the fields the datastore needs and the two the
/// availability probe needs. The remaining fields the
/// provisioning-handlers Spec calls for — argv template,
/// environment, status copy, reachability — land in later
/// workstreams of the same epic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProvisionerDescriptor {
    /// Handler name, matching
    /// [`RunOnceCommand::handler_name`](crate::handlers::run_once::RunOnceCommand::handler_name).
    pub handler: &'static str,
    /// Where the manifest path sits in the command's arguments.
    pub manifest_arg: ManifestArgPosition,
    /// How dodot resolves the program this row runs.
    pub location: ExecutableLocation,
    /// The manager's canonical project page — the *only* install
    /// instruction dodot ever gives, and a compile-time constant
    /// precisely so it can never come from configuration, from a
    /// pack, or from the network. dodot does not run a third-party
    /// installer and does not offer to; see
    /// `docs/adr/0009-never-execute-a-third-party-installer.md`.
    ///
    /// `None` for `install`, which has no manager to install.
    pub project_url: Option<&'static str>,
}

/// Where `brew` lives, in probe order — the Homebrew shell
/// bootstrap's prefix list with `bin/brew` appended to each entry.
///
/// Deliberately the same list as
/// [`crate::shell::homebrew::DEFAULT_PREFIXES`] plus the
/// `$HOMEBREW_PREFIX` override and the per-user
/// [`HOME_RELATIVE_PREFIX`](crate::shell::homebrew::HOME_RELATIVE_PREFIX)
/// fallback: a host whose brew the bootstrap emits a block for is a
/// host whose `Brewfile` must run, and the reverse. The two lists are
/// pinned against each other by
/// `tests::homebrew_candidates_track_the_bootstrap_prefixes`.
pub const HOMEBREW_CANDIDATES: &[CandidatePath] = &[
    CandidatePath::UnderEnv {
        var: "HOMEBREW_PREFIX",
        suffix: "bin/brew",
    },
    CandidatePath::Absolute("/opt/homebrew/bin/brew"),
    CandidatePath::Absolute("/home/linuxbrew/.linuxbrew/bin/brew"),
    CandidatePath::Absolute("/usr/local/bin/brew"),
    CandidatePath::UnderHome(".linuxbrew/bin/brew"),
];

/// Where `nix` lives, in probe order.
///
/// Nix has no `$NIX_PREFIX` equivalent and its installers write to
/// fixed locations, so this list is enumerated outright. Verified
/// against `nixos/nix:latest` (Nix 2.35.2): the image holds
/// `/nix/var/nix/profiles/default/bin/nix` and a `~/.nix-profile`
/// symlink onto that same profile, and holds neither of the last two
/// entries — which is why they are here rather than assumed.
///
/// - `~/.nix-profile/bin/nix` — the per-user profile, and what a
///   login shell resolves `nix` to on both install flavours. It leads
///   because it is the nix the user's own shell would run.
/// - `/nix/var/nix/profiles/default/bin/nix` — the multi-user
///   (daemon) install's default profile, where the official installer
///   puts it.
/// - `~/.local/state/nix/profiles/profile/bin/nix` — the profile
///   location Nix ≥ 2.14 uses, which `~/.nix-profile` normally points
///   at; probed directly for a home whose symlink is missing.
/// - `/run/current-system/sw/bin/nix` — NixOS, where nix arrives with
///   the system closure rather than with a profile.
///
/// [`Fs::stat`](crate::fs::Fs::stat) follows symlinks, which every
/// one of these paths is: each resolves into `/nix/store`.
pub const NIX_CANDIDATES: &[CandidatePath] = &[
    CandidatePath::UnderHome(".nix-profile/bin/nix"),
    CandidatePath::Absolute("/nix/var/nix/profiles/default/bin/nix"),
    CandidatePath::UnderHome(".local/state/nix/profiles/profile/bin/nix"),
    CandidatePath::Absolute("/run/current-system/sw/bin/nix"),
];

/// Homebrew's canonical project page.
pub const HOMEBREW_PROJECT_URL: &str = "https://brew.sh";

/// Nix's canonical download page.
pub const NIX_PROJECT_URL: &str = "https://nixos.org/download";

/// Every provisioning handler dodot ships.
///
/// The indices below mirror the `command_for` implementations in
/// [`crate::handlers::install`], [`crate::handlers::homebrew`], and
/// [`crate::handlers::nix`]:
///
/// - `install` — `<interpreter> -- <script>`
/// - `homebrew` — `brew bundle --file <Brewfile>`
/// - `nix` — `nix profile install --impure --extra-experimental-features
///   <features> --argstr manifest <packages.nix> --expr <wrapper>`
pub const PROVISIONERS: &[ProvisionerDescriptor] = &[
    ProvisionerDescriptor {
        handler: HANDLER_INSTALL,
        manifest_arg: ManifestArgPosition::at(1),
        location: ExecutableLocation::Path,
        project_url: None,
    },
    ProvisionerDescriptor {
        handler: HANDLER_HOMEBREW,
        manifest_arg: ManifestArgPosition::at(2),
        location: ExecutableLocation::Candidates(HOMEBREW_CANDIDATES),
        project_url: Some(HOMEBREW_PROJECT_URL),
    },
    ProvisionerDescriptor {
        handler: HANDLER_NIX,
        manifest_arg: ManifestArgPosition::at(7),
        location: ExecutableLocation::Candidates(NIX_CANDIDATES),
        project_url: Some(NIX_PROJECT_URL),
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
    fn descriptor_for_is_none_outside_provisioning() {
        assert!(descriptor_for("symlink").is_none());
        assert!(descriptor_for("shell").is_none());
        assert!(descriptor_for("external").is_none());
        assert!(descriptor_for("").is_none());
    }

    /// The guard that keeps the registry honest: every declared
    /// position is checked against the argv the handler actually
    /// builds. If a `command_for` grows or reorders an argument
    /// without its row moving with it, this fails.
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

    /// The candidate list and the shell bootstrap's prefix list are
    /// two statements about one fact: where brew is installed. A brew
    /// the bootstrap emits a block for must be a brew a `Brewfile`
    /// runs against, so a prefix added to one list and not the other
    /// fails here rather than at a user's next `dodot up`.
    #[test]
    fn homebrew_candidates_track_the_bootstrap_prefixes() {
        use crate::shell::homebrew::{DEFAULT_PREFIXES, HOME_RELATIVE_PREFIX};

        let mut expected: Vec<String> = vec!["$HOMEBREW_PREFIX/bin/brew".to_string()];
        expected.extend(DEFAULT_PREFIXES.iter().map(|p| format!("{p}/bin/brew")));
        expected.push(format!("~/{HOME_RELATIVE_PREFIX}/bin/brew"));

        let actual: Vec<String> = HOMEBREW_CANDIDATES
            .iter()
            .map(|c| match c {
                CandidatePath::Absolute(p) => (*p).to_string(),
                CandidatePath::UnderHome(suffix) => format!("~/{suffix}"),
                CandidatePath::UnderEnv { var, suffix } => format!("${var}/{suffix}"),
            })
            .collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn every_probed_candidate_is_absolute_or_anchored() {
        // No relative path, and nothing that could turn into a PATH
        // lookup: a candidate is either absolute or anchored to the
        // caller's home / an explicit environment variable.
        for descriptor in PROVISIONERS {
            let ExecutableLocation::Candidates(candidates) = descriptor.location else {
                continue;
            };
            assert!(
                !candidates.is_empty(),
                "{} declares Candidates with an empty list, which probes as present",
                descriptor.handler
            );
            for candidate in candidates {
                match candidate {
                    CandidatePath::Absolute(p) => {
                        assert!(Path::new(p).is_absolute(), "{p} is not an absolute path")
                    }
                    CandidatePath::UnderHome(suffix) | CandidatePath::UnderEnv { suffix, .. } => {
                        assert!(
                            !Path::new(suffix).is_absolute(),
                            "{suffix} is anchored and must not also be absolute"
                        )
                    }
                }
            }
        }
    }

    /// dodot names the manager and prints its project page; it never
    /// runs an installer. Every row that can report absence therefore
    /// has a URL to print, and `install` — which has no manager —
    /// has none.
    #[test]
    fn every_probed_provisioner_names_its_project_page() {
        for descriptor in PROVISIONERS {
            match descriptor.location {
                ExecutableLocation::Candidates(_) => assert!(
                    descriptor.project_url.is_some(),
                    "{} can report absence and must name where to get it",
                    descriptor.handler
                ),
                ExecutableLocation::Path => {
                    assert_eq!(descriptor.project_url, None, "{}", descriptor.handler)
                }
            }
        }
    }

    #[test]
    fn install_is_the_path_exception() {
        assert_eq!(
            descriptor_for(HANDLER_INSTALL).unwrap().location,
            ExecutableLocation::Path
        );
    }

    #[test]
    fn short_arguments_resolve_to_none() {
        let arguments = vec!["--".to_string()];
        assert_eq!(manifest_argument(HANDLER_INSTALL, &arguments), None);
        assert_eq!(manifest_argument(HANDLER_INSTALL, &[]), None);
    }
}
