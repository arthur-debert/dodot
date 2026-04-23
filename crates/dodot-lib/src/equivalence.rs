//! Equivalence detection for "would deploying make any content change?"
//!
//! When dodot's deploy would result in the same content reaching the
//! user's target path, the existing file/symlink can be safely replaced
//! with dodot's standard chain without prompting. This is the
//! "no-content-change is no-conflict" principle from issue #44.
//!
//! Two cases qualify as equivalent:
//!
//! - **Direct (single-hop) symlink** whose target is exactly `source`.
//!   `up` will replace `user_path → source` with `user_path → data_link
//!   → source`. Same realpath, same content reaches the same path.
//!
//! - **Regular file** whose byte content matches `source` exactly.
//!   `up` will replace the file with a symlink to the data_link. The
//!   content the user reads stays bit-identical.
//!
//! Multi-hop symlink chains are deliberately *not* treated as
//! equivalent even if their realpath matches `source`. The chain
//! probably exists for a reason and we shouldn't second-guess it.

use std::path::Path;

use crate::fs::Fs;

/// Whether the existing thing at `user_path` is content-equivalent to
/// `source` — meaning `dodot up` would produce the same content
/// reaching the same path, so it's safe to replace without `--force`.
///
/// See module-level docs for the exact equivalence rules.
pub fn is_equivalent(user_path: &Path, source: &Path, fs: &dyn Fs) -> bool {
    if fs.is_symlink(user_path) {
        // Single-hop direct symlink to source.
        // Multi-hop or pointing elsewhere falls through to false.
        match fs.readlink(user_path) {
            Ok(target) => target == source,
            Err(_) => false,
        }
    } else if fs.exists(user_path) && !fs.is_dir(user_path) {
        // Regular file: byte equality with source.
        match (fs.read_file(user_path), fs.read_file(source)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    } else {
        // Absent, directory, or unreadable.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    #[test]
    fn direct_symlink_to_source_is_equivalent() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");
        env.fs.symlink(&source, &user_path).unwrap();

        assert!(is_equivalent(&user_path, &source, env.fs.as_ref()));
    }

    #[test]
    fn symlink_pointing_elsewhere_is_not_equivalent() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");
        env.fs
            .symlink(std::path::Path::new("/tmp/somewhere-else"), &user_path)
            .unwrap();

        assert!(!is_equivalent(&user_path, &source, env.fs.as_ref()));
    }

    #[test]
    fn multi_hop_symlink_to_source_is_not_equivalent() {
        // Even though the realpath matches, the chain exists for a
        // reason — leave it alone.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();

        let source = env.dotfiles_root.join("vim/vimrc");
        let intermediate = env.home.join(".vimrc.intermediate");
        let user_path = env.home.join(".vimrc");
        env.fs.symlink(&source, &intermediate).unwrap();
        env.fs.symlink(&intermediate, &user_path).unwrap();

        assert!(!is_equivalent(&user_path, &source, env.fs.as_ref()));
    }

    #[test]
    fn regular_file_with_identical_content_is_equivalent() {
        let env = TempEnvironment::builder()
            .pack("git")
            .file("gitconfig", "[user]\n  name = test")
            .done()
            .home_file(".gitconfig", "[user]\n  name = test")
            .build();

        let source = env.dotfiles_root.join("git/gitconfig");
        let user_path = env.home.join(".gitconfig");

        assert!(is_equivalent(&user_path, &source, env.fs.as_ref()));
    }

    #[test]
    fn regular_file_with_different_content_is_not_equivalent() {
        let env = TempEnvironment::builder()
            .pack("git")
            .file("gitconfig", "[user]\n  name = new")
            .done()
            .home_file(".gitconfig", "[user]\n  name = old")
            .build();

        let source = env.dotfiles_root.join("git/gitconfig");
        let user_path = env.home.join(".gitconfig");

        assert!(!is_equivalent(&user_path, &source, env.fs.as_ref()));
    }

    #[test]
    fn absent_user_path_is_not_equivalent() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");
        // No file created at user_path.

        assert!(!is_equivalent(&user_path, &source, env.fs.as_ref()));
    }

    #[test]
    fn directory_is_not_equivalent() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");
        env.fs.mkdir_all(&user_path).unwrap();

        assert!(!is_equivalent(&user_path, &source, env.fs.as_ref()));
    }

    #[test]
    fn broken_symlink_is_not_equivalent() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");
        env.fs
            .symlink(std::path::Path::new("/does/not/exist"), &user_path)
            .unwrap();

        assert!(!is_equivalent(&user_path, &source, env.fs.as_ref()));
    }
}
