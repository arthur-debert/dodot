//! The root vocabulary every Safety Lock consumer shares.
//!
//! Both selection paths — `DOTFILES_ROOT` and implicit Git/cwd discovery —
//! produce one [`ResolvedRoot`]: the same canonical [`RootIdentity`] plus the
//! [`RootSource`] that selected it. Provenance changes *authorization policy*
//! (an environment root is deliberate; an implicit root needs approval); it
//! never changes how the path is represented, compared, or displayed. That is
//! why no source-specific struct exists past this module: checking, listing,
//! inventory, and mutation scoping all take a `ResolvedRoot` or a
//! `RootIdentity`.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::error::{Result, SafetyLockError};
use super::util::{decode_native_path, encode_native_path};

/// How the dotfiles root for this invocation was selected.
///
/// The two implicit variants are kept apart rather than folded into one
/// "implicit" case because the confirmation prompt has to tell the user
/// *which* mechanism picked the path — the Git top-level is frequently not the
/// directory shown in their shell prompt (Spec, story 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RootSource {
    /// A valid `DOTFILES_ROOT`. Deliberate selection: never requires
    /// approval, and never enters the approved-roots collection.
    Environment,
    /// The Git top-level enclosing the current directory.
    Git,
    /// The current directory, when no Git top-level applied.
    CurrentDirectory,
}

impl RootSource {
    /// Whether this source is implicit discovery, i.e. filesystem shape
    /// rather than an explicit act of selection.
    ///
    /// This is the single predicate the safety gate branches on; the
    /// per-mechanism distinction below it is presentational.
    pub fn is_implicit(self) -> bool {
        matches!(self, RootSource::Git | RootSource::CurrentDirectory)
    }

    /// Stable, user-facing name of the selecting mechanism.
    pub fn label(self) -> &'static str {
        match self {
            RootSource::Environment => "DOTFILES_ROOT",
            RootSource::Git => "git top-level",
            RootSource::CurrentDirectory => "current directory",
        }
    }
}

impl fmt::Display for RootSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A dotfiles root's identity: its canonical absolute path in the operating
/// system's native form.
///
/// Equality, ordering, and hashing compare the native path exactly, so two
/// roots that merely *render* alike stay two identities (ADR-0001). The path
/// is held privately because every construction route enforces the identity
/// invariant rather than trusting the caller for it: the stored path is
/// always absolute, always free of `..`, and always spelled with single
/// separators and no trailing one. A relative path — which cannot be approved
/// or revoked unambiguously — and a `..` alias — which would be a *second*
/// identity for a root already approved — are both unrepresentable.
///
/// What the type cannot promise is symlink resolution: that needs the
/// filesystem. Selection canonicalizes once per invocation through
/// [`PathProbe`](super::util::PathProbe) and builds the identity from that
/// result (ADR-0001); the invariant here is what holds for *every* identity,
/// including one read back from trust state long after the path it names
/// stopped existing.
///
/// Serialization uses the reversible spelling of
/// [`encode_native_path`](super::util::encode_native_path), which is what
/// makes the trust file a valid TOML/JSON/YAML document without discarding
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RootIdentity {
    path: PathBuf,
}

