//! Executor — converts [`HandlerIntent`]s into [`DataStore`] calls.
//!
//! The executor is where the complexity lives. Handlers just declare
//! what they want; the executor figures out how to make it happen.
//!
//! Per-intent logic lives in sibling files: [`mod@link`] for symlink
//! deployment (with ancestor-cycle and conflict handling), [`mod@stage`]
//! for datastore staging (with auto-chmod for path handler bins), and
//! [`mod@run`] for sentinel-gated command execution. This file owns the
//! Executor struct, the per-call `execute()` entry point, and the
//! match-based dispatchers (`execute_one`, `simulate`).
//!
//! ## Auto-executable permissions
//!
//! When `auto_chmod_exec` is enabled (the default), the executor
//! ensures that files inside path-handler staged directories have
//! execute permissions (`+x`). This matches the user's intent: files
//! in `bin/` are there to be runnable, but execute bits can be lost
//! in common workflows (git on macOS, manual file creation).
//!
//! Permission failures are reported as warnings in the operation
//! results, not hard errors — the file is still staged and added to
//! `$PATH`, it just won't be directly runnable until the user fixes
//! permissions manually.

mod fetch;
mod link;
mod run;
mod stage;

use tracing::debug;

use crate::datastore::DataStore;
use crate::external::{GitRunner, HttpFetcher};
use crate::fs::Fs;
use crate::operations::{HandlerIntent, OperationResult};
use crate::paths::Pather;
use crate::Result;

/// Executes handler intents by dispatching to the DataStore.
pub struct Executor<'a> {
    datastore: &'a dyn DataStore,
    fs: &'a dyn Fs,
    paths: &'a dyn Pather,
    dry_run: bool,
    force: bool,
    provision_rerun: bool,
    auto_chmod_exec: bool,
    /// HTTP-ish fetcher used for `HandlerIntent::Fetch`. Optional so
    /// the 20+ test sites that don't exercise externals can keep
    /// constructing `Executor::new(...)` unchanged; the fetch
    /// dispatcher errors loudly if it sees a Fetch intent without
    /// one. Production wiring sets this via [`Self::with_fetcher`].
    fetcher: Option<&'a dyn HttpFetcher>,
    /// Git runner for `git-repo` externals. Same opt-in posture as
    /// [`Self::fetcher`].
    git: Option<&'a dyn GitRunner>,
}

impl<'a> Executor<'a> {
    pub fn new(
        datastore: &'a dyn DataStore,
        fs: &'a dyn Fs,
        paths: &'a dyn Pather,
        dry_run: bool,
        force: bool,
        provision_rerun: bool,
        auto_chmod_exec: bool,
    ) -> Self {
        Self {
            datastore,
            fs,
            paths,
            dry_run,
            force,
            provision_rerun,
            auto_chmod_exec,
            fetcher: None,
            git: None,
        }
    }

    /// Builder-style: install the HTTP fetcher used by Fetch intents.
    pub fn with_fetcher(mut self, fetcher: &'a dyn HttpFetcher) -> Self {
        self.fetcher = Some(fetcher);
        self
    }

    /// Builder-style: install the git runner used by `git-repo`
    /// externals.
    pub fn with_git(mut self, git: &'a dyn GitRunner) -> Self {
        self.git = Some(git);
        self
    }

