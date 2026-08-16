//! Authoritative environment-root selection (`DOTFILES_ROOT`).
//!
//! A present `DOTFILES_ROOT` is deliberate selection and is authoritative: an
//! unusable value is a hard error, never a fall-through to Git or the current
//! directory, so an explicit configuration mistake cannot quietly become a
//! different root (ADR-0002, Spec story 9).
//!
//! That guarantee is carried by the return type rather than by caller
//! discipline: implicit discovery is what a caller does with `Ok(None)`, and
//! no failing value can produce `Ok(None)`. Every present value is either a
//! root or an error.
//!
//! The variable is *injected*, not read here. The capture belongs to the
//! process boundary (WS07); this module is a pure function of its input so
//! every explicit-value case — empty, relative, non-Unicode, missing,
//! non-directory, unreadable — is testable without mutating a shared
//! environment. The invocation directory is injected for the same reason: a
//! relative value is anchored to *that* directory, never to whatever
//! directory the process happens to be in when the probe runs.
//!
//! Nothing here writes trust state. An environment root is invocation-local:
//! it authorizes this invocation because the user spelled it out, and is
//! never added to the approved-roots collection (ADR-0003).

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::error::{Result, SafetyLockError};
use super::roots::{ResolvedRoot, RootSource};
use super::util::{canonical_root_identity, encode_native_path, PathProbe, UnusableRoot};

/// The environment inputs root selection depends on, captured once per
/// invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRootInput {
    /// `DOTFILES_ROOT` exactly as the process environment carried it, or
    /// `None` when it is unset.
    ///
    /// An [`OsString`] rather than a `String`: a non-Unicode value is
    /// accepted when it resolves to a readable directory, so the raw bytes
    /// must survive capture.
    pub raw_value: Option<OsString>,

    /// The directory Dodot was invoked from, which a relative value is
    /// resolved against.
    ///
    /// Expected to be absolute — the caller captures it from the process. A
    /// relative value anchored to a relative invocation directory is refused
    /// rather than handed to the probe, because resolving it would silently
    /// depend on the process working directory this module deliberately does
    /// not read.
    pub current_dir: PathBuf,

    /// The home directory used to expand a leading `~`.
    pub home_dir: PathBuf,
}

impl EnvironmentRootInput {
    /// An input with `DOTFILES_ROOT` unset.
    pub fn unset(current_dir: impl Into<PathBuf>, home_dir: impl Into<PathBuf>) -> Self {
        Self {
            raw_value: None,
            current_dir: current_dir.into(),
            home_dir: home_dir.into(),
        }
    }

    /// An input carrying a captured `DOTFILES_ROOT` value.
    pub fn set(
        current_dir: impl Into<PathBuf>,
        home_dir: impl Into<PathBuf>,
        raw_value: impl Into<OsString>,
    ) -> Self {
        Self {
            raw_value: Some(raw_value.into()),
            current_dir: current_dir.into(),
            home_dir: home_dir.into(),
        }
    }

    /// Whether `DOTFILES_ROOT` was present at all — the question that decides
    /// whether implicit selection runs, independent of whether the value is
    /// usable.
    pub fn is_set(&self) -> bool {
        self.raw_value.is_some()
    }
}

