//! Shared Safety Lock utilities: the reversible spelling of a native path,
//! and the filesystem probe the selection paths are injected with.
//!
//! ## Reversible spelling
//!
//! A root's identity is its canonical path in the operating system's own
//! form, stored losslessly (ADR-0001). Trust state, `roots list`, prompts,
//! and diagnostics all name a root through [`encode_native_path`], so the
//! identity a user approves is exactly the one they can inspect and revoke:
//!
//! - a path that is valid UTF-8 is spelled plainly, byte for byte;
//! - anything else is spelled as [`NATIVE_BYTES_TAG`] followed by the
//!   lowercase hex of its native bytes.
//!
//! A lossy rendering (`Path::display`, `to_string_lossy`) is never the stored
//! identity nor a match key: two roots whose lossy renderings collide keep
//! two distinct spellings and stay two records.
//!
//! The tag is only ambiguous for a plain path that itself begins with
//! `os-bytes:`. [`encode_native_path`] resolves that by hex-encoding such a
//! path too, so decoding a spelling always reproduces the exact input bytes —
//! no escape grammar, no unrepresentable path.
//!
//! ## Candidate validation
//!
//! [`canonical_root_identity`] is the one place a candidate path becomes a
//! root identity, shared by both selection paths so an environment root and an
//! implicit one are held to the same standard. It reports *which* requirement
//! failed rather than a bare boolean, because each caller phrases the refusal
//! in its own vocabulary — `DOTFILES_ROOT` names the variable, implicit
//! selection names the mechanism that chose the path.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use super::error::{Result, SafetyLockError};
use super::roots::RootIdentity;

/// Prefix marking a spelling that carries hex-encoded native path bytes
/// rather than the path's plain text.
pub const NATIVE_BYTES_TAG: &str = "os-bytes:";

/// Spell `path` reversibly: plainly when it is valid UTF-8, tagged hex
/// otherwise.
///
/// [`decode_native_path`] reverses this exactly for every input.
pub fn encode_native_path(path: &Path) -> String {
    match path.to_str() {
        Some(text) if !text.starts_with(NATIVE_BYTES_TAG) => text.to_owned(),
        _ => {
            let bytes = path.as_os_str().as_bytes();
            let mut spelling = String::with_capacity(NATIVE_BYTES_TAG.len() + bytes.len() * 2);
            spelling.push_str(NATIVE_BYTES_TAG);
            for byte in bytes {
                // Writing into a String is infallible.
                let _ = write!(spelling, "{byte:02x}");
            }
            spelling
        }
    }
}

