//! macOS-native metadata probes: `mdls` and `mdfind`.
//!
//! Implements Phase M6 of `docs/proposals/macos-paths.lex` §8.3. Same
//! advisory-only contract as `probe::brew`: failures return `None`
//! and never propagate; the resolver in §5 doesn't consult these
//! either.
//!
//! ## Why these and not the others
//!
//! - `mdls` — Spotlight metadata for a single bundle path. Cheap,
//!   scriptable. Used to resolve `/Applications/<X>.app` to its
//!   `kMDItemCFBundleIdentifier` (e.g. `com.microsoft.VSCode`).
//! - `mdfind` — Spotlight query. Used to resolve a friendly display
//!   name ("Cursor") to a `.app` bundle path when only the name is
//!   known.
//!
//! `lsregister -dump` (full LaunchServices DB) is comprehensive but
//! heavy and out of scope. `defaults domains` is for plist preference
//! domains, also out of scope. `NSFileManager.URLsForDirectory(...)`
//! would require linking AppKit and isn't worth the dependency
//! burden.

use std::path::Path;

use crate::datastore::CommandRunner;

/// Read the bundle identifier from a `.app` bundle's Spotlight
/// metadata. Returns `None` on non-macOS, missing `mdls`, missing
/// bundle, or unparseable output.
///
/// Example mdls output:
///
/// ```text
/// kMDItemCFBundleIdentifier = "com.microsoft.VSCode"
/// ```
pub fn bundle_id(app_path: &Path, runner: &dyn CommandRunner) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = runner
        .run(
            "mdls",
            &[
                "-name".into(),
                "kMDItemCFBundleIdentifier".into(),
                app_path.to_string_lossy().into_owned(),
            ],
        )
        .ok()?;
    if output.exit_code != 0 {
        return None;
    }
    parse_mdls_value(&output.stdout)
}

/// Resolve a friendly display name (e.g. `"Cursor"`) to a `.app`
/// bundle path via Spotlight. Returns the first match.
///
/// We construct the query at the call site rather than letting the
/// caller pass arbitrary mdfind args — keeps the surface narrow and
/// shell-injection safe (CommandRunner already isolates args from
/// the shell, but query simplicity matters for testability too).
pub fn find_app_bundle(display_name: &str, runner: &dyn CommandRunner) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let query = format!(
        "kMDItemKind == 'Application' && kMDItemDisplayName == '{}'",
        display_name.replace('\'', "")
    );
    let output = runner.run("mdfind", &[query]).ok()?;
    if output.exit_code != 0 {
        return None;
    }
    output
        .stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Parse `mdls -name kMDItemCFBundleIdentifier <path>` output.
///
/// Handles both the typical quoted form (`= "com.x.y"`) and the
/// `(null)` form mdls emits when a bundle has no value set for the
/// requested key.
fn parse_mdls_value(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        if let Some((_key, rest)) = line.split_once('=') {
            let trimmed = rest.trim();
            if trimmed == "(null)" || trimmed.is_empty() {
                return None;
            }
            // Strip surrounding double quotes if present.
            let unquoted = trimmed
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(trimmed);
            return Some(unquoted.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::CommandOutput;
    use crate::Result;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Minimal CommandRunner mock — duplicated from probe::brew::tests
    /// rather than shared via a test_util module so each probe's tests
    /// stay self-contained.
    struct MockRunner {
        responses: Mutex<HashMap<Vec<String>, CommandOutput>>,
    }

    impl MockRunner {
        fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
            }
        }
        fn respond(&self, args: &[&str], stdout: &str, exit_code: i32) {
            let key: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            self.responses.lock().unwrap().insert(
                key,
                CommandOutput {
                    exit_code,
                    stdout: stdout.into(),
                    stderr: String::new(),
                },
            );
        }
    }

    impl CommandRunner for MockRunner {
        fn run(&self, exe: &str, args: &[String]) -> Result<CommandOutput> {
            let mut full = vec![exe.to_string()];
            full.extend(args.iter().cloned());
            let key: Vec<String> = full.iter().skip(1).cloned().collect();
            self.responses
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .ok_or_else(|| crate::DodotError::Other(format!("no mock response for {full:?}")))
        }
    }

    #[test]
    fn parse_mdls_value_quoted() {
        let out = "kMDItemCFBundleIdentifier = \"com.microsoft.VSCode\"";
        assert_eq!(
            parse_mdls_value(out).as_deref(),
            Some("com.microsoft.VSCode")
        );
    }

    #[test]
    fn parse_mdls_value_null() {
        // mdls emits `(null)` for absent metadata.
        assert_eq!(parse_mdls_value("kMDItemCFBundleIdentifier = (null)"), None);
    }

    #[test]
    fn parse_mdls_value_unquoted() {
        // Some keys (e.g. integers) come back unquoted; we tolerate it.
        let out = "kMDItemPixelHeight = 1080";
        assert_eq!(parse_mdls_value(out).as_deref(), Some("1080"));
    }

    #[test]
    fn parse_mdls_value_empty_input() {
        assert_eq!(parse_mdls_value(""), None);
        assert_eq!(parse_mdls_value("no-equals-sign-here"), None);
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn bundle_id_returns_value_on_success() {
        let runner = MockRunner::new();
        runner.respond(
            &[
                "-name",
                "kMDItemCFBundleIdentifier",
                "/Applications/Visual Studio Code.app",
            ],
            "kMDItemCFBundleIdentifier = \"com.microsoft.VSCode\"\n",
            0,
        );
        let got = bundle_id(Path::new("/Applications/Visual Studio Code.app"), &runner);
        assert_eq!(got.as_deref(), Some("com.microsoft.VSCode"));
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn bundle_id_returns_none_on_nonzero_exit() {
        let runner = MockRunner::new();
        runner.respond(
            &[
                "-name",
                "kMDItemCFBundleIdentifier",
                "/Applications/Missing.app",
            ],
            "",
            1,
        );
        let got = bundle_id(Path::new("/Applications/Missing.app"), &runner);
        assert!(got.is_none());
    }

    #[test]
    fn bundle_id_silent_on_non_macos() {
        let runner = MockRunner::new();
        // No mock response set; on non-macOS we exit early before
        // ever calling runner.run, so no error surfaces.
        if !cfg!(target_os = "macos") {
            assert!(bundle_id(Path::new("/Applications/X.app"), &runner).is_none());
        }
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn find_app_bundle_returns_first_path() {
        let runner = MockRunner::new();
        runner.respond(
            &["kMDItemKind == 'Application' && kMDItemDisplayName == 'Cursor'"],
            "/Applications/Cursor.app\n/Applications/Old/Cursor.app\n",
            0,
        );
        let got = find_app_bundle("Cursor", &runner);
        assert_eq!(got.as_deref(), Some("/Applications/Cursor.app"));
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn find_app_bundle_returns_none_when_no_results() {
        let runner = MockRunner::new();
        runner.respond(
            &["kMDItemKind == 'Application' && kMDItemDisplayName == 'NonexistentApp'"],
            "",
            0,
        );
        let got = find_app_bundle("NonexistentApp", &runner);
        assert!(got.is_none());
    }
}