impl RootIdentity {
    /// Build an identity from an absolute path, enforcing everything
    /// canonicality means that can be decided without touching the
    /// filesystem.
    ///
    /// Refused: a relative path, and any path carrying a `..` component.
    /// Accepted and made canonical in place: purely syntactic spelling
    /// differences that name the same path unconditionally — repeated
    /// separators, a trailing separator, an interior `.`. `/srv//dots/`,
    /// `/srv/./dots`, and `/srv/dots` are therefore one identity, not three,
    /// so no alias can slip past duplicate detection or approval lookup.
    ///
    /// `..` is refused rather than folded away because folding it is only
    /// correct when no component is a symlink, which this constructor cannot
    /// know. Symlink resolution belongs to selection, which canonicalizes
    /// through [`PathProbe`](super::util::PathProbe) and passes the result
    /// here.
    ///
    /// Native bytes are preserved: normalization rewrites separators only,
    /// never a component.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(SafetyLockError::RelativeRootIdentity {
                spelling: encode_native_path(&path),
            });
        }

        let mut canonical = PathBuf::new();
        for component in path.components() {
            if component == Component::ParentDir {
                return Err(SafetyLockError::NonCanonicalRootIdentity {
                    spelling: encode_native_path(&path),
                });
            }
            canonical.push(component.as_os_str());
        }

        Ok(Self { path: canonical })
    }

    /// Read an identity back from a stored or user-supplied spelling.
    ///
    /// Accepts both forms [`spelling`](Self::spelling) produces, so a line
    /// copied out of `roots list` can be passed straight back to
    /// `roots forget`, and applies the same invariant as
    /// [`new`](Self::new) — a stored entry that is relative or aliased fails
    /// the read instead of becoming a second identity.
    pub fn parse(spelling: &str) -> Result<Self> {
        Self::new(decode_native_path(spelling)?)
    }

    /// The canonical native path: absolute, free of `..`, singly separated.
    ///
    /// Canonical up to symlinks — see [`RootIdentity`] for what the type does
    /// and does not decide without the filesystem.
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    /// The reversible spelling: plain text for a UTF-8 path, tagged hex
    /// otherwise. This is what trust state stores and what `roots list`
    /// prints.
    pub fn spelling(&self) -> String {
        encode_native_path(&self.path)
    }

    /// Whether `candidate` is this root or lies inside it.
    ///
    /// Used by mutation scoping to keep an authorized root and the mutated
    /// root the same one (ADR-0002). This is a path comparison, not a
    /// filesystem check: the caller canonicalizes `candidate` first, and a
    /// candidate that is not canonical is answered `false` rather than
    /// trusted. `/srv/dots/../../etc/passwd` starts with `/srv/dots`
    /// component-wise, so a plain prefix test would authorize a write clean
    /// outside the root; here it lands in the out-of-root report instead,
    /// which is the direction that fails closed.
    pub fn contains(&self, candidate: &Path) -> bool {
        candidate.is_absolute()
            && !candidate
                .components()
                .any(|component| component == Component::ParentDir)
            && candidate.starts_with(&self.path)
    }
}

impl fmt::Display for RootIdentity {
    /// Displays the reversible spelling — never a lossy rendering, so a
    /// diagnostic names the root the user can act on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.spelling())
    }
}

impl Serialize for RootIdentity {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.spelling())
    }
}

impl<'de> Deserialize<'de> for RootIdentity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct SpellingVisitor;

        impl Visitor<'_> for SpellingVisitor {
            type Value = RootIdentity;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an absolute dotfiles-root path, or its `os-bytes:` spelling")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
                RootIdentity::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(SpellingVisitor)
    }
}

/// One invocation's dotfiles root: the canonical identity plus how it was
/// selected.
///
/// Resolution happens once per invocation and this value is then carried
/// through trust lookup, confirmation, and execution — nothing downstream
/// consults the environment, Git, or the current directory again (ADR-0001).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedRoot {
    identity: RootIdentity,
    source: RootSource,
}

impl ResolvedRoot {
    /// Pair a canonical identity with the source that selected it.
    pub fn new(identity: RootIdentity, source: RootSource) -> Self {
        Self { identity, source }
    }

    /// The canonical identity.
    pub fn identity(&self) -> &RootIdentity {
        &self.identity
    }

    /// The canonical native path — shorthand for `identity().as_path()`.
    pub fn as_path(&self) -> &Path {
        self.identity.as_path()
    }

    /// How this root was selected.
    pub fn source(&self) -> RootSource {
        self.source
    }

    /// Whether reaching a root-sensitive mutation through this root requires
    /// a recorded approval.
    ///
    /// True exactly for implicitly discovered roots: a valid `DOTFILES_ROOT`
    /// already expresses deliberate selection (Spec, story 8).
    pub fn requires_approval(&self) -> bool {
        self.source.is_implicit()
    }