/// Read a spelling produced by [`encode_native_path`] back into the native
/// path it names.
///
/// Untagged input is taken verbatim, so a path the user typed at the command
/// line round-trips through this function unchanged.
pub fn decode_native_path(spelling: &str) -> Result<PathBuf> {
    let Some(hex) = spelling.strip_prefix(NATIVE_BYTES_TAG) else {
        return Ok(PathBuf::from(spelling));
    };

    if !hex.len().is_multiple_of(2) {
        return Err(SafetyLockError::UnreadableSpelling {
            spelling: spelling.to_owned(),
            reason: format!(
                "the tagged encoding has an odd number of hex digits ({})",
                hex.len()
            ),
        });
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks(2) {
        let digits =
            std::str::from_utf8(pair).map_err(|_| SafetyLockError::UnreadableSpelling {
                spelling: spelling.to_owned(),
                reason: "the tagged encoding contains non-ASCII characters".to_owned(),
            })?;
        let byte =
            u8::from_str_radix(digits, 16).map_err(|_| SafetyLockError::UnreadableSpelling {
                spelling: spelling.to_owned(),
                reason: format!("`{digits}` is not a pair of hex digits"),
            })?;
        bytes.push(byte);
    }

    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

/// The filesystem questions root selection asks, injected so the selection
/// and trust logic stay testable without a real repository.
///
/// The questions come from the Spec's explicit-value rules: a candidate is
/// canonicalized once per invocation and must then be a readable directory.
/// Being a directory and being readable are asked separately because a
/// refused user is told *which* one failed — "is not a directory" and
/// "cannot be read" are different mistakes with different fixes, and one
/// boolean could not tell them apart.
pub trait PathProbe: Send + Sync {
    /// Resolve `path` to its canonical absolute form, following symlinks.
    ///
    /// This is also what separates "nothing is there" from "something is
    /// there that cannot be resolved": the two arrive as
    /// [`ErrorKind::NotFound`](std::io::ErrorKind::NotFound) and any other
    /// error respectively.
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;

    /// Whether `path` is a directory at all.
    ///
    /// Asked of an already-canonical path, so the answer is about the
    /// symlink's target rather than the link.
    fn is_directory(&self, path: &Path) -> bool;

    /// Whether `path` is a directory pack discovery could actually walk:
    /// its entries list, every entry in that listing is readable, and its
    /// children are reachable.
    ///
    /// All three are one question because discovery asks all three. A
    /// directory that opens for listing but hands back unreachable children
    /// — read permission without search permission, mode `0400` — is not a
    /// usable root, and answering "readable" for it would let the pipeline
    /// fail partway through discovery instead of at selection time.
    fn is_readable_dir(&self, path: &Path) -> bool;
}

/// Why a candidate path cannot become a root identity.
///
/// One variant per requirement [`canonical_root_identity`] checks, so a caller
/// can say what the user has to fix: a typo, a file where a directory was
/// meant, and a permission problem have three different next actions.
#[derive(Debug)]
pub(super) enum UnusableRoot {
    /// Nothing exists at the candidate path.
    Missing,
    /// Something is there, but it could not be resolved.
    Unresolvable(std::io::Error),
    /// The canonical path is not a directory.
    NotADirectory,
    /// The canonical path is a directory whose contents cannot be read.
    Unreadable,
    /// The canonical path is not a usable identity — only reachable through a
    /// [`PathProbe`] that returns something non-canonical.
    NotAnIdentity(SafetyLockError),
}

/// Canonicalize `candidate` once and check it is a directory Dodot can walk,
/// yielding the root identity built from that canonical path.
///
/// This is the whole of what "usable root" means, and both selection paths ask
/// it here rather than each spelling out the sequence: canonicalize (which is
/// also what resolves aliases and symlinks into one identity, ADR-0001), then
/// require a directory, then require one whose entries discovery could
/// actually walk.
///
/// `candidate` must already be absolute. A relative path would be resolved by
/// the operating system against the process working directory — the state
/// Safety Lock resolves once per invocation and never re-reads — so callers
/// anchor it to their injected invocation directory, or refuse it, first.
pub(super) fn canonical_root_identity(
    candidate: &Path,
    probe: &dyn PathProbe,
) -> std::result::Result<RootIdentity, UnusableRoot> {
    debug_assert!(
        candidate.is_absolute(),
        "a relative candidate would be resolved against the process working directory"
    );

    let canonical = probe.canonicalize(candidate).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            UnusableRoot::Missing
        } else {
            UnusableRoot::Unresolvable(err)
        }
    })?;

    if !probe.is_directory(&canonical) {
        return Err(UnusableRoot::NotADirectory);
    }

    if !probe.is_readable_dir(&canonical) {
        return Err(UnusableRoot::Unreadable);
    }

    RootIdentity::new(canonical).map_err(UnusableRoot::NotAnIdentity)
}

/// [`PathProbe`] backed by the real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsPathProbe;

