//! Is the manager there? — the presence probe, and the three
//! outcomes every caller reads.
//!
//! A provisioning handler used to spawn its manager and find out the
//! hard way. On a host without brew, `brew bundle` failed, the error
//! left the intent loop, and the pack lost the symlinks and `$PATH`
//! entries it had already earned — for the entirely ordinary
//! condition of not having Homebrew installed. Asking first turns
//! that into a skip.
//!
//! # What the probe does, and what it must never do
//!
//! It resolves the descriptor's ordered
//! [`CandidatePath`](crate::provisioners::CandidatePath) list to
//! absolute paths and tests each one for a regular file carrying an
//! execute bit — [`Fs::stat`] and mode bits, nothing else. No `PATH`
//! lookup, and **no subprocess**.
//!
//! The no-subprocess half is a hard contract, not an optimization.
//! `dodot status` is a passive command: it builds its context with a
//! [`NoopCommandRunner`](crate::datastore::NoopCommandRunner), and
//! `commands/tests/gating.rs` pins its datastore as byte-identical
//! after a run. Status reaches this module on every provisioning row,
//! so anything here that needed the tool to *answer* would break that
//! posture. Asking a manager whether it is well enough to use — a
//! version floor, a working `brew --version` — is
//! [`fitness`](crate::provisioners::fitness), which spawns and is
//! therefore `up`-only.
//!
//! # One probe, two callers
//!
//! Availability is not a handler method, and that is the whole point.
//! A finding produced inside intent generation dies with the intent:
//! [`commands::status::run_once_health`](crate::commands::status)
//! derives a row from the source file and the datastore and never
//! sees it, so an absent manager would render as `never ran` —
//! indistinguishable from one nobody has run yet. Both
//! [`plan_pack`](crate::packs::orchestration::plan_pack) and status
//! call [`probe`] here instead, with the same inputs, so the two
//! agree by construction rather than by review.
//!
//! # Ephemeral, never a receipt
//!
//! An [`Availability`] is a fact about this machine right now. It is
//! carried on a plan and on a status row and is never written to the
//! datastore. A receipt says *this exact file ran successfully*;
//! absence says *the tool is not here*, and conflating them would
//! mean an absent manager needed cleanup before it could run. Because
//! no receipt is written, installing the manager and re-running
//! `dodot up` runs the file — no flag, no stale state.
//!
//! Re-probing is a handful of `stat` calls, so nothing is cached
//! within a run: the probe stays a pure function of `(fs, host,
//! handler)`, which is what makes the planner and status agree
//! trivially. The cache belongs with
//! [`fitness`](crate::provisioners::fitness), where the cost is a
//! subprocess rather than a `stat`, and that is where it lives.
//!
//! See `docs/adr/0008-availability-is-three-outcomes-one-probe.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::fs::Fs;
use crate::provisioners::{descriptor_for, CandidatePath, ExecutableLocation, PROVISIONERS};

/// Whether a provisioner's executable is on this machine.
///
/// Three outcomes, deliberately: absence and failure-to-look are
/// different events with different remedies, and collapsing them
/// would report a permission error on `/opt/homebrew` as "Homebrew is
/// not installed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// The executable is there — proceed.
    ///
    /// `at` names the candidate that answered, or `None` for a
    /// provisioner dodot does not locate itself
    /// ([`ExecutableLocation::Path`]).
    ///
    /// The planner spawns that path: probing one brew and running
    /// whichever brew `PATH` resolves later would make the answer
    /// worthless on exactly the hosts the probe exists for — a
    /// manager installed at a fixed prefix the user's `PATH` omits.
    ///
    /// One exception, and it is a property of the intent rather than
    /// of the answer: an intent's executable is a `String`, so a
    /// candidate whose path is not valid UTF-8 cannot be carried
    /// through to the spawn. `plan_pack` leaves the handler's own
    /// name there and warns, rather than converting lossily into a
    /// path that names no file.
    Present { at: Option<PathBuf> },
    /// No candidate held an executable file. Skip: no receipt, no
    /// error, no effect on the exit code.
    ///
    /// `probed` carries every location that was tested, in order.
    /// Without it, "not installed" and "installed somewhere dodot
    /// does not look" are indistinguishable and the user has nothing
    /// to act on.
    Absent { probed: Vec<PathBuf> },
    /// A candidate could not be tested — a permission error on a
    /// parent directory, an I/O failure. A real error, surfaced with
    /// its detail and never silently absorbed as absence.
    ProbeFailed { at: PathBuf, detail: String },
}

