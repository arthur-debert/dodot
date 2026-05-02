//! Per-file `no_reverse` opt-out for the template reverse-merge.
//!
//! `[preprocessor.template] no_reverse = ["pattern", ...]` lets a user
//! exclude specific template sources from `dodot transform check`'s
//! reverse-merge engine and from the clean filter's slow path. Patterns
//! are globs, matched against the source file's *filename component*
//! (so `*.gen.tmpl` matches `app/foo.gen.tmpl`, and a fully-spelled
//! `complex-config.toml.tmpl` matches anywhere in the tree).
//!
//! The matching itself is dumb (no path walking, no recursion) — this
//! module exists mostly to localise the rule and keep the two callers
//! (transform check, template clean) consistent.

use std::path::Path;

/// Returns true iff `source_path`'s filename matches any of the
/// glob patterns in `patterns`. Patterns that fail to compile as
/// globs are silently ignored (config validation upstream is the
/// place to surface user error; from the engine's point of view a
/// bad pattern means "matches nothing").
pub fn is_no_reverse(source_path: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let Some(filename) = source_path.file_name().and_then(|f| f.to_str()) else {
        return false;
    };
    patterns.iter().any(|p| {
        glob::Pattern::new(p)
            .map(|g| g.matches(filename))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn empty_patterns_never_match() {
        let p = PathBuf::from("/dotfiles/app/config.toml.tmpl");
        assert!(!is_no_reverse(&p, &[]));
    }

    #[test]
    fn exact_filename_match() {
        let p = PathBuf::from("/dotfiles/app/complex-config.toml.tmpl");
        assert!(is_no_reverse(&p, &["complex-config.toml.tmpl".to_string()]));
        assert!(!is_no_reverse(&p, &["other-config.toml.tmpl".to_string()]));
    }

    #[test]
    fn glob_matches_by_suffix() {
        let p = PathBuf::from("/dotfiles/app/foo.gen.tmpl");
        assert!(is_no_reverse(&p, &["*.gen.tmpl".to_string()]));
    }

    #[test]
    fn glob_matches_one_of_many() {
        let p = PathBuf::from("/dotfiles/app/foo.gen.tmpl");
        let patterns = vec![
            "first.tmpl".to_string(),
            "*.gen.tmpl".to_string(),
            "third.tmpl".to_string(),
        ];
        assert!(is_no_reverse(&p, &patterns));
    }

    #[test]
    fn no_match_returns_false() {
        let p = PathBuf::from("/dotfiles/app/regular.tmpl");
        assert!(!is_no_reverse(&p, &["*.gen.tmpl".to_string()]));
    }

    #[test]
    fn invalid_glob_pattern_is_ignored() {
        // `[` opens a character class; never closing it makes the
        // pattern invalid. Matching defaults to "no match" rather
        // than blowing up.
        let p = PathBuf::from("/dotfiles/app/cfg.tmpl");
        assert!(!is_no_reverse(&p, &["[unclosed".to_string()]));
    }

    #[test]
    fn matches_against_filename_not_full_path() {
        // Glob characters in directory portions of the path don't
        // affect matching — we only look at the filename.
        let p = PathBuf::from("/dotfiles/strange-name/cfg.tmpl");
        assert!(is_no_reverse(&p, &["cfg.tmpl".to_string()]));
        assert!(!is_no_reverse(&p, &["strange-name".to_string()]));
    }
}