    /// Consume the resolved root, keeping only its identity.
    pub fn into_identity(self) -> RootIdentity {
        self.identity
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    fn identity(path: &str) -> RootIdentity {
        RootIdentity::new(path).unwrap()
    }

    fn non_unicode_identity(suffix: &[u8]) -> RootIdentity {
        let mut bytes = b"/tmp/".to_vec();
        bytes.extend_from_slice(suffix);
        RootIdentity::new(PathBuf::from(OsString::from_vec(bytes))).unwrap()
    }

    #[test]
    fn relative_paths_are_not_identities() {
        let err = RootIdentity::new("dotfiles").unwrap_err();
        assert!(
            matches!(err, SafetyLockError::RelativeRootIdentity { .. }),
            "unexpected error: {err}"
        );
    }

    /// The alias case the authorization boundary turns on: `..` cannot be
    /// resolved without the filesystem, so an identity carrying one is
    /// refused instead of becoming a second record for an already-approved
    /// root.
    #[test]
    fn parent_dir_aliases_are_not_identities() {
        for alias in ["/srv/dots/../other", "/srv/dots/..", "/../srv/dots"] {
            let err = RootIdentity::new(alias).unwrap_err();
            assert!(
                matches!(err, SafetyLockError::NonCanonicalRootIdentity { .. }),
                "`{alias}` was accepted as an identity: {err:?}"
            );
        }

        // Every construction route, not just the direct one.
        assert!(RootIdentity::parse("/srv/dots/../other").is_err());
        assert!(serde_json::from_str::<RootIdentity>("\"/srv/dots/../other\"").is_err());
    }

    /// A `..` alias must not be able to reach a distinct identity by any
    /// route — a lexically resolved spelling and the real path are one
    /// record, and duplicate detection sees them as one.
    #[test]
    fn aliases_cannot_produce_a_second_identity_for_one_root() {
        let root = identity("/srv/dots/other");

        for alias in ["/srv//dots/other", "/srv/dots/other/", "/srv/./dots/other"] {
            let from_alias = RootIdentity::new(alias).unwrap();
            assert_eq!(from_alias, root, "`{alias}` became a second identity");
            assert_eq!(from_alias.spelling(), root.spelling());
        }

        let set: HashSet<RootIdentity> =
            ["/srv//dots/other", "/srv/dots/other/", "/srv/dots/other"]
                .into_iter()
                .map(identity)
                .collect();
        assert_eq!(
            set.len(),
            1,
            "spelling variants de-duplicated into one root"
        );
    }

    /// Normalization rewrites separators only: a non-Unicode component keeps
    /// its bytes, so the identity still round-trips through trust state.
    #[test]
    fn normalization_preserves_native_component_bytes() {
        let original = non_unicode_identity(b"\x80dots");
        let with_trailing_separator = {
            let mut bytes = b"/tmp/".to_vec();
            bytes.extend_from_slice(b"\x80dots/");
            RootIdentity::new(PathBuf::from(OsString::from_vec(bytes))).unwrap()
        };

        assert_eq!(with_trailing_separator, original);
        assert_eq!(original.spelling(), "os-bytes:2f746d702f80646f7473");
    }

    #[test]
    fn identity_displays_its_reversible_spelling() {
        assert_eq!(
            identity("/home/alice/dotfiles").to_string(),
            "/home/alice/dotfiles"
        );
        assert!(non_unicode_identity(b"\x80")
            .to_string()
            .starts_with("os-bytes:"));
    }

    #[test]
    fn identity_parses_back_from_either_spelling() {
        for original in [
            identity("/home/alice/dotfiles"),
            non_unicode_identity(b"\x80dots"),
        ] {
            let parsed = RootIdentity::parse(&original.spelling()).unwrap();
            assert_eq!(parsed, original);
        }
    }

    /// Identity is the native path, not a rendering of it: roots that render
    /// identically must not compare, hash, or de-duplicate as one.
    #[test]
    fn identities_never_collapse_on_a_lossy_rendering() {
        let one = non_unicode_identity(b"\x80");
        let other = non_unicode_identity(b"\x81");

        assert_eq!(
            one.as_path().to_string_lossy(),
            other.as_path().to_string_lossy(),
            "test premise: these two roots render identically when lossy"
        );
        assert_ne!(one, other);

        let set: HashSet<RootIdentity> = [one, other].into_iter().collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn identity_containment_covers_the_root_itself_and_its_children() {
        let root = identity("/home/alice/dotfiles");

        assert!(root.contains(Path::new("/home/alice/dotfiles")));
        assert!(root.contains(Path::new("/home/alice/dotfiles/vim/vimrc")));
        assert!(!root.contains(Path::new("/home/alice/other/vimrc")));
        // Prefix-of-a-component, not a real descendant.
        assert!(!root.contains(Path::new("/home/alice/dotfiles-backup/vimrc")));
    }

    /// Containment is the mutation-scoping gate, so a candidate that walks
    /// back out of the root must not be reported as inside it — a plain
    /// component-prefix test says it is.
    #[test]
    fn containment_refuses_candidates_that_escape_through_parent_dirs() {
        let root = identity("/home/alice/dotfiles");

        assert!(
            Path::new("/home/alice/dotfiles/../../../etc/passwd").starts_with(root.as_path()),
            "test premise: a bare prefix test accepts this escape"
        );
        assert!(!root.contains(Path::new("/home/alice/dotfiles/../../../etc/passwd")));
        assert!(!root.contains(Path::new("/home/alice/dotfiles/vim/../vimrc")));
        assert!(!root.contains(Path::new("dotfiles/vim/vimrc")));
    }

    #[test]
    fn only_implicit_sources_require_approval() {
        let id = identity("/home/alice/dotfiles");

        assert!(!ResolvedRoot::new(id.clone(), RootSource::Environment).requires_approval());
        assert!(ResolvedRoot::new(id.clone(), RootSource::Git).requires_approval());
        assert!(ResolvedRoot::new(id, RootSource::CurrentDirectory).requires_approval());
    }

    #[test]
    fn source_labels_name_the_selecting_mechanism() {
        assert_eq!(RootSource::Environment.to_string(), "DOTFILES_ROOT");
        assert_eq!(RootSource::Git.to_string(), "git top-level");
        assert_eq!(
            RootSource::CurrentDirectory.to_string(),
            "current directory"
        );
    }

    /// Both selection paths hand downstream code the same type, so a
    /// consumer cannot accidentally branch on a source-specific struct.
    #[test]
    fn both_selection_paths_produce_one_resolved_root_type() {
        let from_env = ResolvedRoot::new(identity("/srv/dots"), RootSource::Environment);
        let from_git = ResolvedRoot::new(identity("/srv/dots"), RootSource::Git);

        assert_eq!(from_env.identity(), from_git.identity());
        assert_eq!(from_env.as_path(), Path::new("/srv/dots"));
        assert_ne!(from_env, from_git);
        assert_eq!(from_git.into_identity(), identity("/srv/dots"));
    }

    #[test]
    fn identity_serializes_as_its_spelling() {
        let plain = serde_json::to_string(&identity("/home/alice/dotfiles")).unwrap();
        assert_eq!(plain, "\"/home/alice/dotfiles\"");

        let tagged = serde_json::to_string(&non_unicode_identity(b"\x80")).unwrap();
        assert_eq!(tagged, "\"os-bytes:2f746d702f80\"");
    }

    #[test]
    fn identity_deserialization_rejects_relative_and_malformed_spellings() {
        assert!(serde_json::from_str::<RootIdentity>("\"dotfiles\"").is_err());
        assert!(serde_json::from_str::<RootIdentity>("\"os-bytes:2f7\"").is_err());
        assert!(serde_json::from_str::<RootIdentity>("42").is_err());
    }
}