impl PathProbe for OsPathProbe {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn is_directory(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn is_readable_dir(&self, path: &Path) -> bool {
        if !path.is_dir() {
            return false;
        }

        // A successful `read_dir` only proves the iterator opened. Search
        // ("execute") permission is a separate bit, and without it a
        // directory still hands back its names while every child is
        // unreachable — so ask the lookup discovery will ask: resolving `.`
        // *inside* the directory needs exactly the permission that walking
        // into its entries needs.
        if std::fs::metadata(path.join(".")).is_err() {
            return false;
        }

        // The listing must also survive being consumed: entries can error
        // one by one while the iterator itself opened cleanly, and a root
        // whose listing is incomplete is one discovery cannot walk either.
        match std::fs::read_dir(path) {
            Ok(mut entries) => entries.all(|entry| entry.is_ok()),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A non-Unicode path, built from bytes std will never hand back as
    /// UTF-8. `0x80` is a continuation byte with no lead byte.
    fn non_unicode_path(suffix: &[u8]) -> PathBuf {
        let mut bytes = b"/tmp/".to_vec();
        bytes.extend_from_slice(suffix);
        PathBuf::from(OsString::from_vec(bytes))
    }

    #[test]
    fn utf8_paths_spell_plainly() {
        assert_eq!(
            encode_native_path(Path::new("/home/alice/dotfiles")),
            "/home/alice/dotfiles"
        );
    }

    #[test]
    fn utf8_paths_round_trip() {
        let path = Path::new("/home/alice/dot files/über");
        let spelling = encode_native_path(path);
        assert_eq!(decode_native_path(&spelling).unwrap(), path);
    }

    #[test]
    fn non_unicode_paths_spell_tagged_and_round_trip() {
        let path = non_unicode_path(b"\x80dots");
        let spelling = encode_native_path(&path);

        assert!(
            spelling.starts_with(NATIVE_BYTES_TAG),
            "non-Unicode path spelled plainly: {spelling}"
        );
        assert_eq!(spelling, "os-bytes:2f746d702f80646f7473");
        assert_eq!(decode_native_path(&spelling).unwrap(), path);
    }

    /// The reason the identity is bytes and not a lossy rendering: two
    /// distinct roots can share one `to_string_lossy` rendering. Their
    /// spellings must stay distinct or the two roots collapse into one
    /// trust record.
    #[test]
    fn lossy_collisions_keep_distinct_spellings() {
        let one = non_unicode_path(b"\x80");
        let other = non_unicode_path(b"\x81");

        assert_eq!(
            one.to_string_lossy(),
            other.to_string_lossy(),
            "test premise: these two paths render identically when lossy"
        );
        assert_ne!(encode_native_path(&one), encode_native_path(&other));
        assert_eq!(decode_native_path(&encode_native_path(&one)).unwrap(), one);
        assert_eq!(
            decode_native_path(&encode_native_path(&other)).unwrap(),
            other
        );
    }

    /// A plain path that begins with the tag would otherwise decode as
    /// tagged hex. Encoding hex-encodes it instead, so reversibility holds
    /// for every input rather than for realistic inputs only.
    #[test]
    fn plain_text_starting_with_the_tag_is_encoded_not_confused() {
        let path = PathBuf::from(format!("{NATIVE_BYTES_TAG}deadbeef"));
        let spelling = encode_native_path(&path);

        assert_ne!(spelling, path.to_str().unwrap());
        assert_eq!(decode_native_path(&spelling).unwrap(), path);
    }

    #[test]
    fn truncated_tagged_spelling_is_rejected() {
        let err = decode_native_path("os-bytes:2f7").unwrap_err();
        assert!(
            matches!(err, SafetyLockError::UnreadableSpelling { .. }),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn non_hex_tagged_spelling_is_rejected() {
        let err = decode_native_path("os-bytes:zz").unwrap_err();
        assert!(
            matches!(err, SafetyLockError::UnreadableSpelling { .. }),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn os_probe_canonicalizes_and_reports_readable_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let probe = OsPathProbe;

        let canonical = probe.canonicalize(dir.path()).unwrap();
        assert!(probe.is_directory(&canonical));
        assert!(probe.is_readable_dir(&canonical));

        let file = canonical.join("not-a-dir");
        std::fs::write(&file, b"").unwrap();
        assert!(!probe.is_directory(&file));
        assert!(!probe.is_readable_dir(&file));
        assert!(!probe.is_directory(&canonical.join("missing")));
        assert!(!probe.is_readable_dir(&canonical.join("missing")));
    }

    /// Build `name` under `parent` holding one entry, at `mode`, and ask the
    /// probe both directory questions about it — restoring the permissions
    /// before returning so a failed assertion in the caller cannot leave the
    /// directory unremovable for `TempDir`'s cleanup.
    fn probe_dir_at_mode(parent: &Path, name: &str, mode: u32) -> (bool, bool) {
        use std::os::unix::fs::PermissionsExt;

        let dir = parent.join(name);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("pack"), b"").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();

        let probe = OsPathProbe;
        let answers = (probe.is_directory(&dir), probe.is_readable_dir(&dir));

        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        answers
    }

    /// Whether `chmod` actually blocks this process. Running as root — or
    /// with `CAP_DAC_OVERRIDE` in a container — bypasses the permission bits
    /// entirely, which would make the cases below vacuous rather than wrong,
    /// so those runs skip.
    fn chmod_blocks_this_process(parent: &Path) -> bool {
        let (_, readable) = probe_dir_at_mode(parent, "dac-probe", 0o000);
        !readable
    }

    /// The two directory questions are asked separately so a diagnostic can
    /// say which one failed: a mode-000 directory *is* a directory, and only
    /// the reading fails.
    #[test]
    fn os_probe_separates_being_a_directory_from_being_readable() {
        let parent = tempfile::tempdir().unwrap();
        if !chmod_blocks_this_process(parent.path()) {
            eprintln!(
                "skipping os_probe_separates_being_a_directory_from_being_readable: \
                 process bypasses DAC permissions (running as root?)"
            );
            return;
        }

        let (is_dir, readable) = probe_dir_at_mode(parent.path(), "locked", 0o000);
        assert!(is_dir);
        assert!(!readable);
    }

    /// Read permission without search permission is the case a plain
    /// `read_dir(..).is_ok()` gets wrong: the listing opens and the names
    /// come back, but every child is unreachable, so discovery would fail
    /// entry by entry on a root selection had already blessed.
    #[test]
    fn os_probe_rejects_a_directory_it_can_list_but_not_walk() {
        let parent = tempfile::tempdir().unwrap();
        if !chmod_blocks_this_process(parent.path()) {
            eprintln!(
                "skipping os_probe_rejects_a_directory_it_can_list_but_not_walk: \
                 process bypasses DAC permissions (running as root?)"
            );
            return;
        }

        let (is_dir, readable) = probe_dir_at_mode(parent.path(), "no-search", 0o400);
        assert!(is_dir, "a mode-0400 directory is still a directory");
        assert!(
            !readable,
            "a directory whose children cannot be reached is not a usable root"
        );

        // The ordinary case still passes: the stricter probe rejects the
        // unwalkable directory, not every directory.
        let (is_dir, readable) = probe_dir_at_mode(parent.path(), "ordinary", 0o700);
        assert!(is_dir);
        assert!(readable);
    }
}
