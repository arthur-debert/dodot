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
use std::path::{Path, PathBuf};

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
/// is held privately because every construction route validates it: an
/// identity that is relative — and therefore cannot be approved or revoked
/// unambiguously — is not representable.
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
    /// Build an identity from an already-canonicalized absolute path.
    ///
    /// Canonicalization itself belongs to the selection modules — this
    /// constructor only enforces the invariant every consumer relies on
    /// (absolute path, native bytes preserved).
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(SafetyLockError::RelativeRootIdentity {
                spelling: encode_native_path(&path),
            });
        }
        Ok(Self { path })
    }

    /// Read an identity back from a stored or user-supplied spelling.
    ///
    /// Accepts both forms [`spelling`](Self::spelling) produces, so a line
    /// copied out of `roots list` can be passed straight back to
    /// `roots forget`.
    pub fn parse(spelling: &str) -> Result<Self> {
        Self::new(decode_native_path(spelling)?)
    }

    /// The canonical native path.
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
    /// root the same one (ADR-0002). Both sides must already be canonical;
    /// this is a path comparison, not a filesystem check.
    pub fn contains(&self, candidate: &Path) -> bool {
        candidate.starts_with(&self.path)
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
