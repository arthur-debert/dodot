//! The persisted Safety Lock trust schema.
//!
//! Approval state is **one** clapfig-managed file, `safety-lock.toml`, living
//! under Dodot's data directory — never inside the dotfiles root, which holds
//! user-authored source only (Spec, "State ownership and migration"). Its
//! whole content is the collection of implicit roots the user approved:
//!
//! ```toml
//! [roots]
//! approved = [
//!   "/home/alice/dotfiles",
//!   "/home/alice/work-dotfiles",
//!   "os-bytes:2f746d702f80646f7473",  # a non-Unicode root
//! ]
//! ```
//!
//! Three properties this shape is chosen for:
//!
//! - **Multiple roots, no preferred one.** `approved` is a flat collection;
//!   nothing marks a default (Spec, story 5).
//! - **Absent file means no approvals, never an error.** The compiled default
//!   is an empty collection, so a first-ever run just finds nothing approved.
//!   That is the *only* way an empty registry may arise: an unreadable or
//!   malformed file fails closed (ADR-0001) rather than reading as empty.
//! - **Lossless identity.** Entries are [`RootIdentity`] spellings, so a
//!   non-Unicode root survives a write/read cycle byte for byte.
//!
//! Environment-selected roots never appear here: `DOTFILES_ROOT` *is* the
//! deliberate selection, so it is authorized without a record (ADR-0003).
//!
//! This module owns the schema, the store's coordinates, and the two ways to
//! load it: [`SafetyLockConfig::load_from`], which every consumer uses and
//! which fails closed, and
//! [`SafetyLockConfig::load_for_revocation`], which drops the duplicate check
//! alone so `roots forget` can repair the one violation a readable file can
//! carry. *Writing* belongs to the caller: the checking, listing, and
//! forgetting APIs take an already-loaded [`SafetyLockConfig`] and return
//! typed state changes.

use std::path::{Path, PathBuf};

use clapfig::{Clapfig, ClapfigBuilder, SearchPath};
use confique::Config;
use serde::{Deserialize, Serialize};

use super::error::{Result, SafetyLockError};
use super::roots::RootIdentity;

/// File name of the trust file inside Dodot's data directory.
pub const SAFETY_LOCK_FILE_NAME: &str = "safety-lock.toml";

/// Name of the clapfig persist scope that writes [`SAFETY_LOCK_FILE_NAME`].
pub const SAFETY_LOCK_PERSIST_SCOPE: &str = "data";

/// The complete persisted Safety Lock state.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SafetyLockConfig {
    #[config(nested)]
    pub roots: TrustedRootsSection,
}

/// The approved-roots collection.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct TrustedRootsSection {
    /// Dotfiles roots this user has approved for root-sensitive mutation,
    /// each as the canonical absolute path it was approved at.
    ///
    /// A path that is valid UTF-8 is written plainly; anything else is
    /// written as `os-bytes:` followed by the hex of its native bytes, which
    /// is exactly the spelling `dodot roots list` prints and `dodot roots
    /// forget` accepts back.
    ///
    /// Approval does not expire. Entries are removed by `dodot roots forget
    /// <path>` or by `dodot reset` clearing Dodot-owned state.
    #[config(default = [])]
    pub approved: Vec<RootIdentity>,
}

