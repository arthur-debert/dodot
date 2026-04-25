//! Pre-flight syntax check for shell-sourced files.
//!
//! Runs `bash -n` / `zsh -n` against each deployed shell source so that
//! syntax errors in `aliases.sh`, `profile.zsh` etc. surface at
//! `dodot up` time instead of silently breaking the user's next shell
//! startup. The interpreter's stderr (which carries `file: line N:
//! error_message`) is preserved verbatim into a sidecar file under the
//! handler datastore so `dodot status` can show it later (3c).
//!
//! This module does not invoke the staged file. It only parses it.
//! `bash -n` / `zsh -n` are syntax-only — no commands run, no side
//! effects on the user's environment.
//!
//! Sidecar layout: `<data_dir>/packs/<pack>/shell/.errors/<filename>.err`
//! - Written on a fresh syntax failure.
//! - Removed on a fresh syntax success (so a fix clears the prior error).
//! - Untouched when the interpreter is missing (we have no info either way).
//!
//! Init-script generation already filters non-symlinks, so the
//! `.errors` subdirectory does not end up sourced at shell startup.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

/// Run a syntax-only check on a shell file.
///
/// Implementations must not run the file or alter the environment;
/// they invoke the interpreter in parse-only mode and return what it
/// found.
pub trait SyntaxChecker: Send + Sync {
    fn check(&self, interpreter: &str, file: &Path) -> SyntaxCheckResult;
}

#[derive(Debug, Clone)]
pub enum SyntaxCheckResult {
    /// Interpreter parsed the file with no errors.
    Ok,
    /// Interpreter reported syntax errors. `stderr` carries the
    /// raw output (with line/column information from the shell).
    SyntaxError { stderr: String },
    /// The interpreter binary was not found on PATH.
    InterpreterMissing,
}

/// Production [`SyntaxChecker`] that spawns real subprocesses.
pub struct SystemSyntaxChecker;

/// [`SyntaxChecker`] that always returns [`SyntaxCheckResult::Ok`].
/// Used in tests to keep the validation pass deterministic and
/// hermetic (no real `bash`/`zsh` invocations).
pub struct NoopSyntaxChecker;

impl SyntaxChecker for NoopSyntaxChecker {
    fn check(&self, _interpreter: &str, _file: &Path) -> SyntaxCheckResult {
        SyntaxCheckResult::Ok
    }
}

impl SyntaxChecker for SystemSyntaxChecker {
    fn check(&self, interpreter: &str, file: &Path) -> SyntaxCheckResult {
        match std::process::Command::new(interpreter)
            .arg("-n")
            .arg(file)
            .output()
        {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                SyntaxCheckResult::InterpreterMissing
            }
            Err(e) => SyntaxCheckResult::SyntaxError {
                stderr: format!("dodot: failed to spawn {interpreter}: {e}\n"),
            },
            Ok(output) if output.status.success() => SyntaxCheckResult::Ok,
            Ok(output) => SyntaxCheckResult::SyntaxError {
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            },
        }
    }
}

/// Pick the interpreter to validate `file` with based on its extension.
/// Returns `None` for files we don't know how to syntax-check (the
/// caller should skip those).
fn interpreter_for(file: &Path) -> Option<&'static str> {
    match file.extension().and_then(|e| e.to_str()) {
        Some("zsh") => Some("zsh"),
        Some("bash") => Some("bash"),
        // bash is the most permissive POSIX-compatible parser. A user
        // whose interactive shell is dash/sh and who relies on POSIX
        // strictness will need to verify themselves; bash -n still
        // catches the bulk of real-world syntax errors in `.sh` files.
        Some("sh") => Some("bash"),
        _ => None,
    }
}

/// Summary of one validation pass over the deployed shell sources.
#[derive(Debug, Default)]
pub struct ShellValidationReport {
    /// Total files inspected (matched a known shell extension).
    pub checked: usize,
    /// Per-failure detail for callers that want to render or log them.
    pub failures: Vec<ShellValidationFailure>,
    /// Interpreters we tried to spawn but couldn't find. Each entry is
    /// recorded once even if many files needed it.
    pub missing_interpreters: BTreeSet<String>,
}

/// One file that failed pre-flight syntax check.
#[derive(Debug, Clone)]
pub struct ShellValidationFailure {
    pub pack: String,
    pub source: PathBuf,
    pub stderr: String,
}