/// How a non-[`Present`](Availability::Present) outcome renders.
///
/// Built here so the two renderers that place these rows — `dodot
/// up --dry-run`'s per-intent listing and `dodot status`'s
/// [`Health`](crate::commands::status) rows — cannot come to word
/// the same machine state differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableRow {
    /// Status style for the row: `skipped` for absence (the run
    /// attempted nothing and nothing is wrong), `broken` for a probe
    /// failure (dodot could not answer the question it was asked).
    pub style: &'static str,
    /// Short status-column text.
    pub label: String,
    /// Footnote body — where dodot looked, and what to do about it.
    pub note: String,
    /// Footnote severity: `warning` for absence, `error` for a probe
    /// failure. A warning must not present as a current error.
    pub note_kind: &'static str,
}

impl Availability {
    /// Whether the run may proceed to the manager.
    pub fn is_present(&self) -> bool {
        matches!(self, Availability::Present { .. })
    }

    /// The row this outcome renders as, or `None` when the manager is
    /// present and the row belongs to the ordinary run-once states.
    ///
    /// `handler` names the manager in the copy — `homebrew`, `nix` —
    /// which is also the handler column's own vocabulary, so a row
    /// reads as one statement rather than two.
    pub fn unavailable_row(&self, handler: &str) -> Option<UnavailableRow> {
        match self {
            Availability::Present { .. } => None,
            Availability::Absent { probed } => Some(UnavailableRow {
                style: "skipped",
                label: format!("{handler} not installed"),
                note: absent_note(handler, probed),
                note_kind: "warning",
            }),
            Availability::ProbeFailed { at, detail } => Some(UnavailableRow {
                style: "broken",
                label: format!("cannot probe {handler}"),
                note: format!(
                    "could not check whether {handler} is installed: {} ({detail}). \
                     Nothing was run for this file.",
                    at.display()
                ),
                note_kind: "error",
            }),
        }
    }
}

impl Availability {
    /// `probed A, B, C` — the locations half of the copy on its own.
    ///
    /// For rows whose verdict comes from a *receipt* rather than from
    /// this answer, where [`Availability::unavailable_row`]'s remedy
    /// ("nothing was recorded, so the next `dodot up` runs this file")
    /// would be false: something was recorded. Those rows say where
    /// dodot looked and word the rest themselves. `None` for any
    /// outcome but [`Availability::Absent`].
    pub fn probed_locations(&self) -> Option<String> {
        match self {
            Availability::Absent { probed } => Some(format!("probed {}", locations(probed))),
            _ => None,
        }
    }
}