impl SafetyLockConfig {
    /// The trust file's path inside `data_dir`.
    pub fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(SAFETY_LOCK_FILE_NAME)
    }

    /// The clapfig builder for the trust file in `data_dir`.
    ///
    /// One search path and one persist scope: the trust file is a single
    /// document at a known location, not a merged hierarchy. The environment
    /// layer is off deliberately — an env var that could inject an approved
    /// root would be a bypass around the confirmation gate.
    ///
    /// [`validate`](Self::validate) is attached as clapfig's post-validation
    /// hook, so **every** route through this builder — `load`, and the config
    /// actions — either yields a config satisfying the invariants or fails.
    /// Validation is not a step a caller can forget: an invariant checked
    /// only by convention is one [`is_approved`](Self::is_approved) would
    /// eventually answer from unvalidated state.
    ///
    /// The one route that deliberately does not come through here is
    /// [`load_for_revocation`](Self::load_for_revocation), which has to read
    /// the state this hook refuses in order to repair it.
    pub fn store_in(data_dir: &Path) -> ClapfigBuilder<Self> {
        Self::coordinates(data_dir)
            .post_validate(|config: &Self| config.validate().map_err(|err| err.to_string()))
    }

    /// Where the trust file is and how it is layered — the store without the
    /// invariant hook.
    ///
    /// Private, and the only statement of the file's coordinates:
    /// [`store_in`](Self::store_in) is this plus post-validation, and
    /// [`load_for_revocation`](Self::load_for_revocation) is this alone.
    /// Saying it once is what keeps the validated and recovery routes reading
    /// the same file.
    fn coordinates(data_dir: &Path) -> ClapfigBuilder<Self> {
        Clapfig::builder::<Self>()
            .app_name("dodot")
            .file_name(SAFETY_LOCK_FILE_NAME)
            .search_paths(vec![SearchPath::Path(data_dir.to_path_buf())])
            .persist_scope(
                SAFETY_LOCK_PERSIST_SCOPE,
                SearchPath::Path(data_dir.to_path_buf()),
            )
            .no_env()
    }

    /// Load the trust file from `data_dir`, or the empty default when it is
    /// absent.
    ///
    /// The loading API Safety Lock consumers use. Fails closed on every other
    /// outcome — unreadable file, malformed entry, violated invariant — so a
    /// trust file Dodot cannot fully read never resolves to a smaller, or
    /// empty, set of approvals (ADR-0001).
    pub fn load_from(data_dir: &Path) -> Result<Self> {
        Self::load_through(Self::store_in(data_dir), data_dir)
    }

    /// Load the trust file for **revocation only**, without the duplicate
    /// check.
    ///
    /// `roots forget` is the Spec's narrow recovery from a trust file Dodot
    /// refuses: "`roots list` surfaces the problem, `roots forget` removes an
    /// identifiable affected record, and factory `reset` remains the recovery
    /// route when narrower revocation is not possible." A duplicated approval
    /// is the only invariant a syntactically valid file can violate, so
    /// loading through [`load_from`](Self::load_from) would refuse the very
    /// state revocation exists to repair, and the user's only exit from one
    /// bad record would be the factory reset that drops every *other* approval
    /// with it.
    ///
    /// Relaxed by that one invariant and nothing else. An unreadable file, a
    /// malformed document, and an entry that is not a representable
    /// [`RootIdentity`] all still fail, because those are refused while
    /// deserializing rather than by the hook this route drops.
    ///
    /// What comes back belongs to
    /// [`forget_root`](super::forget::forget_root) and to nothing else — it
    /// validates the collection it *leaves*, so the repair is checked before
    /// the caller writes it. Routing [`decide`](super::check::decide),
    /// [`approve`](super::check::approve), or
    /// [`list_roots`](super::list::list_roots) through here would change
    /// nothing but which call reports the problem: each validates what it is
    /// given.
    pub fn load_for_revocation(data_dir: &Path) -> Result<Self> {
        Self::load_through(Self::coordinates(data_dir), data_dir)
    }

    /// Run a load and name the trust file in whatever goes wrong, so both
    /// routes report a failure the same way.
    fn load_through(store: ClapfigBuilder<Self>, data_dir: &Path) -> Result<Self> {
        store
            .load()
            .map_err(|err| SafetyLockError::TrustStateUnusable {
                path: Self::path_in(data_dir),
                reason: err.to_string(),
            })
    }

    /// Whether `identity` is approved.
    ///
    /// Matching is exact on the canonical native path, so two roots whose
    /// lossy renderings collide never satisfy each other's lookup.
    pub fn is_approved(&self, identity: &RootIdentity) -> bool {
        self.roots.approved.contains(identity)
    }

    /// Check the invariants a loaded trust file must satisfy.
    ///
    /// Runs automatically on every load — [`store_in`](Self::store_in)
    /// attaches it as clapfig's post-validation hook — and is public for the
    /// other direction: checking a collection Dodot has just changed, before
    /// writing it back.
    ///
    /// Only one invariant can be violated by a syntactically valid file: the
    /// same canonical root listed twice. (Relative, aliased, or malformed
    /// entries cannot be represented — [`RootIdentity`] rejects them while
    /// deserializing.) A duplicate is a hard error rather than a silent
    /// de-duplication because trust state Dodot does not fully understand
    /// must never be treated as approval.
    pub fn validate(&self) -> Result<()> {
        let approved = &self.roots.approved;
        for (index, identity) in approved.iter().enumerate() {
            if approved[..index].contains(identity) {
                return Err(SafetyLockError::DuplicateApprovedRoot {
                    spelling: identity.spelling(),
                });
            }
        }
        Ok(())
    }
}

