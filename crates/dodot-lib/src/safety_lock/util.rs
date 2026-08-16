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

use std::ffi::OsString;
use std::fmt::Write as _;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use super::error::{Result, SafetyLockError};

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

    /// Whether `path` is a directory whose entries can be listed.
    ///
    /// A directory that exists but cannot be read is not a usable root: the
    /// pipeline would fail partway through pack discovery instead of at
    /// selection time.
    fn is_readable_dir(&self, path: &Path) -> bool;
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
        path.is_dir() && std::fs::read_dir(path).is_ok()
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

    /// The two directory questions are asked separately so a diagnostic can
    /// say which one failed: a mode-000 directory *is* a directory, and only
    /// the listing fails.
    #[test]
    fn os_probe_separates_being_a_directory_from_being_readable() {
        use std::os::unix::fs::PermissionsExt;

        // Running as root defeats the permission bits entirely.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }

        let parent = tempfile::tempdir().unwrap();
        let locked = parent.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        let probe = OsPathProbe;
        assert!(probe.is_directory(&locked));
        assert!(!probe.is_readable_dir(&locked));

        // Let the TempDir clean itself up.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}