/// Comma-separated candidate paths, in probe order.
fn locations(probed: &[PathBuf]) -> String {
    probed
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Footnote body for an absent manager: where dodot looked, where to
/// get the manager, and what happens next.
///
/// The remedy is the reason the note exists — the label states the
/// condition and leaves the user nothing to do about it. dodot names
/// the manager and prints its project page and stops there: it does
/// not run a third-party installer and does not offer to. See
/// `docs/adr/0009-never-execute-a-third-party-installer.md`.
fn absent_note(handler: &str, probed: &[PathBuf]) -> String {
    let locations = locations(probed);
    let where_to_get = descriptor_for(handler)
        .and_then(|d| d.project_url)
        .map(|url| format!(" ({url})"))
        .unwrap_or_default();
    format!(
        "{handler} is not installed — probed {locations}. Nothing was recorded, so \
         installing {handler}{where_to_get} and re-running `dodot up` runs this file."
    )
}

/// Where this machine keeps its provisioners, resolved once.
///
/// Injected on [`ExecutionContext`](crate::packs::context::ExecutionContext)
/// rather than read from the process, for the same reason
/// [`ProbePolicy`](crate::shell::ProbePolicy) is: a test that has not
/// opted in must not be able to reach the developer's real
/// `/opt/homebrew`, or CI and a laptop would disagree about what a
/// `Brewfile` does.
#[derive(Debug, Clone, Default)]
pub struct ProvisionHost {
    /// Resolved candidates per handler, in probe order.
    ///
    /// A handler with no entry is one dodot does not locate, and
    /// probes as [`Availability::Present`] with no path: that is
    /// `install`'s `PATH` exception in production, and the whole map
    /// under [`ProvisionHost::assume_present`].
    located: HashMap<&'static str, Vec<PathBuf>>,
}

impl ProvisionHost {
    /// Snapshot the running host: every descriptor's candidate list,
    /// resolved against `home` and the process environment.
    ///
    /// `home` is the caller's
    /// [`Pather::home_dir`](crate::paths::Pather::home_dir), dodot's
    /// one source of truth for the home directory, rather than a
    /// fresh `$HOME` reading — so an embedder or an isolated test
    /// that supplies its own home gets that user's profiles probed,
    /// and a non-UTF-8 home survives.
    pub fn detect(home: &Path) -> Self {
        let mut located = HashMap::new();
        for descriptor in PROVISIONERS {
            let ExecutableLocation::Candidates(candidates) = descriptor.location else {
                continue;
            };
            located.insert(descriptor.handler, resolve_candidates(candidates, home));
        }
        Self { located }
    }

    /// A host that locates nothing, so every provisioner probes as
    /// present.
    ///
    /// The default for test contexts: probing is opt-in there, and a
    /// test that has not opted in behaves as it did before the probe
    /// existed instead of depending on what the machine running it
    /// happens to have installed.
    pub fn assume_present() -> Self {
        Self::default()
    }

    /// A host that looks for `handler` at exactly `candidates`.
    ///
    /// The opt-in for tests: point it at a temp directory and create
    /// (or don't create) an executable there to pin present, absent,
    /// or unreadable without touching the real machine.
    pub fn with_candidates(handler: &'static str, candidates: Vec<PathBuf>) -> Self {
        let mut located = HashMap::new();
        located.insert(handler, candidates);
        Self { located }
    }

    /// The ordered candidates for `handler`; empty when dodot does
    /// not locate this handler's executable.
    pub fn candidates(&self, handler: &str) -> &[PathBuf] {
        self.located
            .get(handler)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

/// Resolve compile-time candidate specs into absolute paths.
///
/// An unset or blank environment variable drops its candidate rather
/// than producing a garbage path; an empty `home` drops the
/// home-anchored ones the same way. Environment values are read as
/// [`OsString`](std::ffi::OsString), so a non-UTF-8 prefix keeps its
/// candidate.
///
/// A candidate that does not resolve to an absolute path is dropped,
/// which is [`CandidatePath`]'s stated contract enforced rather than
/// assumed. The two inputs that can break it come from outside the
/// registry: `$HOMEBREW_PREFIX` is whatever the user exported, and
/// `home` is whatever the caller supplied. A relative one would be
/// resolved by `stat` against the current working directory — so
/// dodot would probe, and then run, a `bin/brew` belonging to
/// whichever directory `dodot up` happened to be invoked from. That
/// is the pack-chooses-your-executable failure ADR-0007 exists to
/// prevent, arriving through the back door.
fn resolve_candidates(candidates: &[CandidatePath], home: &Path) -> Vec<PathBuf> {
    let mut resolved = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        match candidate {
            CandidatePath::Absolute(path) => resolved.push(PathBuf::from(path)),
            CandidatePath::UnderHome(suffix) => {
                if !home.as_os_str().is_empty() {
                    resolved.push(home.join(suffix));
                }
            }
            CandidatePath::UnderEnv { var, suffix } => {
                // `var_os`, not `var`: a prefix is a path, and a path
                // is bytes rather than text. `$HOMEBREW_PREFIX` on a
                // host whose directory name is not valid UTF-8 names a
                // real brew, and `var` would drop it silently — taking
                // the candidate with it and reporting the manager
                // absent from the one location the user configured.
                // The rest of this module preserves non-UTF-8 paths
                // (`home` arrives as an `OsStr`), and this was the one
                // place that did not.
                let Some(prefix) = std::env::var_os(var) else {
                    continue;
                };
                // Blank means unset here: an empty value would resolve
                // to a bare `/bin/brew`-style path the user never
                // meant, and a whitespace-only one is the same typo
                // wearing a space. Only a value dodot can read as text
                // can be trimmed; anything else is judged empty or not.
                let blank = prefix
                    .to_str()
                    .map_or_else(|| prefix.is_empty(), |v| v.trim().is_empty());
                if !blank {
                    resolved.push(PathBuf::from(prefix).join(suffix));
                }
            }
        }
    }
    resolved.retain(|candidate| candidate.is_absolute());
    resolved
}

/// Ask whether `handler`'s executable is on this machine.
///
/// Stats each candidate in order and returns at the first that is a
/// regular file with an execute bit. A candidate that is missing, a
/// directory, or a non-executable leftover is skipped so the next one
/// gets its turn; a candidate that cannot be *tested* stops the probe
/// with [`Availability::ProbeFailed`], because a host where dodot
/// cannot read `/opt/homebrew` has not answered the question.
///
/// Spawns nothing. See the module docs for why that is a contract.
pub fn probe(fs: &dyn Fs, host: &ProvisionHost, handler: &str) -> Availability {
    let candidates = host.candidates(handler);
    if candidates.is_empty() {
        return Availability::Present { at: None };
    }

    let mut probed = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        probed.push(candidate.clone());
        match fs.stat(candidate) {
            Ok(meta) if meta.is_file && meta.mode & 0o111 != 0 => {
                return Availability::Present {
                    at: Some(candidate.clone()),
                }
            }
            // There, but not something to run: a directory, or a
            // leftover with no execute bit. Not this host's brew.
            Ok(_) => continue,
            Err(e) if is_missing(&e) => continue,
            Err(e) => {
                return Availability::ProbeFailed {
                    at: candidate.clone(),
                    detail: e.to_string(),
                }
            }
        }
    }
    Availability::Absent { probed }
}

/// Whether an error means "nothing is at this path" rather than "this
/// path could not be examined".
///
/// `NotFound` is the ordinary miss. `NotADirectory` (`ENOTDIR`) is
/// the same miss wearing a different hat: a candidate like
/// `/usr/local/bin/brew` whose `bin` is a regular file reports "not a
/// directory", which is still just this host not having brew there.
/// The raw errno is checked alongside the kind because a platform
/// that does not map `ENOTDIR` onto `NotADirectory` still means it.
/// Anything else — a permission error, an I/O failure — is a probe
/// failure and stops the walk.
fn is_missing(error: &crate::DodotError) -> bool {
    /// Same value on Linux and macOS.
    const ENOTDIR: i32 = 20;
    match error {
        crate::DodotError::Fs { source, .. } => {
            matches!(
                source.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) || source.raw_os_error() == Some(ENOTDIR)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{DirEntry, FsMetadata};
    use crate::handlers::{HANDLER_HOMEBREW, HANDLER_INSTALL, HANDLER_NIX};
    use crate::DodotError;

    /// What a fake candidate path is.
    enum Entry {
        Executable,
        /// A regular file with no execute bit — a leftover, not a brew.
        Plain,
        Directory,
        /// `stat` fails with this kind.
        Fails(std::io::ErrorKind),
    }

    /// A filesystem that answers `stat` from a map and panics on
    /// everything else.
    ///
    /// The panics are the point: they pin the claim in this module's
    /// docs that the presence probe reads mode bits and touches
    /// nothing else. There is no `CommandRunner` here at all, which
    /// is the other half — a probe that spawned could not be built
    /// from these fixtures.
    struct FakeFs {
        entries: HashMap<PathBuf, Entry>,
    }

    impl FakeFs {
        fn new(entries: Vec<(&str, Entry)>) -> Self {
            Self {
                entries: entries
                    .into_iter()
                    .map(|(p, e)| (PathBuf::from(p), e))
                    .collect(),
            }
        }
    }

    impl Fs for FakeFs {
        fn stat(&self, path: &Path) -> crate::Result<FsMetadata> {
            let missing = || DodotError::Fs {
                path: path.to_path_buf(),
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            };
            match self.entries.get(path) {
                None => Err(missing()),
                Some(Entry::Executable) => Ok(FsMetadata {
                    is_file: true,
                    is_dir: false,
                    is_symlink: false,
                    len: 0,
                    mode: 0o755,
                }),
                Some(Entry::Plain) => Ok(FsMetadata {
                    is_file: true,
                    is_dir: false,
                    is_symlink: false,
                    len: 0,
                    mode: 0o644,
                }),
                Some(Entry::Directory) => Ok(FsMetadata {
                    is_file: false,
                    is_dir: true,
                    is_symlink: false,
                    len: 0,
                    mode: 0o755,
                }),
                Some(Entry::Fails(kind)) => Err(DodotError::Fs {
                    path: path.to_path_buf(),
                    source: std::io::Error::from(*kind),
                }),
            }
        }

        fn lstat(&self, _: &Path) -> crate::Result<FsMetadata> {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn open_read(&self, _: &Path) -> crate::Result<Box<dyn std::io::Read + Send + Sync>> {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn read_file(&self, _: &Path) -> crate::Result<Vec<u8>> {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn read_to_string(&self, _: &Path) -> crate::Result<String> {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn write_file(&self, _: &Path, _: &[u8]) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
        fn set_permissions(&self, _: &Path, _: u32) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
        fn mkdir_all(&self, _: &Path) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
        fn symlink(&self, _: &Path, _: &Path) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
        fn readlink(&self, _: &Path) -> crate::Result<PathBuf> {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn remove_file(&self, _: &Path) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
        fn remove_dir_all(&self, _: &Path) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
        fn exists(&self, _: &Path) -> bool {
            unimplemented!("the presence probe asks for mode bits, not existence")
        }
        fn is_symlink(&self, _: &Path) -> bool {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn is_dir(&self, _: &Path) -> bool {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn read_dir(&self, _: &Path) -> crate::Result<Vec<DirEntry>> {
            unimplemented!("the presence probe stats and does nothing else")
        }
        fn rename(&self, _: &Path, _: &Path) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
        fn copy_file(&self, _: &Path, _: &Path) -> crate::Result<()> {
            unimplemented!("the presence probe never writes")
        }
    }

    fn host(candidates: &[&str]) -> ProvisionHost {
        ProvisionHost::with_candidates(
            HANDLER_HOMEBREW,
            candidates.iter().map(PathBuf::from).collect(),
        )
    }

    #[test]
    fn present_at_the_first_candidate_holding_an_executable() {
        let fs = FakeFs::new(vec![
            ("/opt/homebrew/bin/brew", Entry::Executable),
            ("/usr/local/bin/brew", Entry::Executable),
        ]);
        assert_eq!(
            probe(
                &fs,
                &host(&["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]),
                HANDLER_HOMEBREW
            ),
            Availability::Present {
                at: Some(PathBuf::from("/opt/homebrew/bin/brew"))
            }
        );
    }

    #[test]
    fn a_candidate_that_is_not_runnable_lets_the_next_one_answer() {
        // A directory and a non-executable leftover are both "not
        // this host's brew", not "stop looking".
        let fs = FakeFs::new(vec![
            ("/opt/homebrew/bin/brew", Entry::Directory),
            ("/home/linuxbrew/.linuxbrew/bin/brew", Entry::Plain),
            ("/usr/local/bin/brew", Entry::Executable),
        ]);
        assert_eq!(
            probe(
                &fs,
                &host(&[
                    "/opt/homebrew/bin/brew",
                    "/home/linuxbrew/.linuxbrew/bin/brew",
                    "/usr/local/bin/brew",
                ]),
                HANDLER_HOMEBREW
            ),
            Availability::Present {
                at: Some(PathBuf::from("/usr/local/bin/brew"))
            }
        );
    }

    #[test]
    fn absent_names_every_location_probed_in_order() {
        let fs = FakeFs::new(vec![("/opt/homebrew/bin/brew", Entry::Plain)]);
        assert_eq!(
            probe(
                &fs,
                &host(&["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]),
                HANDLER_HOMEBREW
            ),
            Availability::Absent {
                probed: vec![
                    PathBuf::from("/opt/homebrew/bin/brew"),
                    PathBuf::from("/usr/local/bin/brew"),
                ]
            }
        );
    }

    #[test]
    fn a_candidate_that_cannot_be_examined_is_a_failure_not_an_absence() {
        // "I looked and it isn't there" and "I was not allowed to
        // look" are different machine states with different remedies.
        let fs = FakeFs::new(vec![(
            "/opt/homebrew/bin/brew",
            Entry::Fails(std::io::ErrorKind::PermissionDenied),
        )]);
        let outcome = probe(
            &fs,
            &host(&["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]),
            HANDLER_HOMEBREW,
        );
        match outcome {
            Availability::ProbeFailed { at, detail } => {
                assert_eq!(at, PathBuf::from("/opt/homebrew/bin/brew"));
                assert!(detail.contains("permission denied"), "detail: {detail}");
            }
            other => panic!("expected ProbeFailed, got {other:?}"),
        }
    }

    #[test]
    fn a_candidate_under_a_non_directory_is_a_miss() {
        // `/usr/local/bin` being a regular file makes `stat` answer
        // ENOTDIR. That is still just "no brew here".
        let fs = FakeFs::new(vec![(
            "/usr/local/bin/brew",
            Entry::Fails(std::io::ErrorKind::NotADirectory),
        )]);
        // Both spellings of the same errno reach the same verdict:
        // the kind, and the raw code a platform may report instead.
        assert!(is_missing(&DodotError::Fs {
            path: PathBuf::from("/usr/local/bin/brew"),
            source: std::io::Error::from_raw_os_error(20),
        }));
        assert!(matches!(
            probe(&fs, &host(&["/usr/local/bin/brew"]), HANDLER_HOMEBREW),
            Availability::Absent { .. }
        ));
    }

    #[test]
    fn a_handler_dodot_does_not_locate_is_present_with_no_path() {
        // `install`'s PATH exception: nothing to probe, so nothing is
        // skipped, and no candidate is named.
        let fs = FakeFs::new(vec![]);
        assert_eq!(
            probe(
                &fs,
                &ProvisionHost::detect(Path::new("/home/u")),
                HANDLER_INSTALL
            ),
            Availability::Present { at: None }
        );
    }

    #[test]
    fn a_test_context_that_has_not_opted_in_probes_nothing() {
        let fs = FakeFs::new(vec![]);
        for handler in [HANDLER_INSTALL, HANDLER_HOMEBREW, HANDLER_NIX] {
            assert_eq!(
                probe(&fs, &ProvisionHost::assume_present(), handler),
                Availability::Present { at: None },
                "{handler} must not reach the real machine from a test"
            );
        }
    }

    #[test]
    fn detect_anchors_home_candidates_to_the_callers_home() {
        let host = ProvisionHost::detect(Path::new("/home/ada"));
        assert!(host
            .candidates(HANDLER_NIX)
            .contains(&PathBuf::from("/home/ada/.nix-profile/bin/nix")));
        assert!(host
            .candidates(HANDLER_HOMEBREW)
            .contains(&PathBuf::from("/home/ada/.linuxbrew/bin/brew")));
    }

    #[test]
    fn detect_locates_the_managers_and_leaves_install_alone() {
        let host = ProvisionHost::detect(Path::new("/home/ada"));
        assert!(!host.candidates(HANDLER_HOMEBREW).is_empty());
        assert!(!host.candidates(HANDLER_NIX).is_empty());
        assert!(
            host.candidates(HANDLER_INSTALL).is_empty(),
            "install resolves its interpreter through PATH and is not located"
        );
    }

    #[test]
    fn an_empty_home_drops_home_anchored_candidates_rather_than_inventing_one() {
        let host = ProvisionHost::detect(Path::new(""));
        assert!(host
            .candidates(HANDLER_NIX)
            .iter()
            .all(|c| c.is_absolute() && !c.starts_with("/.nix-profile")));
    }

    #[test]
    fn an_unset_environment_variable_drops_its_candidate() {
        let resolved = resolve_candidates(
            &[
                CandidatePath::UnderEnv {
                    var: "DODOT_TEST_NO_SUCH_PREFIX",
                    suffix: "bin/brew",
                },
                CandidatePath::Absolute("/usr/local/bin/brew"),
            ],
            Path::new("/home/ada"),
        );
        assert_eq!(resolved, vec![PathBuf::from("/usr/local/bin/brew")]);
    }

    #[test]
    fn a_set_environment_variable_leads_the_candidate_list() {
        // `PATH` stands in for `$HOMEBREW_PREFIX` — it is always set,
        // so the test needs no environment mutation of its own.
        let expected = PathBuf::from(std::env::var_os("PATH").unwrap()).join("bin/brew");
        let resolved = resolve_candidates(
            &[
                CandidatePath::UnderEnv {
                    var: "PATH",
                    suffix: "bin/brew",
                },
                CandidatePath::Absolute("/usr/local/bin/brew"),
            ],
            Path::new("/home/ada"),
        );
        assert_eq!(resolved.first(), Some(&expected));
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_environment_prefix_keeps_its_candidate() {
        // A directory name is bytes. Reading the variable as text
        // dropped this prefix silently and reported the manager
        // absent from the one location the user had configured.
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let prefix = OsStr::from_bytes(b"/opt/br\xff/prefix");
        let _guard = crate::testing::EnvVarGuard::set_os("DODOT_TEST_ODD_PREFIX", prefix);

        let resolved = resolve_candidates(
            &[CandidatePath::UnderEnv {
                var: "DODOT_TEST_ODD_PREFIX",
                suffix: "bin/brew",
            }],
            Path::new("/home/ada"),
        );

        assert_eq!(resolved, vec![PathBuf::from(prefix).join("bin/brew")]);
    }

    #[test]
    fn a_blank_environment_variable_drops_its_candidate() {
        // Empty resolves to a bare `/bin/brew` the user never meant;
        // whitespace-only is the same typo wearing a space.
        for value in ["", "   "] {
            let _guard = crate::testing::EnvVarGuard::set("DODOT_TEST_BLANK_PREFIX", value);
            let resolved = resolve_candidates(
                &[
                    CandidatePath::UnderEnv {
                        var: "DODOT_TEST_BLANK_PREFIX",
                        suffix: "bin/brew",
                    },
                    CandidatePath::Absolute("/usr/local/bin/brew"),
                ],
                Path::new("/home/ada"),
            );
            assert_eq!(resolved, vec![PathBuf::from("/usr/local/bin/brew")]);
        }
    }

    #[test]
    fn a_relative_candidate_is_dropped_rather_than_resolved_against_the_cwd() {
        // `CandidatePath` promises an absolute path or nothing. The
        // two inputs that can break that promise arrive from outside
        // the registry — the user's own `$HOMEBREW_PREFIX` and the
        // caller's home — and a relative one would make `stat`, and
        // then the spawn, depend on where `dodot up` was invoked from.
        let _guard = crate::testing::EnvVarGuard::set("DODOT_TEST_RELATIVE_PREFIX", "opt/brew");
        let resolved = resolve_candidates(
            &[
                CandidatePath::UnderEnv {
                    var: "DODOT_TEST_RELATIVE_PREFIX",
                    suffix: "bin/brew",
                },
                CandidatePath::UnderHome(".linuxbrew/bin/brew"),
                CandidatePath::Absolute("/usr/local/bin/brew"),
            ],
            Path::new("relative/home"),
        );

        assert_eq!(resolved, vec![PathBuf::from("/usr/local/bin/brew")]);
    }

    #[test]
    fn an_absent_row_names_the_manager_the_locations_and_the_project_page() {
        let row = Availability::Absent {
            probed: vec![
                PathBuf::from("/opt/homebrew/bin/brew"),
                PathBuf::from("/usr/local/bin/brew"),
            ],
        }
        .unavailable_row(HANDLER_HOMEBREW)
        .expect("an absent manager renders a row");

        assert_eq!(row.style, "skipped");
        assert_eq!(row.label, "homebrew not installed");
        assert_eq!(row.note_kind, "warning");
        assert!(row.note.contains("/opt/homebrew/bin/brew"));
        assert!(row.note.contains("/usr/local/bin/brew"));
        assert!(row.note.contains(crate::provisioners::HOMEBREW_PROJECT_URL));
        assert!(row.note.contains("dodot up"));
    }

    #[test]
    fn a_probe_failure_renders_as_an_error_not_a_skip() {
        let row = Availability::ProbeFailed {
            at: PathBuf::from("/opt/homebrew/bin/brew"),
            detail: "permission denied".into(),
        }
        .unavailable_row(HANDLER_HOMEBREW)
        .expect("a probe failure renders a row");

        assert_eq!(row.style, "broken");
        assert_eq!(row.note_kind, "error");
        assert!(row.label.contains("homebrew"));
        assert!(row.note.contains("permission denied"));
    }

    #[test]
    fn a_row_with_a_receipt_gets_the_locations_without_the_wrong_remedy() {
        // "nothing was recorded, so the next `up` runs this file" is
        // false for a file that already ran, so those rows take the
        // locations and word the rest themselves.
        let absent = Availability::Absent {
            probed: vec![PathBuf::from("/opt/homebrew/bin/brew")],
        };
        assert_eq!(
            absent.probed_locations(),
            Some("probed /opt/homebrew/bin/brew".to_string())
        );
        assert_eq!(Availability::Present { at: None }.probed_locations(), None);
    }

    #[test]
    fn a_present_manager_renders_no_row_of_its_own() {
        assert!(Availability::Present { at: None }
            .unavailable_row(HANDLER_HOMEBREW)
            .is_none());
    }
}