/// Subdirectory (under each pack's shell handler dir) where sidecar
/// `.err` files live. Public so 3c (`status`) can read it back.
pub const ERRORS_SUBDIR: &str = ".errors";

/// Path of the sidecar error file for one source.
pub fn error_sidecar_path(paths: &dyn Pather, pack: &str, source_filename: &str) -> PathBuf {
    paths
        .handler_data_dir(pack, "shell")
        .join(ERRORS_SUBDIR)
        .join(format!("{source_filename}.err"))
}

/// Iterate every deployed shell source, run a syntax check, and update
/// the per-file sidecar files. Idempotent across runs: a previously
/// failing file that's been fixed gets its sidecar removed.
pub fn validate_shell_sources(
    fs: &dyn Fs,
    paths: &dyn Pather,
    checker: &dyn SyntaxChecker,
) -> Result<ShellValidationReport> {
    let mut report = ShellValidationReport::default();

    let packs_dir = paths.data_dir().join("packs");
    if !fs.exists(&packs_dir) {
        return Ok(report);
    }

    for pack_entry in fs.read_dir(&packs_dir)? {
        if !pack_entry.is_dir {
            continue;
        }
        let pack_name = &pack_entry.name;
        let shell_dir = paths.handler_data_dir(pack_name, "shell");
        if !fs.is_dir(&shell_dir) {
            continue;
        }
        let errors_dir = shell_dir.join(ERRORS_SUBDIR);

        let entries = match fs.read_dir(&shell_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries {
            if !entry.is_symlink {
                continue;
            }
            let source = match fs.readlink(&entry.path) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let interpreter = match interpreter_for(&source) {
                Some(i) => i,
                None => continue,
            };

            let filename = source
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let err_path = errors_dir.join(format!("{filename}.err"));

            report.checked += 1;
            match checker.check(interpreter, &source) {
                SyntaxCheckResult::Ok => {
                    // Clear any stale sidecar from a previous failure.
                    if fs.exists(&err_path) {
                        let _ = fs.remove_file(&err_path);
                    }
                }
                SyntaxCheckResult::SyntaxError { stderr } => {
                    fs.mkdir_all(&errors_dir)?;
                    fs.write_file(&err_path, stderr.as_bytes())?;
                    report.failures.push(ShellValidationFailure {
                        pack: pack_name.clone(),
                        source: source.clone(),
                        stderr,
                    });
                }
                SyntaxCheckResult::InterpreterMissing => {
                    report.missing_interpreters.insert(interpreter.to_string());
                }
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner, DataStore, FilesystemDataStore};
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct NoopRunner;
    impl CommandRunner for NoopRunner {
        fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput> {
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn make_datastore(env: &TempEnvironment) -> FilesystemDataStore {
        FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), Arc::new(NoopRunner))
    }

    /// Test checker: returns canned results keyed by source filename
    /// (basename), so tests can target individual files without caring
    /// about absolute paths.
    #[derive(Default)]
    struct CannedChecker {
        results: Mutex<HashMap<String, SyntaxCheckResult>>,
        calls: Mutex<Vec<(String, PathBuf)>>,
    }
    impl CannedChecker {
        fn set(&self, filename: &str, result: SyntaxCheckResult) {
            self.results
                .lock()
                .unwrap()
                .insert(filename.to_string(), result);
        }
        fn calls(&self) -> Vec<(String, PathBuf)> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl SyntaxChecker for CannedChecker {
        fn check(&self, interpreter: &str, file: &Path) -> SyntaxCheckResult {
            let basename = file
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.calls
                .lock()
                .unwrap()
                .push((interpreter.to_string(), file.to_path_buf()));
            self.results
                .lock()
                .unwrap()
                .get(&basename)
                .cloned()
                .unwrap_or(SyntaxCheckResult::Ok)
        }
    }

    #[test]
    fn interpreter_picked_per_extension() {
        assert_eq!(interpreter_for(Path::new("a.sh")), Some("bash"));
        assert_eq!(interpreter_for(Path::new("a.bash")), Some("bash"));
        assert_eq!(interpreter_for(Path::new("a.zsh")), Some("zsh"));
        assert_eq!(interpreter_for(Path::new("a.fish")), None);
        assert_eq!(interpreter_for(Path::new("Makefile")), None);
    }

    #[test]
    fn validates_each_deployed_shell_file() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .file("env.zsh", "export FOO=bar")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/env.zsh"))
            .unwrap();

        let checker = CannedChecker::default();
        let report = validate_shell_sources(env.fs.as_ref(), env.paths.as_ref(), &checker).unwrap();

        assert_eq!(report.checked, 2);
        assert!(report.failures.is_empty());
        assert!(report.missing_interpreters.is_empty());

        let calls = checker.calls();
        let interpreters: Vec<&String> = calls.iter().map(|(i, _)| i).collect();
        assert!(interpreters.contains(&&"bash".to_string()));
        assert!(interpreters.contains(&&"zsh".to_string()));
    }

    #[test]
    fn syntax_failure_writes_sidecar_with_stderr() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "if [ x = y\nfi\n")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let checker = CannedChecker::default();
        checker.set(
            "aliases.sh",
            SyntaxCheckResult::SyntaxError {
                stderr: "aliases.sh: line 2: syntax error near `fi'\n".into(),
            },
        );

        let report = validate_shell_sources(env.fs.as_ref(), env.paths.as_ref(), &checker).unwrap();

        assert_eq!(report.checked, 1);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].pack, "vim");

        let sidecar = error_sidecar_path(env.paths.as_ref(), "vim", "aliases.sh");
        assert!(env.fs.exists(&sidecar));
        let body = env.fs.read_to_string(&sidecar).unwrap();
        assert!(body.contains("syntax error near"), "sidecar:\n{body}");
    }

    #[test]
    fn fixed_syntax_clears_stale_sidecar() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        // First run: failure → sidecar written.
        let bad = CannedChecker::default();
        bad.set(
            "aliases.sh",
            SyntaxCheckResult::SyntaxError {
                stderr: "aliases.sh: line 1: oops\n".into(),
            },
        );
        validate_shell_sources(env.fs.as_ref(), env.paths.as_ref(), &bad).unwrap();
        let sidecar = error_sidecar_path(env.paths.as_ref(), "vim", "aliases.sh");
        assert!(env.fs.exists(&sidecar));

        // Second run: success → sidecar removed.
        let good = CannedChecker::default();
        let report = validate_shell_sources(env.fs.as_ref(), env.paths.as_ref(), &good).unwrap();
        assert_eq!(report.checked, 1);
        assert!(report.failures.is_empty());
        assert!(!env.fs.exists(&sidecar));
    }

    #[test]
    fn missing_interpreter_recorded_and_sidecar_left_alone() {
        // If we previously had a sidecar (e.g., from a prior failure)
        // and the interpreter goes missing, we don't clobber the
        // sidecar — we have no fresh info, so we leave the old verdict.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.zsh", "alias vi=vim")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.zsh"))
            .unwrap();

        // Prime a stale sidecar.
        let sidecar = error_sidecar_path(env.paths.as_ref(), "vim", "aliases.zsh");
        env.fs.mkdir_all(sidecar.parent().unwrap()).unwrap();
        env.fs.write_file(&sidecar, b"old failure\n").unwrap();

        let checker = CannedChecker::default();
        checker.set("aliases.zsh", SyntaxCheckResult::InterpreterMissing);

        let report = validate_shell_sources(env.fs.as_ref(), env.paths.as_ref(), &checker).unwrap();

        assert_eq!(report.checked, 1);
        assert!(report.failures.is_empty());
        assert!(report.missing_interpreters.contains("zsh"));
        // Sidecar untouched.
        assert!(env.fs.exists(&sidecar));
        assert_eq!(env.fs.read_to_string(&sidecar).unwrap(), "old failure\n");
    }

    #[test]
    fn unknown_extensions_are_skipped() {
        // A file the symlink handler caught (because it didn't match
        // shell mappings) ends up under handler dir "symlink", not
        // "shell" — so this test really only covers the case where
        // someone manually places a non-shell-extension file under the
        // shell handler. We still avoid running bash on a `.fish` file.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("config.fish", "set -x FOO bar")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/config.fish"))
            .unwrap();

        let checker = CannedChecker::default();
        let report = validate_shell_sources(env.fs.as_ref(), env.paths.as_ref(), &checker).unwrap();

        assert_eq!(report.checked, 0);
        assert!(checker.calls().is_empty());
    }

    #[test]
    fn empty_datastore_is_ok() {
        let env = TempEnvironment::builder().build();
        let checker = CannedChecker::default();
        let report = validate_shell_sources(env.fs.as_ref(), env.paths.as_ref(), &checker).unwrap();
        assert_eq!(report.checked, 0);
    }
}