/// Resolve the environment root, if `DOTFILES_ROOT` selected one.
///
/// A present value is expanded (a leading `~` against the injected home
/// directory, a relative path against the injected invocation directory),
/// validated as a readable directory, and canonicalized once. The native form
/// is preserved throughout, so a non-Unicode value keeps its bytes into the
/// resulting [`RootIdentity`] and into any diagnostic naming it.
///
/// Returns:
///
/// - `Ok(Some(root))` — a valid value, canonicalized, with source
///   [`RootSource::Environment`](super::roots::RootSource::Environment).
///   Selection produces at most this one invocation-local root; it is never
///   written to the approved-roots collection.
/// - `Ok(None)` — `DOTFILES_ROOT` is unset, so implicit selection applies.
/// - `Err(..)` — the value is present but unusable
///   ([`SafetyLockError::EnvironmentRootEmpty`](super::error::SafetyLockError::EnvironmentRootEmpty)
///   or
///   [`EnvironmentRootUnusable`](super::error::SafetyLockError::EnvironmentRootUnusable)).
///   The caller must fail rather than resolve another root.
pub fn resolve_environment_root(
    input: &EnvironmentRootInput,
    probe: &dyn PathProbe,
) -> Result<Option<ResolvedRoot>> {
    let Some(raw_value) = input.raw_value.as_deref() else {
        return Ok(None);
    };

    if raw_value.is_empty() {
        return Err(SafetyLockError::EnvironmentRootEmpty);
    }

    let expanded = expand_leading_tilde(raw_value, &input.home_dir);
    let candidate = if expanded.is_absolute() {
        expanded
    } else {
        input.current_dir.join(expanded)
    };

    // Naming the value the user set, plus the path it expanded to when the
    // two differ: `~/dots` and `dots` are only actionable once the reader
    // knows which directory they landed in.
    let unusable = |reason: String| SafetyLockError::EnvironmentRootUnusable {
        spelling: encode_native_path(Path::new(raw_value)),
        reason,
    };
    let named = if candidate.as_os_str() == raw_value {
        "it".to_owned()
    } else {
        format!("`{}`", encode_native_path(&candidate))
    };

    if !candidate.is_absolute() {
        return Err(unusable(format!(
            "{named} is relative and the invocation directory `{}` is not \
             absolute, so it cannot be anchored",
            encode_native_path(&input.current_dir)
        )));
    }

    // The usability requirements themselves are shared with implicit
    // selection; only the phrasing is the variable's own, because a refused
    // user is reading about `DOTFILES_ROOT` rather than about a path Dodot
    // picked. The last case is defensive: a canonical path from the filesystem
    // is absolute and free of `..`, so it only fires for a probe that says
    // otherwise, and it stays an environment diagnostic so the message still
    // names the variable.
    let identity = canonical_root_identity(&candidate, probe).map_err(|failure| match failure {
        UnusableRoot::Missing => unusable(format!("{named} does not exist")),
        UnusableRoot::Unresolvable(err) => {
            unusable(format!("{named} could not be resolved: {err}"))
        }
        UnusableRoot::NotADirectory => unusable(format!("{named} is not a directory")),
        UnusableRoot::Unreadable => unusable(format!(
            "{named} is a directory whose contents cannot be read"
        )),
        UnusableRoot::NotAnIdentity(err) => {
            unusable(format!("{named} did not resolve to a usable root: {err}"))
        }
    })?;

    Ok(Some(ResolvedRoot::new(identity, RootSource::Environment)))
}