impl Default for SafetyLockConfig {
    /// No approved roots — the state of a user who has never confirmed an
    /// implicit root, and what an absent trust file resolves to.
    fn default() -> Self {
        Self {
            roots: TrustedRootsSection {
                approved: Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    use clapfig::types::ConfigAction;
    use clapfig::ConfigResult;

    use super::*;

    fn identity(path: &str) -> RootIdentity {
        RootIdentity::new(path).unwrap()
    }

    fn non_unicode_identity(suffix: &[u8]) -> RootIdentity {
        let mut bytes = b"/tmp/".to_vec();
        bytes.extend_from_slice(suffix);
        RootIdentity::new(PathBuf::from(OsString::from_vec(bytes))).unwrap()
    }

    fn load(data_dir: &Path) -> SafetyLockConfig {
        SafetyLockConfig::load_from(data_dir).unwrap()
    }

    fn write_trust_file(data_dir: &Path, content: &str) {
        std::fs::write(SafetyLockConfig::path_in(data_dir), content).unwrap();
    }

    #[test]
    fn the_trust_file_is_one_document_under_the_data_dir() {
        assert_eq!(
            SafetyLockConfig::path_in(Path::new("/u/.local/share/dodot")),
            PathBuf::from("/u/.local/share/dodot/safety-lock.toml")
        );
    }

    #[test]
    fn absent_file_loads_an_empty_collection() {
        let data_dir = tempfile::tempdir().unwrap();
        let config = load(data_dir.path());

        assert!(config.roots.approved.is_empty());
        assert!(!SafetyLockConfig::path_in(data_dir.path()).exists());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn the_default_matches_what_an_absent_file_loads() {
        let data_dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load(data_dir.path()).roots.approved,
            SafetyLockConfig::default().roots.approved
        );
    }

    #[test]
    fn multiple_roots_round_trip_through_the_file() {
        let data_dir = tempfile::tempdir().unwrap();
        let approved = vec![
            identity("/home/alice/dotfiles"),
            identity("/home/alice/work-dotfiles"),
            non_unicode_identity(b"\x80dots"),
        ];

        let written = toml::to_string(&SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: approved.clone(),
            },
        })
        .unwrap();
        write_trust_file(data_dir.path(), &written);

        // Valid UTF-8 roots are stored plainly; only the non-Unicode one is
        // tagged. The file stays a document a user can read and edit.
        assert!(written.contains("\"/home/alice/dotfiles\""));
        assert!(written.contains("\"os-bytes:2f746d702f80646f7473\""));

        let loaded = load(data_dir.path());
        assert_eq!(loaded.roots.approved, approved);
        assert!(loaded.validate().is_ok());
    }

    /// The reason the byte-exact identity matters at the storage layer: a
    /// deleted non-Unicode root must remain revocable by passing back what
    /// `roots list` printed, which requires the stored spelling to survive
    /// the write/read cycle unchanged.
    #[test]
    fn lossy_colliding_roots_persist_as_two_records() {
        let data_dir = tempfile::tempdir().unwrap();
        let one = non_unicode_identity(b"\x80");
        let other = non_unicode_identity(b"\x81");

        let config = SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: vec![one.clone(), other.clone()],
            },
        };
        write_trust_file(data_dir.path(), &toml::to_string(&config).unwrap());

