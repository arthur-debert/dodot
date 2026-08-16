//! `dodot install` — wiring dodot into shell startup
//! (`docs/proposals/shipped/shell-hookup.lex` §4).
//!
//! Everything dodot verified before this epic lived in the deployment
//! layer; the hookup — the one line in an rc file that makes any of it
//! reach a terminal — was wired by hand, outside dodot's machinery.
//! This command closes that: it names the shell, names the file, shows
//! the line, and with `--write` puts it there and then *measures*
//! whether a new shell picks it up.
//!
//! # Dry by default
//!
//! The bare invocation is strictly read-only: no rc file is touched
//! and no shell is spawned. dodot never modifies shell startup unasked
//! (spec §9), and the dry run is what lets the user see the decision
//! before delegating it — the failure mode conda's managed block is
//! remembered for.
//!
//! # Write, then verify
//!
//! `--write` splices dodot's marked block into the resolved rc file
//! ([`crate::shell::rc`]) and re-runs the probe against the just-written
//! state, so the command ends on a measured verdict rather than a
//! promise. The block holds a guarded *direct source* of the generated
//! script — not `eval "$(dodot init-sh)"` — because sourcing a file
//! costs no binary invocation per shell start and cannot deadlock on a
//! `dodot` that is not on `PATH` until dodot's own init puts it there
//! (spec §4.2). Hand-wired `eval` hookups keep working untouched; the
//! evidence rides inside the emitted script either way.

use std::path::PathBuf;

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::shell::activation::{self, ActivationNotice};
use crate::shell::probe;
use crate::shell::rc::{self, BlockOutcome, HookupShell, RcTarget};
use crate::{DodotError, Result};

/// What the caller asked for.
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Write the marked block (and then verify). Without it, the
    /// command only reports.
    pub write: bool,
    /// `--rc <file>`: override the resolution ladder entirely.
    pub rc: Option<PathBuf>,
}

/// Everything `dodot install` reports, dry or written.
#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    /// The detected shell (`"zsh"`), or `None` when only `--rc` made
    /// this run possible.
    pub shell: Option<String>,
    /// The rc file, `~/`-relative for display.
    pub rc_path: String,
    /// Present when the rc file is a symlink: the path the ladder
    /// picked, before resolution. Written *through*, never replaced.
    pub rc_link_source: Option<String>,
    /// Whether that file exists now — after this run, so a `--write`
    /// that created it reports `true`. A dry run's `false` is the
    /// "`--write` would create it" case.
    pub rc_exists: bool,
    /// How the hook is wired in that file now, after this run:
    /// `managed-block`, `manual`, or `absent`. What *changed* is
    /// [`outcome`](Self::outcome).
    pub hook_presence: String,
    /// The exact line dodot writes (and that a user can paste).
    pub hook_line: String,
    /// True when this run wrote to disk.
    pub wrote: bool,
    /// What the write did — `None` on a dry run.
    pub outcome: Option<String>,
    /// What was done or noticed along the way: ZDOTDIR resolution, the
    /// symlink write-through, the macOS `.bash_profile` chain.
    pub notes: Vec<String>,
    /// The activation verdict — measured after `--write`, read from
    /// evidence alone on a dry run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_hookup: Option<ActivationNotice>,
}

/// Run `dodot install`.
///
/// Refuses rather than guesses when `$SHELL` names a shell with no
/// canonical rc file in the ladder (spec §4.3 rung 1): the error
/// carries the hook line and points at `--rc`, which is strictly more
/// useful than writing a bashrc snippet for a fish user.
pub fn install(ctx: &ExecutionContext, opts: &InstallOptions) -> Result<InstallResult> {
    let home = ctx.paths.home_dir();
    let init_script = ctx.paths.init_script_path();
    let hook_line = activation::hook_line(&init_script, home);
    let shell = ctx.shell_env.hookup_shell();

    if shell.is_none() && opts.rc.is_none() {
        return Err(DodotError::Other(refusal(&ctx.shell_env.shell, &hook_line)));
    }

    let target = rc::resolve_rc(
        ctx.fs.as_ref(),
        home,
        shell,
        &ctx.shell_env,
        opts.rc.as_deref(),
    );
    let mut notes = target.notes.clone();

    let outcome = if opts.write {
        Some(write_hookup(ctx, shell, &target, &hook_line, &mut notes)?)
    } else {
        None
    };
    // Read the file's state *after* any write, so every field
    // describes the world as it stands now and `outcome` alone
    // carries what changed. Reporting the pre-write state next to
    // "created it" would have the report contradict itself.
    let presence = rc::scan_hook_file(ctx.fs.as_ref(), &target.path);
    let exists = target.exists || opts.write;

    Ok(InstallResult {
        shell: shell.map(|s| s.as_str().to_string()),
        rc_path: rc::display_home_relative(&target.path, home),
        rc_link_source: target
            .link_source
            .as_deref()
            .map(|p| rc::display_home_relative(p, home)),
        rc_exists: exists,
        hook_presence: presence.as_str().to_string(),
        hook_line,
        wrote: opts.write,
        outcome: outcome.map(|o| o.as_str().to_string()),
        notes,
        shell_hookup: verdict(ctx, opts, &target),
    })
}

