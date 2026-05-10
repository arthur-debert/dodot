//! Git pre-commit hook installer.
//!
//! `dodot transform install-hook` writes a guarded managed block into
//! `<dotfiles_root>/.git/hooks/pre-commit` that invokes
//! `dodot refresh --quiet && dodot transform check --strict` on every
//! commit. The block is detected by sentinel guard comments, so reruns
//! are idempotent and existing hook content is preserved.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::Result;

/// The guard line that opens our managed block in `.git/hooks/pre-commit`.
/// Detection of this string is what makes [`install_hook`] idempotent.
pub(crate) const HOOK_GUARD_START: &str =
    "# >>> dodot transform check --strict (managed by `dodot transform install-hook`) >>>";

/// The guard line that closes our managed block. Paired with
/// [`HOOK_GUARD_START`].
pub(crate) const HOOK_GUARD_END: &str = "# <<< dodot transform check --strict <<<";

/// Outcome of `dodot transform install-hook`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallHookOutcome {
    /// Hook file did not exist; we created it with shebang + our block.
    Created,
    /// Hook file existed; we appended our block to it. Existing content
    /// is preserved.
    Appended,
    /// Hook was already installed and matches the current managed
    /// block exactly — no change.
    AlreadyInstalled,
    /// Hook was installed but the managed block was an older version
    /// (e.g. didn't yet call `dodot refresh`). We replaced the
    /// outdated block in place. Existing non-managed content in the
    /// hook file is preserved.
    Updated,
}

/// Result returned by [`install_hook`]. Renders through the
/// `transform-install-hook.jinja` template; CLI exits 0 in all three
/// outcomes (every state is a success).
#[derive(Debug, Clone, Serialize)]
pub struct InstallHookResult {
    pub outcome: InstallHookOutcome,
    /// Absolute path of the hook file that was written or inspected.
    pub hook_path: String,
    /// Path of the hook rendered relative to `$HOME` for display.
    pub hook_display_path: String,
    /// The exact line the hook will execute on each commit. Surfaced
    /// so the user can see what `--strict` looks like in their hook.
    pub command_line: String,
}

/// Install (or detect-already-installed) the dodot pre-commit hook
/// that runs `dodot transform check --strict`.
///
/// # Behavior
///
/// - If `<dotfiles_root>/.git/hooks/pre-commit` does not exist:
///   create it with `#!/bin/sh` + our guarded block, mode `0o755`.
/// - If it exists and already contains [`HOOK_GUARD_START`]:
///   no-op, return [`InstallHookOutcome::AlreadyInstalled`].
/// - If it exists without our guard: append our block (preserving
///   existing content), ensure executable bit is set.
///
/// # Errors
///
/// Returns an error if `<dotfiles_root>/.git` doesn't exist (the
/// dotfiles repo isn't a git working tree). The hook only makes
/// sense in a git context.
pub fn install_hook(ctx: &ExecutionContext) -> Result<InstallHookResult> {
    let dotfiles_root = ctx.paths.dotfiles_root();
    let git_dir = dotfiles_root.join(".git");
    if !ctx.fs.is_dir(&git_dir) {
        return Err(crate::DodotError::Other(format!(
            "no .git directory at {}; pre-commit hooks only apply to git working \
             trees. Run `git init` in {} first.",
            git_dir.display(),
            dotfiles_root.display(),
        )));
    }

    let hooks_dir = git_dir.join("hooks");
    let hook_path = hooks_dir.join("pre-commit");

    let block = managed_block();

    let outcome = if ctx.fs.exists(&hook_path) {
        let existing = ctx.fs.read_to_string(&hook_path)?;
        if let Some((start_byte, end_byte)) = find_managed_block(&existing) {
            // A managed block exists. Decide whether it matches the
            // current `block` exactly (no-op) or is stale and needs
            // replacing.
            let current_block = &existing[start_byte..end_byte];
            if current_block == block {
                InstallHookOutcome::AlreadyInstalled
            } else {
                // Stale block — rewrite it in place. Anything outside
                // the marker pair is preserved.
                let mut new_content = String::with_capacity(existing.len() + block.len());
                new_content.push_str(&existing[..start_byte]);
                new_content.push_str(&block);
                new_content.push_str(&existing[end_byte..]);
                ctx.fs.write_file(&hook_path, new_content.as_bytes())?;
                ctx.fs.set_permissions(&hook_path, 0o755)?;
                InstallHookOutcome::Updated
            }
        } else {
            // No managed block at all — append. Preserves existing
            // hook content (user-written or installed by another tool).
            let mut new_content = existing.clone();
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            if !new_content.ends_with("\n\n") {
                new_content.push('\n');
            }
            new_content.push_str(&block);
            ctx.fs.write_file(&hook_path, new_content.as_bytes())?;
            ctx.fs.set_permissions(&hook_path, 0o755)?;
            InstallHookOutcome::Appended
        }
    } else {
        ctx.fs.mkdir_all(&hooks_dir)?;
        let mut new_content = String::from("#!/bin/sh\n\n");
        new_content.push_str(&block);
        ctx.fs.write_file(&hook_path, new_content.as_bytes())?;
        ctx.fs.set_permissions(&hook_path, 0o755)?;
        InstallHookOutcome::Created
    };

    Ok(InstallHookResult {
        outcome,
        hook_path: hook_path.display().to_string(),
        hook_display_path: super::render_path(&hook_path, ctx.paths.home_dir()),
        command_line: HOOK_COMMAND.to_string(),
    })
}

