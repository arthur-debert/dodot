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
//! This module owns the schema, the store's coordinates, and the ways to
//! reach the document at them: [`SafetyLockConfig::load_from`], which every
//! read-only consumer uses and which fails closed;
//! [`SafetyLockConfig::load_for_revocation`], which drops the duplicate check
//! alone so `roots forget` can repair the one violation a readable file can
//! carry; and [`TrustFileTransaction`], the only route to the write.
//!
//! Deciding *whether* to write stays outside: the checking, listing, and
//! forgetting APIs take an already-loaded [`SafetyLockConfig`] and hand back a
//! typed state change rather than committing it, so the CLI can record an
//! approval before starting the mutation it authorizes. What the transaction
//! adds is *when* that state is read: a writer re-loads under the
//! transaction's interprocess lock and applies its semantic change to the
//! state actually on disk, so two concurrent writers compose instead of the
//! later one silently persisting the earlier one away (Spec,
//! "Testing / Verification": concurrent attempts).

use std::path::{Path, PathBuf};

use clapfig::{Clapfig, ClapfigBuilder, SearchPath};
use confique::Config;
use serde::{Deserialize, Serialize};

use super::error::{Result, SafetyLockError};
use super::roots::RootIdentity;

/// File name of the trust file inside Dodot's data directory.
pub const SAFETY_LOCK_FILE_NAME: &str = "safety-lock.toml";

/// File name of the lock file [`TrustFileTransaction`] serializes writers on,
/// beside the trust file. Holds no state — only the OS advisory lock.
///
/// This path must be STABLE: nothing may unlink it (`reset`'s sweep skips it
/// by this name). Unlinking a held lock file splits the lock — the holder
/// keeps the old inode while the next writer creates and locks a fresh one at
/// the same path, and both then believe they are exclusive.
pub const SAFETY_LOCK_LOCK_FILE_NAME: &str = "safety-lock.toml.lock";

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

/// An exclusive transaction over the trust file — the only route to a write.
///
/// Trust updates are whole-document read/modify/write operations, and two of
/// them can run at once: an approval prompted in one terminal racing a
/// `roots forget` in another, or two first-run approvals side by side. The
/// transaction serializes them with an OS advisory lock on a sibling lock
/// file ([`SAFETY_LOCK_LOCK_FILE_NAME`]), held from [`begin`](Self::begin)
/// until the value drops. The discipline a writer follows is: begin, load
/// **through the transaction** (never reuse a config read before the lock was
/// held), apply the semantic change, [`persist`](Self::persist). Re-loading
/// under the lock is what turns "write my whole stale document back" into
/// "apply my one change to the current state" — without it, an approval
/// racing a forget could resurrect the forgotten root, and one of two
/// concurrent approvals could vanish.
///
/// Reads outside a transaction stay lock-free: the replacement below is
/// atomic, so [`SafetyLockConfig::load_from`] always sees a complete document
/// — one from just before or just after a concurrent write, either of which
/// is a state the file genuinely held.
///
/// The lock file is separate from the trust file because the write replaces
/// the trust file by rename: a lock held on the replaced inode would guard a
/// file no longer at the path.
///
/// `reset` participates in the same serialization: it takes a transaction
/// before removing the trust file, so an in-flight approval either persists
/// before the wipe (and is wiped with everything else) or begins after it
/// (and applies to the empty post-reset state) — never resurrects a
/// pre-reset document. The lock file itself survives `reset` (see
/// [`SAFETY_LOCK_LOCK_FILE_NAME`]).
pub struct TrustFileTransaction {
    data_dir: PathBuf,
    /// Held for the transaction's lifetime; dropping the handle releases the
    /// OS advisory lock.
    _lock: std::fs::File,
}