/// Write the marked block, plus — only when the ladder itself chose
/// `~/.bashrc` — the macOS bash chain, which is what stands between
/// that file and a login shell reading it.
fn write_hookup(
    ctx: &ExecutionContext,
    shell: Option<HookupShell>,
    target: &RcTarget,
    hook_line: &str,
    notes: &mut Vec<String>,
) -> Result<BlockOutcome> {
    let home = ctx.paths.home_dir();
    let outcome = rc::write_block(
        ctx.fs.as_ref(),
        &target.path,
        &rc::render_block(&[hook_line]),
    )?;
    if outcome == BlockOutcome::Created {
        notes.push(format!(
            "created {}",
            rc::display_home_relative(&target.path, home)
        ));
    }

    // macOS Terminal opens *login* shells, which read `.bash_profile`
    // and never `.bashrc` — where the block just landed. Without the
    // chain the hook is written and never read, which is the exact
    // silent failure this epic exists to kill.
    //
    // Only when the block landed in `~/.bashrc` itself: the chain is
    // rung 2 of the ladder, and `--rc` overrides the whole ladder
    // (spec §4.3 rung 5). Chaining for an override would be wrong at
    // both ends — pointless for a file login shells already read, and
    // destructive when the override *is* `~/.bash_profile`, where the
    // chain block's identical markers would replace the hook block
    // written a few lines above. The nominal path is what matters: a
    // symlinked `~/.bashrc` is still the file bash opens.
    if shell == Some(HookupShell::Bash)
        && ctx.host_facts.os == "darwin"
        && target.nominal() == home.join(".bashrc")
    {
        // Never the file we just wrote, for the same marker-collision
        // reason — a `~/.bashrc` symlinked at a profile is pathological
        // but must not cost the user their hook.
        if let Some(profile) =
            rc::bash_chain_target(ctx.fs.as_ref(), home).filter(|p| p != &target.path)
        {
            rc::write_block(
                ctx.fs.as_ref(),
                &profile,
                &rc::render_block(&[rc::BASH_CHAIN_LINE]),
            )?;
            notes.push(format!(
                "macOS opens login shells: chained {} to ~/.bashrc",
                rc::display_home_relative(&profile, home)
            ));
        }
    }
    Ok(outcome)
}

/// The activation verdict to report.
///
/// After `--write` this is a measurement: the probe runs unconditionally
/// against the state we just wrote, because "I wrote it" is the moment a
/// measured answer is worth a shell spawn (spec §4.2) — the signal gate
/// governs `up`, not this. A dry run measures nothing and reports the
/// evidence it can read for free.
fn verdict(
    ctx: &ExecutionContext,
    opts: &InstallOptions,
    target: &RcTarget,
) -> Option<ActivationNotice> {
    let reference = activation::read_script_generation(ctx.fs.as_ref(), ctx.paths.as_ref());
    // Nothing to activate without a script: the hook is wired, and the
    // user's next `dodot up` is what gives it something to source.
    let timeout = ctx
        .shell_probe
        .timeout()
        .filter(|_| opts.write)
        .filter(|_| ctx.fs.exists(&ctx.paths.init_script_path()));
    // The session signal is dropped only where a spawn replaces it —
    // the same rule `notice_with_probe` applies, decided from what
    // will actually happen rather than from the policy. A dry run
    // measures nothing, so a tty-attached session with no stamp is
    // still direct evidence that *this* shell did not load dodot
    // (#279), and saying otherwise would put `install` at odds with a
    // `status` run in the same terminal.
    let evidence = activation::notice_for(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        ctx.env_stamp.clone(),
        reference,
        ctx.tty && timeout.is_none(),
        &ctx.shell_env,
    );
    let Some(timeout) = timeout else {
        return evidence;
    };
    // The diagnosis talks about the file we just wrote — including a
    // `--rc` override, which the ladder would not have found itself.
    probe::measure(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        timeout,
        &ctx.shell_env,
        Some(&target.path),
        reference,
        evidence,
    )
}

/// The honest refusal for a shell dodot has no rc guess for.
fn refusal(shell: &Option<String>, hook_line: &str) -> String {
    let named = match shell.as_deref() {
        Some(s) if !s.is_empty() => format!("your shell ({s})"),
        _ => "your shell ($SHELL is not set)".to_string(),
    };
    format!(
        "dodot can wire up bash and zsh; it will not guess an rc file for {named}. \
         Add this line to whatever file your shell reads at startup: {hook_line} \
         — or point dodot at it with `dodot install --write --rc <file>`."
    )
}