/// Expand a leading `~` against `home_dir`, byte-wise so a non-Unicode value
/// survives.
///
/// Only a bare `~` and a `~/`-prefixed path expand; `~user` is left alone,
/// matching the rest of Dodot's tilde handling (`crate::paths`).
fn expand_leading_tilde(raw_value: &OsStr, home_dir: &Path) -> PathBuf {
    let bytes = raw_value.as_bytes();

    if bytes == b"~" {
        return home_dir.to_path_buf();
    }

    let Some(rest) = bytes.strip_prefix(b"~/") else {
        return PathBuf::from(raw_value);
    };

    // `~//dots` must stay under home: joining a `/`-prefixed remainder would
    // discard the home directory entirely.
    let rest = match rest.iter().position(|byte| *byte != b'/') {
        Some(start) => &rest[start..],
        None => return home_dir.to_path_buf(),
    };

    home_dir.join(Path::new(OsStr::from_bytes(rest)))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::Mutex;

    use super::super::roots::RootIdentity;
    use super::super::test_probe::{Entry, FakeProbe};
    use super::*;

    const CWD: &str = "/home/alice/work";
    const HOME: &str = "/home/alice";

    fn resolve(raw_value: impl Into<OsString>, probe: &FakeProbe) -> Result<Option<ResolvedRoot>> {
        resolve_environment_root(&EnvironmentRootInput::set(CWD, HOME, raw_value), probe)
    }

    fn unusable_reason(error: SafetyLockError) -> String {
        match error {
            SafetyLockError::EnvironmentRootUnusable { reason, .. } => reason,
            other => panic!("expected an unusable-value error, got: {other}"),
        }
    }

    #[test]
    fn presence_is_independent_of_usability() {
        assert!(!EnvironmentRootInput::unset(CWD, HOME).is_set());
        assert!(EnvironmentRootInput::set(CWD, HOME, "").is_set());
        assert!(EnvironmentRootInput::set(CWD, HOME, "/srv/dots").is_set());
    }

    #[test]
    fn a_captured_value_keeps_its_native_bytes() {
        let raw = OsString::from_vec(b"/tmp/\x80dots".to_vec());
        let input = EnvironmentRootInput::set(CWD, HOME, raw.clone());

        assert_eq!(input.raw_value.as_deref(), Some(raw.as_os_str()));
    }

    #[test]
    fn an_unset_variable_leaves_selection_to_implicit_discovery() {
        let probe = FakeProbe::default();
        let resolved =
            resolve_environment_root(&EnvironmentRootInput::unset(CWD, HOME), &probe).unwrap();

        assert_eq!(resolved, None);
        assert!(
            probe.canonicalized().is_empty(),
            "an unset variable touched the filesystem"
        );
    }

    #[test]
    fn an_absolute_value_selects_that_root_with_environment_provenance() {
        let root = resolve("/srv/dots", &FakeProbe::dir("/srv/dots"))
            .unwrap()
            .expect("a valid value selects a root");

        assert_eq!(root.as_path(), Path::new("/srv/dots"));
        assert_eq!(root.source(), RootSource::Environment);
    }

    /// Selection canonicalizes once, so downstream trust lookup and mutation
    /// see the symlink's target rather than the spelling that reached them
    /// (ADR-0001).
    #[test]
    fn a_value_is_canonicalized_once_into_the_identity() {
        let probe = FakeProbe::default().link("/srv/link", "/srv/dots");
        let root = resolve("/srv/link", &probe).unwrap().unwrap();

        assert_eq!(root.identity(), &RootIdentity::new("/srv/dots").unwrap());
        assert_eq!(probe.canonicalized(), vec![PathBuf::from("/srv/link")]);
    }

    /// The value is anchored to the *injected* invocation directory: this
    /// module reads no process state, so the same input resolves the same way
    /// wherever the test binary happens to run.
    #[test]
    fn a_relative_value_resolves_against_the_injected_invocation_directory() {
        let probe = FakeProbe::dir("/home/alice/work/dots");
        let root = resolve("dots", &probe).unwrap().unwrap();

        assert_eq!(root.as_path(), Path::new("/home/alice/work/dots"));
        assert_eq!(
            probe.canonicalized(),
            vec![PathBuf::from("/home/alice/work/dots")],
            "the probe saw a path anchored somewhere other than the injected cwd"
        );
    }

    #[test]
    fn a_leading_tilde_expands_against_the_injected_home_directory() {
        for (value, expected) in [
            ("~", "/home/alice"),
            ("~/", "/home/alice"),
            ("~/dots", "/home/alice/dots"),
            // A doubled separator must not discard home.
            ("~//dots", "/home/alice/dots"),
        ] {
            let root = resolve(value, &FakeProbe::dir(expected)).unwrap().unwrap();
            assert_eq!(
                root.as_path(),
                Path::new(expected),
                "`{value}` expanded wrong"
            );
        }
    }

    /// `~user` is not tilde expansion Dodot performs, so it stays a literal
    /// relative path rather than becoming some other user's home.
    #[test]
    fn a_tilde_user_value_is_not_expanded() {
        let probe = FakeProbe::dir("/home/alice/work/~bob/dots");
        let root = resolve("~bob/dots", &probe).unwrap().unwrap();

        assert_eq!(root.as_path(), Path::new("/home/alice/work/~bob/dots"));
    }

    /// A non-Unicode value keeps its native bytes all the way into the
    /// identity, and is spelled the same reversible way trust state spells it
    /// (ADR-0001).
    #[test]
    fn a_non_unicode_value_keeps_its_native_form() {
        let raw = OsString::from_vec(b"/srv/\x80dots".to_vec());
        let probe = FakeProbe::default().add(PathBuf::from(raw.clone()), Entry::ReadableDir);

        let root = resolve(raw.clone(), &probe).unwrap().unwrap();

        assert_eq!(root.as_path(), Path::new(&raw));
        assert_eq!(root.identity().spelling(), "os-bytes:2f7372762f80646f7473");
        assert_eq!(root.source(), RootSource::Environment);
    }

    /// A non-Unicode value that fails validation is named in the same
    /// reversible spelling, so the diagnostic points at a path the user can
    /// act on (Spec, "Proposed Shape").
    #[test]
    fn a_non_unicode_failure_is_named_reversibly() {
        let raw = OsString::from_vec(b"/srv/\x80dots".to_vec());
        let error = resolve(raw, &FakeProbe::default()).unwrap_err();

        let SafetyLockError::EnvironmentRootUnusable { spelling, .. } = error else {
            panic!("expected an unusable-value error");
        };
        assert_eq!(spelling, "os-bytes:2f7372762f80646f7473");
    }

    #[test]
    fn an_empty_value_is_a_hard_error() {
        let error = resolve("", &FakeProbe::default()).unwrap_err();
        assert!(
            matches!(error, SafetyLockError::EnvironmentRootEmpty),
            "unexpected error: {error}"
        );
    }

    /// Each unusable class names what is actually wrong: the user's next
    /// action differs between a typo, a file, and a permission problem.
    #[test]
    fn each_unusable_class_produces_its_own_diagnostic() {
        let missing = unusable_reason(resolve("/srv/dots", &FakeProbe::default()).unwrap_err());
        let not_a_dir = unusable_reason(
            resolve(
                "/srv/dots",
                &FakeProbe::with("/srv/dots", Entry::NotADirectory),
            )
            .unwrap_err(),
        );
        let unreadable = unusable_reason(
            resolve(
                "/srv/dots",
                &FakeProbe::with("/srv/dots", Entry::UnreadableDir),
            )
            .unwrap_err(),
        );
        let uncanonicalizable = unusable_reason(
            resolve(
                "/srv/dots",
                &FakeProbe::with(
                    "/srv/dots",
                    Entry::Unresolvable(io::ErrorKind::PermissionDenied),
                ),
            )
            .unwrap_err(),
        );

        assert!(missing.contains("does not exist"), "{missing}");
        assert!(not_a_dir.contains("is not a directory"), "{not_a_dir}");
        assert!(unreadable.contains("cannot be read"), "{unreadable}");
        assert!(
            uncanonicalizable.contains("could not be resolved"),
            "{uncanonicalizable}"
        );

        let distinct: HashSet<String> = [missing, not_a_dir, unreadable, uncanonicalizable]
            .into_iter()
            .collect();
        assert_eq!(distinct.len(), 4, "two classes share one diagnostic");
    }

    /// A value that had to be expanded is reported with the path it expanded
    /// to: `~/dots` alone does not tell the reader where Dodot looked.
    #[test]
    fn a_failing_expanded_value_names_the_path_it_expanded_to() {
        let error = resolve("~/dots", &FakeProbe::default()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "DOTFILES_ROOT is set to `~/dots` but `/home/alice/dots` does not exist"
        );
    }

    /// Anchoring a relative value to a relative invocation directory would
    /// hand the probe a relative path, which the operating system resolves
    /// against the process working directory — the one piece of state this
    /// module refuses to depend on.
    #[test]
    fn a_relative_value_is_refused_when_the_invocation_directory_is_relative() {
        let probe = FakeProbe::dir("work/dots");
        let input = EnvironmentRootInput::set("work", HOME, "dots");

        let reason = unusable_reason(resolve_environment_root(&input, &probe).unwrap_err());

        assert!(reason.contains("cannot be anchored"), "{reason}");
        assert!(
            probe.canonicalized().is_empty(),
            "a relative path reached the filesystem probe"
        );
    }

    /// The Spec's central environment rule (story 9, ADR-0002): no present
    /// value, however broken, may reach Git or current-directory discovery.
    /// This is the composition a caller performs, with discovery instrumented.
    #[test]
    fn no_failing_value_ever_reaches_implicit_discovery() {
        struct Selection {
            implicit_calls: Mutex<usize>,
        }

        impl Selection {
            /// Environment first, implicit discovery only when the variable
            /// selected nothing — the shape every caller of
            /// [`resolve_environment_root`] has.
            fn select(&self, input: &EnvironmentRootInput, probe: &dyn PathProbe) -> Result<()> {
                if resolve_environment_root(input, probe)?.is_some() {
                    return Ok(());
                }
                *self.implicit_calls.lock().unwrap() += 1;
                Ok(())
            }
        }

        let failing: Vec<(&str, FakeProbe)> = vec![
            ("", FakeProbe::default()),
            ("/srv/missing", FakeProbe::default()),
            (
                "/srv/dots",
                FakeProbe::with("/srv/dots", Entry::NotADirectory),
            ),
            (
                "/srv/dots",
                FakeProbe::with("/srv/dots", Entry::UnreadableDir),
            ),
            (
                "/srv/dots",
                FakeProbe::with(
                    "/srv/dots",
                    Entry::Unresolvable(io::ErrorKind::PermissionDenied),
                ),
            ),
            ("~/dots", FakeProbe::default()),
            ("dots", FakeProbe::default()),
        ];

        let selection = Selection {
            implicit_calls: Mutex::new(0),
        };

        for (value, probe) in &failing {
            let input = EnvironmentRootInput::set(CWD, HOME, *value);
            assert!(
                selection.select(&input, probe).is_err(),
                "`{value}` did not fail"
            );
        }
        assert_eq!(
            *selection.implicit_calls.lock().unwrap(),
            0,
            "a failing DOTFILES_ROOT fell through to implicit discovery"
        );

        // The one input that *does* hand over: no variable at all.
        selection
            .select(
                &EnvironmentRootInput::unset(CWD, HOME),
                &FakeProbe::default(),
            )
            .unwrap();
        assert_eq!(*selection.implicit_calls.lock().unwrap(), 1);
    }

    /// An environment root is deliberate selection, so it needs no approval —
    /// and resolution produces nothing to persist, which is what keeps it
    /// invocation-local (ADR-0003).
    #[test]
    fn an_environment_root_is_invocation_local_and_needs_no_approval() {
        let root = resolve("/srv/dots", &FakeProbe::dir("/srv/dots"))
            .unwrap()
            .unwrap();

        assert!(!root.requires_approval());
        assert!(!root.source().is_implicit());
    }

    /// The whole path against a real filesystem, so the injected probe is not
    /// the only thing the behaviour has ever been proven against.
    #[test]
    fn a_real_directory_resolves_through_the_os_probe() {
        use super::super::util::OsPathProbe;

        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("dotfiles");
        std::fs::create_dir(&root).unwrap();
        let canonical_root = std::fs::canonicalize(&root).unwrap();

        let input = EnvironmentRootInput::set(home.path(), home.path(), "~/dotfiles");
        let resolved = resolve_environment_root(&input, &OsPathProbe)
            .unwrap()
            .unwrap();

        assert_eq!(resolved.as_path(), canonical_root);
        assert_eq!(resolved.source(), RootSource::Environment);

        // A file at the same coordinate is refused rather than selected.
        let file = home.path().join("not-a-root");
        std::fs::write(&file, b"").unwrap();
        let input = EnvironmentRootInput::set(home.path(), home.path(), file);
        let reason = unusable_reason(resolve_environment_root(&input, &OsPathProbe).unwrap_err());
        assert!(reason.contains("is not a directory"), "{reason}");
    }
}