        let loaded = load(data_dir.path());
        assert_eq!(loaded.roots.approved, vec![one.clone(), other.clone()]);
        assert!(loaded.is_approved(&one));
        assert!(loaded.is_approved(&other));
        assert!(loaded.validate().is_ok());
    }

    #[test]
    fn approval_lookup_is_exact() {
        let config = SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: vec![identity("/home/alice/dotfiles")],
            },
        };

        assert!(config.is_approved(&identity("/home/alice/dotfiles")));
        assert!(!config.is_approved(&identity("/home/alice/dotfiles/vim")));
        assert!(!config.is_approved(&identity("/home/alice/other")));
        assert!(!SafetyLockConfig::default().is_approved(&identity("/home/alice/dotfiles")));
    }

    #[test]
    fn a_root_listed_twice_is_rejected() {
        let config = SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: vec![
                    identity("/home/alice/dotfiles"),
                    identity("/home/alice/dotfiles"),
                ],
            },
        };

        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, SafetyLockError::DuplicateApprovedRoot { ref spelling } if spelling == "/home/alice/dotfiles"),
            "unexpected error: {err}"
        );
    }

    /// An entry that is not a usable identity fails the load rather than
    /// being skipped: a trust file Dodot cannot fully read never resolves to
    /// a smaller — or empty — set of approvals.
    #[test]
    fn a_malformed_entry_fails_the_load_instead_of_reading_as_empty() {
        for entry in ["relative/dotfiles", "/home/alice/dotfiles/../other"] {
            let data_dir = tempfile::tempdir().unwrap();
            write_trust_file(
                data_dir.path(),
                &format!("[roots]\napproved = [\"{entry}\"]\n"),
            );

            assert!(
                SafetyLockConfig::load_from(data_dir.path()).is_err(),
                "`{entry}` loaded as a usable approval"
            );
        }
    }

    /// The invariant has to hold at the *loading* seam, not only for a value
    /// a caller remembered to check: `is_approved` answers authorization
    /// straight off a loaded config, so an invalid file must never become
    /// one.
    #[test]
    fn duplicate_roots_in_the_file_fail_the_production_load() {
        let data_dir = tempfile::tempdir().unwrap();
        write_trust_file(
            data_dir.path(),
            "[roots]\napproved = [\"/home/alice/dotfiles\", \"/home/alice/dotfiles\"]\n",
        );

        let err = SafetyLockConfig::load_from(data_dir.path()).unwrap_err();
        assert!(
            matches!(err, SafetyLockError::TrustStateUnusable { .. }),
            "unexpected error: {err}"
        );
        assert!(
            err.to_string().contains("/home/alice/dotfiles"),
            "the diagnostic must name the offending root: {err}"
        );

        // Not merely the wrapper's doing: the builder itself refuses.
        assert!(SafetyLockConfig::store_in(data_dir.path()).load().is_err());
    }

    /// The revocation route relaxes the duplicate invariant and *only* that
    /// one, so `roots forget` can reach the state it exists to repair without
    /// becoming a way to read any other broken file as approval.
    #[test]
    fn the_revocation_route_relaxes_the_duplicate_invariant_and_nothing_else() {
        let data_dir = tempfile::tempdir().unwrap();
        write_trust_file(
            data_dir.path(),
            "[roots]\napproved = [\"/home/alice/dotfiles\", \"/home/alice//dotfiles/\"]\n",
        );

        // The duplicate — including one reached through a spelling variant —
        // loads, byte-exact and un-de-duplicated, so revocation can see which
        // record to remove.
        let recoverable = SafetyLockConfig::load_for_revocation(data_dir.path()).unwrap();
        assert_eq!(
            recoverable.roots.approved,
            vec![
                identity("/home/alice/dotfiles"),
                identity("/home/alice/dotfiles")
            ]
        );
        assert!(
            recoverable.validate().is_err(),
            "the route must hand back the unusable state, not quietly repair it"
        );

        // Everything else still fails: a malformed document, and an entry
        // that is not a representable identity.
        for content in [
            "[roots]\napproved = [\"/home/alice/dotfiles\"",
            "[roots]\napproved = [\"relative/dotfiles\"]\n",
            "[roots]\napproved = [\"/home/alice/dotfiles/../other\"]\n",
            "[roots]\napproved = \"not-a-list\"\n",
        ] {
            let data_dir = tempfile::tempdir().unwrap();
            write_trust_file(data_dir.path(), content);

            assert!(
                SafetyLockConfig::load_for_revocation(data_dir.path()).is_err(),
                "the revocation route accepted `{content}`"
            );
        }

        // And an absent file is still the empty default, not an error.
        let empty = tempfile::tempdir().unwrap();
        assert!(SafetyLockConfig::load_for_revocation(empty.path())
            .unwrap()
            .roots
            .approved
            .is_empty());
    }

    /// Two spellings of one root are a duplicate too — identity
    /// normalization is what makes the check see through the alias.
    #[test]
    fn spelling_variants_of_one_root_are_caught_as_duplicates() {
        let data_dir = tempfile::tempdir().unwrap();
        write_trust_file(
            data_dir.path(),
            "[roots]\napproved = [\"/home/alice/dotfiles\", \"/home/alice//dotfiles/\"]\n",
        );

        assert!(SafetyLockConfig::load_from(data_dir.path()).is_err());
    }

    /// The file is self-documenting: clapfig generates its template from the
    /// schema's doc comments, so the one documented file stays in step with
    /// the struct.
    #[test]
    fn the_generated_template_documents_the_approved_collection() {
        let data_dir = tempfile::tempdir().unwrap();
        let ConfigResult::Template(template) = SafetyLockConfig::store_in(data_dir.path())
            .handle(&ConfigAction::Gen { output: None })
            .unwrap()
        else {
            panic!("expected a generated template");
        };

        assert!(template.contains("[roots]"), "template:\n{template}");
        assert!(template.contains("approved"), "template:\n{template}");
        assert!(
            template.contains("dodot roots forget"),
            "the doc comment explaining how approvals are removed did not \
             reach the template:\n{template}"
        );
    }
}