    /// Accessor for the fetch dispatcher.
    pub(super) fn fetcher(&self) -> Option<&'a dyn HttpFetcher> {
        self.fetcher
    }

    /// Accessor for the fetch dispatcher (git side).
    pub(super) fn git(&self) -> Option<&'a dyn GitRunner> {
        self.git
    }

    /// Execute a list of handler intents, returning one result per
    /// atomic operation performed.
    ///
    /// Conflicts (pre-existing files at target paths) are returned as
    /// failed `OperationResult`s — non-fatal, so other intents still
    /// execute. Hard errors (I/O failures, command failures) stop
    /// execution immediately via `?`.
    /// In dry-run mode, all intents are simulated regardless of errors.
    pub fn execute(&self, intents: Vec<HandlerIntent>) -> Result<Vec<OperationResult>> {
        debug!(
            count = intents.len(),
            dry_run = self.dry_run,
            force = self.force,
            "executor starting"
        );
        let mut results = Vec::new();

        for intent in intents {
            let intent_results = if self.dry_run {
                self.simulate(&intent)
            } else {
                self.execute_one(&intent)?
            };
            results.extend(intent_results);
        }

        let succeeded = results.iter().filter(|r| r.success).count();
        let failed = results.iter().filter(|r| !r.success).count();
        debug!(succeeded, failed, "executor finished");

        Ok(results)
    }

    /// Execute a single intent, which may produce multiple operations.
    fn execute_one(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        match intent {
            HandlerIntent::Link { .. } => self.execute_link(intent),
            HandlerIntent::Stage { .. } => self.execute_stage(intent),
            HandlerIntent::Run { .. } => self.execute_run(intent),
            HandlerIntent::Fetch { .. } => self.execute_fetch(intent),
        }
    }

    /// Simulate an intent without touching the filesystem.
    fn simulate(&self, intent: &HandlerIntent) -> Vec<OperationResult> {
        match intent {
            HandlerIntent::Link { .. } => self.simulate_link(intent),
            HandlerIntent::Stage { .. } => self.simulate_stage(intent),
            HandlerIntent::Run { .. } => self.simulate_run(intent),
            HandlerIntent::Fetch { .. } => self.simulate_fetch(intent),
        }
    }
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests {
    //! Cross-intent dispatcher tests. Per-intent test suites live next
    //! to their implementation in `link.rs`, `stage.rs`, and `run.rs`.

    use super::test_support::make_datastore;
    use super::Executor;
    use crate::operations::HandlerIntent;
    use crate::testing::TempEnvironment;

    #[test]
    fn dry_run_does_not_modify_filesystem() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true,
            false,
            false,
            true,
        );

        let results = executor
            .execute(vec![
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                    user_path: env.home.join(".vimrc"),
                },
                HandlerIntent::Stage {
                    pack: "vim".into(),
                    handler: "shell".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                },
                HandlerIntent::Run {
                    pack: "vim".into(),
                    handler: "install".into(),
                    executable: "echo".into(),
                    arguments: vec!["hi".into()],
                    sentinel: "s1".into(),
                },
            ])
            .unwrap();

        // All should succeed with dry-run messages
        assert_eq!(results.len(), 3); // Link=1, Stage=1, Run=1
        for r in &results {
            assert!(r.success);
            assert!(r.message.contains("[dry-run]"), "msg: {}", r.message);
        }

        // Nothing should have been created
        env.assert_not_exists(&env.home.join(".vimrc"));
        env.assert_no_handler_state("vim", "symlink");
        env.assert_no_handler_state("vim", "shell");
        env.assert_no_handler_state("vim", "install");
    }

    #[test]
    fn execute_multiple_intents_sequentially() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let results = executor
            .execute(vec![
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                    user_path: env.home.join(".vimrc"),
                },
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/gvimrc"),
                    user_path: env.home.join(".gvimrc"),
                },
            ])
            .unwrap();

        assert_eq!(results.len(), 2); // 1 op per link
        assert!(results.iter().all(|r| r.success));

        env.assert_double_link(
            "vim",
            "symlink",
            "vimrc",
            &env.dotfiles_root.join("vim/vimrc"),
            &env.home.join(".vimrc"),
        );
        env.assert_double_link(
            "vim",
            "symlink",
            "gvimrc",
            &env.dotfiles_root.join("vim/gvimrc"),
            &env.home.join(".gvimrc"),
        );
    }

    // ── Auto-chmod +x for path handler ─────────────────────────
}