/// Detect whether the hook is currently installed in the dotfiles
/// repo. Used by the `dodot up` first-template-deploy prompt to
/// decide whether to offer installation. Cheap (single read of the
/// hook file).
pub fn hook_is_installed(ctx: &ExecutionContext) -> Result<bool> {
    let hook_path = ctx.paths.dotfiles_root().join(".git/hooks/pre-commit");
    if !ctx.fs.exists(&hook_path) {
        return Ok(false);
    }
    let existing = ctx.fs.read_to_string(&hook_path)?;
    Ok(existing.contains(HOOK_GUARD_START))
}

/// Public for `dodot transform show-hook` (future) and for the
/// onboarding prompt in `commands::up` to surface what would be
/// installed. Includes the guard lines so callers can grep-detect
/// the block in arbitrary contexts.
///
/// The block runs two commands:
///
/// 1. `dodot refresh --quiet` — touch source mtimes for any
///    deployed-side edits so git's stat-cache invalidates. Without
///    this, the clean filter (R6) wouldn't fire on the upcoming
///    commit, and the commit could include stale template content.
/// 2. `dodot transform check --strict` — run the 4-state matrix and
///    refuse the commit on any finding (Conflict, missing,
///    unresolved markers, NeedsRebaseline). `Patched` outcomes don't
///    refuse — burgertocow's auto-merge already produced a clean
///    unified patch and rewrote the source; the user `git add`s and
///    commits the follow-up if they want a clean history.
///
/// Each step short-circuits with `|| exit 1`; a failure in either
/// aborts the commit (with exit code 1 — the inner command's exit
/// status is intentionally not preserved, since for git's purposes
/// "any non-zero" is what blocks the commit).
pub fn managed_block() -> String {
    format!(
        "{guard_start}\n\
         # Aborts the commit if any template-source has drift that needs review —\n\
         # divergent deployed file or unresolved dodot-conflict markers. Remove\n\
         # this block to opt out.\n\
         {refresh}\n\
         {check}\n\
         {guard_end}\n",
        guard_start = HOOK_GUARD_START,
        guard_end = HOOK_GUARD_END,
        refresh = HOOK_COMMAND_REFRESH,
        check = HOOK_COMMAND_CHECK,
    )
}

/// First shell line of the managed block: invalidate git's
/// stat-cache for any deployed-side edits. `--quiet` so a no-op
/// refresh doesn't print on every commit.
pub(crate) const HOOK_COMMAND_REFRESH: &str = "dodot refresh --quiet || exit 1";

/// Second shell line: run the strict check. Splits across two lines
/// in the hook so each step can be diagnosed independently.
pub(crate) const HOOK_COMMAND_CHECK: &str = "dodot transform check --strict || exit 1";

/// Combined "what the hook runs" string for display purposes
/// (shown by the install message + the post-up prompt). The actual
/// hook file uses the two-line form from [`managed_block`].
pub(crate) const HOOK_COMMAND: &str = "dodot refresh --quiet && dodot transform check --strict";