impl TrustFileTransaction {
    /// Acquire the trust file's writer lock, blocking until any concurrent
    /// transaction finishes.
    ///
    /// Blocking rather than failing: a collision means another Dodot process
    /// is mid-update on a document that takes microseconds to write, and
    /// "wait your turn" is the behaviour the Spec's concurrent-attempt
    /// requirement describes — not a spurious error the user has to retry.
    ///
    /// Fails when the data directory cannot be created or the lock file
    /// cannot be opened or locked, reported as the trust file not being
    /// writable — which is what that situation is.
    pub fn begin(data_dir: &Path) -> Result<Self> {
        let not_writable = |reason: String| SafetyLockError::TrustStateNotWritable {
            path: SafetyLockConfig::path_in(data_dir),
            reason,
        };

        std::fs::create_dir_all(data_dir).map_err(|err| not_writable(err.to_string()))?;

        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(data_dir.join(SAFETY_LOCK_LOCK_FILE_NAME))
            .map_err(|err| not_writable(format!("cannot open the writer lock: {err}")))?;
        lock.lock()
            .map_err(|err| not_writable(format!("cannot acquire the writer lock: {err}")))?;

        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            _lock: lock,
        })
    }

    /// [`SafetyLockConfig::load_from`], under the lock — the state a writer's
    /// semantic change applies to.
    pub fn load(&self) -> Result<SafetyLockConfig> {
        SafetyLockConfig::load_from(&self.data_dir)
    }

    /// [`SafetyLockConfig::load_for_revocation`], under the lock — the same,
    /// for `roots forget`'s repair route.
    pub fn load_for_revocation(&self) -> Result<SafetyLockConfig> {
        SafetyLockConfig::load_for_revocation(&self.data_dir)
    }

    /// Write `config` to the trust file.
    ///
    /// The write half of the coordinates this module states once: the two
    /// load routes read that one document, and this puts it back. What stays
    /// the caller's is the *decision* to write — [`approve`](super::check::approve)
    /// and [`forget_root`](super::forget::forget_root) hand back a state
    /// rather than committing it, which is what lets the CLI record an
    /// approval **before** starting the mutation it authorizes (Spec,
    /// "Risks").
    ///
    /// Written whole rather than through clapfig's per-key persist action:
    /// the document *is* one collection, and clapfig's `Set` carries a string
    /// value per dotted key, so routing an array of identities through it
    /// would mean hand-building the TOML anyway.
    ///
    /// The replacement is atomic — a uniquely named sibling temporary file
    /// renamed over the destination — so an interrupted or failing write
    /// leaves the previous approvals intact rather than truncating them into
    /// a file every later load then fails closed on. The unique scratch name
    /// means even a writer outside this lock (a crashed process's leftover, a
    /// concurrent version that predates the lock) cannot swap its bytes under
    /// this writer's rename.
    ///
    /// Fails when the state cannot be serialized or the write or rename
    /// cannot complete. Every failure names the trust file, because that path
    /// is what the user can act on.
    pub fn persist(&self, config: &SafetyLockConfig) -> Result<()> {
        let path = SafetyLockConfig::path_in(&self.data_dir);
        let not_writable = |reason: String| SafetyLockError::TrustStateNotWritable {
            path: path.clone(),
            reason,
        };

        let document = toml::to_string(config).map_err(|err| not_writable(err.to_string()))?;

        // Same directory as the destination so the rename stays within one
        // filesystem, which is what makes it a replacement rather than a
        // copy that can fail halfway.
        let mut scratch = tempfile::Builder::new()
            .prefix(SAFETY_LOCK_FILE_NAME)
            .suffix(".new")
            .tempfile_in(&self.data_dir)
            .map_err(|err| not_writable(err.to_string()))?;
        std::io::Write::write_all(scratch.as_file_mut(), document.as_bytes())
            .map_err(|err| not_writable(err.to_string()))?;
        scratch
            .persist(&path)
            .map_err(|err| not_writable(err.to_string()))?;
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

    fn persist(config: &SafetyLockConfig, data_dir: &Path) {
        TrustFileTransaction::begin(data_dir)
            .unwrap()
            .persist(config)
            .unwrap();
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

    /// The write and the loads have to agree on the coordinates *and* the
    /// spelling, or an approval recorded now would not be found later.
    #[test]
    fn persisted_state_loads_back_byte_for_byte() {
        let data_dir = tempfile::tempdir().unwrap();
        let approved = vec![
            identity("/home/alice/dotfiles"),
            non_unicode_identity(b"\x80dots"),
        ];

        persist(
            &SafetyLockConfig {
                roots: TrustedRootsSection {
                    approved: approved.clone(),
                },
            },
            data_dir.path(),
        );

        assert_eq!(load(data_dir.path()).roots.approved, approved);
    }

    /// The data directory may not exist on a first-ever approval, and a
    /// missing parent must not be the reason a user's answer is lost.
    #[test]
    fn persisting_creates_the_data_directory() {
        let parent = tempfile::tempdir().unwrap();
        let data_dir = parent.path().join("share").join("dodot");

        persist(&SafetyLockConfig::default(), &data_dir);

        assert!(SafetyLockConfig::path_in(&data_dir).is_file());
    }

    /// The atomic replacement's point: the previous approvals survive a write
    /// that cannot complete, instead of being truncated into a file every
    /// later load fails closed on.
    #[test]
    fn a_failed_write_leaves_the_previous_approvals_intact() {
        use std::os::unix::fs::PermissionsExt;

        let data_dir = tempfile::tempdir().unwrap();
        let approved = vec![identity("/home/alice/dotfiles")];
        persist(
            &SafetyLockConfig {
                roots: TrustedRootsSection {
                    approved: approved.clone(),
                },
            },
            data_dir.path(),
        );

        // Acquire the transaction *before* revoking write permission — the
        // failure under test is the write, not the lock.
        let transaction = TrustFileTransaction::begin(data_dir.path()).unwrap();
        let set_mode = |mode: u32| {
            std::fs::set_permissions(data_dir.path(), std::fs::Permissions::from_mode(mode))
                .unwrap();
        };
        set_mode(0o555);
        let err = transaction
            .persist(&SafetyLockConfig::default())
            .unwrap_err();
        set_mode(0o755);

        assert!(
            matches!(err, SafetyLockError::TrustStateNotWritable { .. }),
            "unexpected error: {err}"
        );
        assert_eq!(load(data_dir.path()).roots.approved, approved);
    }

    /// The Spec's concurrent-attempt requirement at the storage seam: racing
    /// writers that follow the transaction's discipline — load under the
    /// lock, change, persist — compose, so every one of their updates
    /// survives. Without the lock's serialization this is the classic lost
    /// update; without loading *through* the transaction each writer would
    /// persist its own stale full document over the others'.
    #[test]
    fn racing_transactions_lose_no_update() {
        let data_dir = tempfile::tempdir().unwrap();

        let writers: Vec<_> = (0..8)
            .map(|index| {
                let data_dir = data_dir.path().to_path_buf();
                std::thread::spawn(move || {
                    let transaction = TrustFileTransaction::begin(&data_dir).unwrap();
                    let mut config = transaction.load().unwrap();
                    config
                        .roots
                        .approved
                        .push(identity(&format!("/home/alice/dots-{index}")));
                    transaction.persist(&config).unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }

        let approved = load(data_dir.path()).roots.approved;
        assert_eq!(approved.len(), 8);
        for index in 0..8 {
            assert!(
                approved.contains(&identity(&format!("/home/alice/dots-{index}"))),
                "writer {index}'s update was lost: {approved:?}"
            );
        }
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