/// Locate the byte range of our managed block inside `text` —
/// from the first character of `HOOK_GUARD_START` through the
/// trailing newline after `HOOK_GUARD_END`. Returns `None` if either
/// guard is missing or if the end guard doesn't appear after the
/// start guard.
///
/// Used by the install path to detect stale managed blocks (and
/// rewrite them to the current shape) without disturbing any
/// non-managed content the user has in their hook.
fn find_managed_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(HOOK_GUARD_START)?;
    // Find the end guard after `start`.
    let after_start = start + HOOK_GUARD_START.len();
    let end_rel = text[after_start..].find(HOOK_GUARD_END)?;
    let end_guard_start = after_start + end_rel;
    let end_byte = end_guard_start + HOOK_GUARD_END.len();
    // Include the trailing newline (if any) so re-inserting the new
    // block doesn't double-up the line break.
    let end_byte = if text.as_bytes().get(end_byte) == Some(&b'\n') {
        end_byte + 1
    } else {
        end_byte
    };
    Some((start, end_byte))
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]

    use super::super::test_support::make_ctx;
    use super::*;
    use crate::fs::Fs;
    use crate::testing::TempEnvironment;

    // ── install_hook ────────────────────────────────────────────

    /// Stand up a fake `.git` directory inside the dotfiles_root so
    /// `install_hook` recognises the dotfiles repo as a git working
    /// tree. We don't `git init` for real because every test would
    /// pay the subprocess cost; the installer only checks for
    /// `.git` as a dir, so a bare `mkdir` suffices.
    fn fake_git_dir(env: &TempEnvironment) {
        env.fs
            .mkdir_all(&env.dotfiles_root.join(".git/hooks"))
            .unwrap();
    }

    #[test]
    fn install_hook_creates_new_pre_commit_when_absent() {
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        // Make sure the hooks dir exists but the hook file does not.
        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        assert!(!env.fs.exists(&hook_path));

        let ctx = make_ctx(&env);
        let result = install_hook(&ctx).unwrap();
        assert!(matches!(result.outcome, InstallHookOutcome::Created));
        assert!(env.fs.exists(&hook_path));

        let body = env.fs.read_to_string(&hook_path).unwrap();
        assert!(body.starts_with("#!/bin/sh\n"), "body: {body:?}");
        assert!(body.contains(HOOK_GUARD_START), "body: {body:?}");
        assert!(body.contains(HOOK_COMMAND_REFRESH), "body: {body:?}");
        assert!(body.contains(HOOK_COMMAND_CHECK), "body: {body:?}");
        assert!(body.contains(HOOK_GUARD_END), "body: {body:?}");
    }

    #[test]
    fn install_hook_appends_to_existing_pre_commit() {
        // The user already has a hook (e.g. installed by another tool
        // or a personal script). install_hook must preserve that
        // content and append our block, not clobber it.
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        let existing = "#!/bin/sh\necho 'my pre-commit'\nexit 0\n";
        env.fs.write_file(&hook_path, existing.as_bytes()).unwrap();

        let ctx = make_ctx(&env);
        let result = install_hook(&ctx).unwrap();
        assert!(matches!(result.outcome, InstallHookOutcome::Appended));

        let body = env.fs.read_to_string(&hook_path).unwrap();
        assert!(body.starts_with(existing), "user content lost: {body:?}");
        assert!(body.contains(HOOK_GUARD_START));
        assert!(body.contains(HOOK_COMMAND_REFRESH));
        assert!(body.contains(HOOK_COMMAND_CHECK));
    }

    #[test]
    fn install_hook_is_idempotent_on_second_call() {
        // Running `dodot transform install-hook` twice in a row must
        // not double-append the block. The guard line is what makes
        // this safe.
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let ctx = make_ctx(&env);

        let r1 = install_hook(&ctx).unwrap();
        assert!(matches!(r1.outcome, InstallHookOutcome::Created));

        let body_after_first = env
            .fs
            .read_to_string(&env.dotfiles_root.join(".git/hooks/pre-commit"))
            .unwrap();

        let r2 = install_hook(&ctx).unwrap();
        assert!(matches!(r2.outcome, InstallHookOutcome::AlreadyInstalled));

        let body_after_second = env
            .fs
            .read_to_string(&env.dotfiles_root.join(".git/hooks/pre-commit"))
            .unwrap();
        assert_eq!(
            body_after_first, body_after_second,
            "body changed on second call"
        );
        // Exactly one occurrence of the guard line.
        assert_eq!(body_after_second.matches(HOOK_GUARD_START).count(), 1);
    }

    #[test]
    fn install_hook_errors_if_no_git_dir() {
        // If the dotfiles root isn't a git working tree, refuse
        // with a clear error rather than silently writing a hook
        // that nothing will ever invoke.
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let err = install_hook(&ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no .git directory"), "msg: {msg}");
        assert!(msg.contains("git init"), "msg: {msg}");
    }

    #[test]
    fn hook_is_installed_reports_correctly() {
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let ctx = make_ctx(&env);

        // No hook yet → not installed.
        assert!(!hook_is_installed(&ctx).unwrap());

        // Install it → reported as installed.
        install_hook(&ctx).unwrap();
        assert!(hook_is_installed(&ctx).unwrap());

        // A user-written hook without our guard → not installed
        // (from our perspective).
        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        env.fs
            .write_file(&hook_path, b"#!/bin/sh\necho hello\n")
            .unwrap();
        assert!(!hook_is_installed(&ctx).unwrap());
    }

    #[test]
    fn install_hook_sets_executable_bit() {
        // The hook needs +x to be invoked by git. Confirm we set
        // the bit on both the create and the append paths.
        use std::os::unix::fs::PermissionsExt;

        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let ctx = make_ctx(&env);
        install_hook(&ctx).unwrap();

        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        let mode = std::fs::metadata(&hook_path).unwrap().permissions().mode();
        // owner-execute bit must be set; we test for any execute
        // rather than exact 0o755 because the OS may apply umask.
        assert!(
            mode & 0o100 != 0,
            "hook is not executable, mode = {:o}",
            mode
        );
    }

    #[test]
    fn managed_block_is_self_contained_and_grep_detectable() {
        // The block alone should be enough to detect the install:
        // its first line is exactly HOOK_GUARD_START. This pins the
        // contract that downstream tools (or future `transform
        // uninstall-hook`) can grep for the guard line.
        let block = managed_block();
        assert!(block.starts_with(HOOK_GUARD_START));
        assert!(block.trim_end().ends_with(HOOK_GUARD_END));
        // Both shell lines (refresh + check) must appear in the
        // block — the hook runs them as two independent steps.
        assert!(block.contains(HOOK_COMMAND_REFRESH));
        assert!(block.contains(HOOK_COMMAND_CHECK));
    }

    // ── hook upgrade (managed-block detection + replacement) ────

    #[test]
    fn install_hook_replaces_a_stale_managed_block() {
        // An older R4-shape block (single check command, no refresh
        // line) must be detected and rewritten to the new two-line
        // form when `install-hook` runs again. Existing non-managed
        // content is preserved.
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);

        // Stage an old-style block manually. This is what an R4-era
        // install-hook would have produced: the same guards, but the
        // single old `dodot transform check --strict || exit 1`
        // command line and the older comment.
        let stale = format!(
            "#!/bin/sh\n\
         echo 'user-installed pre-commit step'\n\
         \n\
         {start}\n\
         # Old-style block from R4. Still works, but doesn't run\n\
         # `dodot refresh` first, so deployed-side edits between\n\
         # commits aren't always picked up.\n\
         dodot transform check --strict || exit 1\n\
         {end}\n\
         # User content after the block.\n\
         echo 'trailing user step'\n",
            start = HOOK_GUARD_START,
            end = HOOK_GUARD_END,
        );
        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        env.fs.write_file(&hook_path, stale.as_bytes()).unwrap();

        let ctx = make_ctx(&env);
        let result = install_hook(&ctx).unwrap();
        assert!(matches!(result.outcome, InstallHookOutcome::Updated));

        let body = env.fs.read_to_string(&hook_path).unwrap();
        // New shape: both refresh + check lines, comment matches the
        // current block.
        assert!(body.contains(HOOK_COMMAND_REFRESH), "body: {body:?}");
        assert!(body.contains(HOOK_COMMAND_CHECK), "body: {body:?}");
        // User content (before AND after the managed block) survived.
        assert!(body.contains("user-installed pre-commit step"));
        assert!(body.contains("trailing user step"));
        // Exactly one managed block — no duplicates.
        assert_eq!(body.matches(HOOK_GUARD_START).count(), 1);
        assert_eq!(body.matches(HOOK_GUARD_END).count(), 1);
    }

    #[test]
    fn install_hook_no_op_on_current_block() {
        // The exact opposite of the upgrade test: if the existing
        // block is already the current shape, install_hook returns
        // AlreadyInstalled and leaves the file byte-identical.
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let ctx = make_ctx(&env);

        // Install fresh.
        let r1 = install_hook(&ctx).unwrap();
        assert!(matches!(r1.outcome, InstallHookOutcome::Created));
        let body_after_first = env
            .fs
            .read_to_string(&env.dotfiles_root.join(".git/hooks/pre-commit"))
            .unwrap();

        // Re-install — current block is up to date, no change.
        let r2 = install_hook(&ctx).unwrap();
        assert!(matches!(r2.outcome, InstallHookOutcome::AlreadyInstalled));
        let body_after_second = env
            .fs
            .read_to_string(&env.dotfiles_root.join(".git/hooks/pre-commit"))
            .unwrap();
        assert_eq!(body_after_first, body_after_second);
    }

    #[test]
    fn find_managed_block_locates_byte_range() {
        // White-box test for the byte-range finder so we don't have
        // to reverse-engineer it from the splice tests above.
        let block = managed_block();
        let prefix = "before\n";
        let suffix = "after\n";
        let text = format!("{prefix}{block}{suffix}");
        let (start, end) = find_managed_block(&text).expect("must find block");
        assert_eq!(&text[start..end], block);
    }

    #[test]
    fn find_managed_block_returns_none_when_absent() {
        assert!(find_managed_block("nothing here").is_none());
        // Half-block (start without end) → also None: we treat
        // partial blocks as "not installed" so install_hook will
        // append rather than try to splice.
        let only_start = format!("{HOOK_GUARD_START}\nrandom content\n");
        assert!(find_managed_block(&only_start).is_none());
    }
}
