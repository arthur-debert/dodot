//! Interactive tutorial driver (`dodot tutorial`).
//!
//! Walks a new user through their first pack deployment using their
//! actual dotfiles repo. The flow is a hand-rolled state machine over
//! named steps, each step rendering a templated body (via
//! `standout_render`) and asking one question through the [`Prompts`]
//! trait.
//!
//! `Prompts` is the test seam: production runs use [`InquirePrompts`]
//! (real TUI prompts via the `inquire` crate); tests inject
//! [`ScriptedPrompts`] with a queue of canned answers, exercising the
//! whole driver in-process with no TTY.

#[cfg(test)]
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use standout::OutputMode;

use dodot_lib::commands::tutorial::{self as lib_tut, TutorialCtx, TutorialState};
use dodot_lib::commands::{self, GroupMode, ViewMode};
use dodot_lib::config::ConfigManager;
use dodot_lib::datastore::DataStore;
use dodot_lib::fs::Fs;
use dodot_lib::packs::orchestration::ExecutionContext;
use dodot_lib::paths::{Pather, XdgPather};
use dodot_lib::render;
use dodot_lib::shell::SyntaxChecker;

// ── Prompt seam ─────────────────────────────────────────────────

/// Prompt I/O abstraction. The driver only knows about this trait —
/// the production path uses [`InquirePrompts`], tests use
/// [`ScriptedPrompts`].
pub trait Prompts {
    /// Yes/no prompt. `default` is the highlighted choice; `Cancel` /
    /// Ctrl-C surfaces as `Err`.
    fn confirm(&self, message: &str, default: bool) -> Result<bool>;

    /// Single-choice select. Returns the chosen label.
    fn select(&self, message: &str, options: Vec<String>) -> Result<String>;

    /// "Press enter to continue" — non-cancelling, no value.
    fn press_enter(&self) -> Result<()>;
}

/// Real interactive prompts (production path).
pub struct InquirePrompts;

/// Build the inquire `RenderConfig` for tutorial prompts.
///
/// The prompt question line is rendered in italic so it visually
/// stands apart from the (heavier-weight) step body above it. This
/// mirrors the `tutorial-prompt` style in dodot's theme YAML — the
/// theme entry is the documented knob; this function applies it to
/// inquire (which doesn't read the dodot theme directly). Keep both
/// in sync.
fn tutorial_render_config() -> inquire::ui::RenderConfig<'static> {
    use inquire::ui::{Attributes, RenderConfig, StyleSheet};

    let mut cfg = RenderConfig::default_colored();
    cfg.prompt = StyleSheet::new().with_attr(Attributes::ITALIC);
    cfg.help_message = StyleSheet::new().with_attr(Attributes::ITALIC);
    cfg
}

/// Print a blank line before every prompt so the question is visually
/// separated from the step body that just rendered.
fn pad_before_prompt() {
    println!();
}

impl Prompts for InquirePrompts {
    fn confirm(&self, message: &str, default: bool) -> Result<bool> {
        pad_before_prompt();
        match inquire::Confirm::new(message)
            .with_default(default)
            .with_render_config(tutorial_render_config())
            .prompt()
        {
            Ok(b) => Ok(b),
            Err(inquire::InquireError::OperationCanceled)
            | Err(inquire::InquireError::OperationInterrupted) => Err(anyhow!("cancelled")),
            Err(e) => Err(anyhow!("{e}")),
        }
    }

    fn select(&self, message: &str, options: Vec<String>) -> Result<String> {
        pad_before_prompt();
        match inquire::Select::new(message, options)
            .with_render_config(tutorial_render_config())
            .prompt()
        {
            Ok(s) => Ok(s),
            Err(inquire::InquireError::OperationCanceled)
            | Err(inquire::InquireError::OperationInterrupted) => Err(anyhow!("cancelled")),
            Err(e) => Err(anyhow!("{e}")),
        }
    }

    fn press_enter(&self) -> Result<()> {
        pad_before_prompt();
        // Inquire's Text returns an error on empty submit by default;
        // with_default("") makes empty acceptable so just hitting
        // enter advances. Ctrl-C / Esc still surface as a cancel so
        // the driver can exit cleanly — the intro promises Ctrl-C
        // works at *any* prompt, including these.
        match inquire::Text::new("(press enter to continue)")
            .with_default("")
            .with_render_config(tutorial_render_config())
            .prompt()
        {
            Ok(_) => Ok(()),
            Err(inquire::InquireError::OperationCanceled)
            | Err(inquire::InquireError::OperationInterrupted) => Err(anyhow!("cancelled")),
            Err(e) => Err(anyhow!("{e}")),
        }
    }
}

/// Scripted prompts for tests. Pop one queued response per call and
/// validate kind so wizard-reorder bugs surface at the offending step
/// rather than as silent wrong-data assertions later.
#[cfg(test)]
pub enum ScriptedAnswer {
    Confirm(bool),
    /// 0-based index into the options vec passed to `select`.
    Choice(usize),
    /// Equivalent of pressing enter.
    Enter,
}

#[cfg(test)]
pub struct ScriptedPrompts {
    queue: Mutex<VecDeque<ScriptedAnswer>>,
}

#[cfg(test)]
impl ScriptedPrompts {
    pub fn new(answers: impl IntoIterator<Item = ScriptedAnswer>) -> Self {
        Self {
            queue: Mutex::new(answers.into_iter().collect()),
        }
    }

    pub fn remaining(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    fn next(&self, expected: &str, msg: &str) -> Result<ScriptedAnswer> {
        self.queue.lock().unwrap().pop_front().ok_or_else(|| {
            anyhow!("ScriptedPrompts: ran out of answers; next was a `{expected}` prompt: {msg:?}")
        })
    }
}

#[cfg(test)]
impl Prompts for ScriptedPrompts {
    fn confirm(&self, message: &str, _default: bool) -> Result<bool> {
        match self.next("confirm", message)? {
            ScriptedAnswer::Confirm(b) => Ok(b),
            other => Err(anyhow!(
                "ScriptedPrompts: expected Confirm for {message:?}, got {other:?}"
            )),
        }
    }

    fn select(&self, message: &str, options: Vec<String>) -> Result<String> {
        match self.next("select", message)? {
            ScriptedAnswer::Choice(i) => options.get(i).cloned().ok_or_else(|| {
                anyhow!(
                    "ScriptedPrompts: Choice({i}) out of range for select with {} option(s) ({message:?})",
                    options.len()
                )
            }),
            other => Err(anyhow!(
                "ScriptedPrompts: expected Choice for {message:?}, got {other:?}"
            )),
        }
    }

    fn press_enter(&self) -> Result<()> {
        match self.next("enter", "press enter")? {
            ScriptedAnswer::Enter => Ok(()),
            other => Err(anyhow!("ScriptedPrompts: expected Enter, got {other:?}")),
        }
    }
}

#[cfg(test)]
impl std::fmt::Debug for ScriptedAnswer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptedAnswer::Confirm(b) => write!(f, "Confirm({b})"),
            ScriptedAnswer::Choice(i) => write!(f, "Choice({i})"),
            ScriptedAnswer::Enter => write!(f, "Enter"),
        }
    }
}

// ── Environment ────────────────────────────────────────────────

/// Filesystem + paths + datastore the tutorial needs. In production
/// built by [`TutorialEnv::from_process_env`]; tests construct one
/// directly from a `TempEnvironment` so the whole flow runs against
/// a temp dir.
pub struct TutorialEnv {
    pub fs: Arc<dyn Fs>,
    pub paths: Arc<dyn Pather>,
    pub datastore: Arc<dyn DataStore>,
    pub command_runner: Arc<dyn dodot_lib::datastore::CommandRunner>,
    pub config_manager: Arc<ConfigManager>,
    pub syntax_checker: Arc<dyn SyntaxChecker>,
    /// Where the dotfiles root came from (for the check_root step
    /// template) — `"DOTFILES_ROOT env var"` / `"git toplevel"` / etc.
    pub root_origin: String,
}

impl TutorialEnv {
    /// Build from real process environment — same wiring as
    /// `ExecutionContext::production`, but the parts are kept so we
    /// can derive multiple ExecutionContexts with different
    /// `dry_run` flags without rebuilding everything.
    pub fn from_process_env() -> Result<Self> {
        let (root, origin) = discover_root_with_origin();
        let paths = Arc::new(
            XdgPather::builder()
                .dotfiles_root(&root)
                .build()
                .map_err(|e| anyhow!("paths: {e}"))?,
        );
        let fs: Arc<dyn Fs> = Arc::new(dodot_lib::fs::OsFs::new());
        let runner: Arc<dyn dodot_lib::datastore::CommandRunner> =
            Arc::new(dodot_lib::datastore::ShellCommandRunner::new(false));
        let datastore: Arc<dyn DataStore> =
            Arc::new(dodot_lib::datastore::FilesystemDataStore::new(
                fs.clone(),
                paths.clone(),
                runner.clone(),
            ));
        let config_manager =
            Arc::new(ConfigManager::new(&root).map_err(|e| anyhow!("config: {e}"))?);
        Ok(Self {
            fs,
            paths,
            datastore,
            command_runner: runner,
            config_manager,
            syntax_checker: Arc::new(dodot_lib::shell::SystemSyntaxChecker),
            root_origin: origin,
        })
    }

    fn make_ctx(&self, dry_run: bool) -> ExecutionContext {
        ExecutionContext {
            fs: self.fs.clone(),
            datastore: self.datastore.clone(),
            paths: self.paths.clone(),
            config_manager: self.config_manager.clone(),
            syntax_checker: self.syntax_checker.clone(),
            command_runner: self.command_runner.clone(),
            dry_run,
            no_provision: false,
            provision_rerun: false,
            force: false,
            check_drift: false,
            show_diff: false,
            view_mode: ViewMode::Full,
            group_mode: GroupMode::Name,
            verbose: false,
            host_facts: std::sync::Arc::new(dodot_lib::gates::HostFacts::detect()),
        }
    }
}

// ── Options & entry ─────────────────────────────────────────────

pub struct Options {
    pub reset: bool,
    pub from: Option<String>,
    /// Output mode for templates. `OutputMode::Auto` in production,
    /// `OutputMode::Text` in tests.
    pub mode: OutputMode,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            reset: false,
            from: None,
            mode: OutputMode::Auto,
        }
    }
}

/// Production entry point — uses real Inquire prompts and stdout.
pub fn run(opts: Options, out: &mut impl Write) -> Result<()> {
    let env = TutorialEnv::from_process_env()?;
    run_with_prompts(&env, opts, &InquirePrompts, out)
}

/// Test-friendly entry point. Use [`ScriptedPrompts`] to drive a
/// deterministic flow without a TTY, against an env wired to a
/// `TempEnvironment` fixture.
pub fn run_with_prompts(
    env: &TutorialEnv,
    opts: Options,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<()> {
    let xdg = env.paths.clone();

    if opts.reset {
        let _ = lib_tut::clear_state(xdg.as_ref());
    }

    let mut ctx = build_initial_ctx(env)?;

    let mut step: &'static str = "intro";
    if opts.from.is_none() && !opts.reset {
        if let Some(saved) = lib_tut::load_state(xdg.as_ref()) {
            match prompt_resume(prompts, &saved)? {
                ResumeChoice::Resume => {
                    step = step_id_to_static(&saved.step_id).unwrap_or("intro");
                    if let Some(p) = saved.pack {
                        ctx.chosen_pack = Some(p);
                    }
                }
                ResumeChoice::Restart => {
                    let _ = lib_tut::clear_state(xdg.as_ref());
                }
                ResumeChoice::Quit => return Ok(()),
            }
        }
    }
    if let Some(from) = opts.from.as_deref() {
        if let Some(s) = step_id_to_static(from) {
            step = s;
        }
    }

    loop {
        save_step(xdg.as_ref(), step, &ctx);
        match run_step(step, &mut ctx, &opts, env, prompts, out)? {
            Next::Go(next) => step = next,
            Next::Done => {
                let _ = lib_tut::clear_state(xdg.as_ref());
                return Ok(());
            }
            Next::Quit => {
                writeln!(
                    out,
                    "\nExited tutorial. Resume any time with: dodot tutorial"
                )?;
                return Ok(());
            }
        }
    }
}

#[derive(Debug)]
enum Next {
    Go(&'static str),
    Done,
    Quit,
}

#[derive(Debug)]
enum ResumeChoice {
    Resume,
    Restart,
    Quit,
}

fn prompt_resume(prompts: &dyn Prompts, saved: &TutorialState) -> Result<ResumeChoice> {
    let pack_label = saved.pack.as_deref().unwrap_or("(no pack picked yet)");
    let label = format!(
        "Found a saved tutorial at step '{}' (pack: {})",
        saved.step_id, pack_label
    );
    let choice = prompts.select(
        &label,
        vec!["Resume".into(), "Start over".into(), "Quit".into()],
    )?;
    Ok(match choice.as_str() {
        "Resume" => ResumeChoice::Resume,
        "Start over" => ResumeChoice::Restart,
        _ => ResumeChoice::Quit,
    })
}

fn save_step(paths: &dyn Pather, step: &str, ctx: &TutorialCtx) {
    let state = TutorialState {
        step_id: step.to_string(),
        pack: ctx.chosen_pack.clone(),
        started_at: None,
    };
    let _ = lib_tut::save_state(paths, &state);
}

// ── Steps ──────────────────────────────────────────────────────

fn run_step(
    step: &str,
    ctx: &mut TutorialCtx,
    opts: &Options,
    env: &TutorialEnv,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    match step {
        "intro" => step_intro(ctx, opts, prompts, out),
        "check_root" => step_check_root(ctx, opts, prompts, out),
        "list_packs" => step_list_packs(ctx, opts, prompts, out),
        "no_packs" => step_no_packs(ctx, opts, prompts, out),
        "pick_pack" => step_pick_pack(ctx, opts, prompts, out),
        "show_status" => step_show_status(ctx, opts, env, prompts, out),
        "annotate_status" => step_annotate_status(ctx, opts, prompts, out),
        "concept_targets" => step_concept_targets(ctx, opts, prompts, out),
        "concept_shell" => step_concept_shell(ctx, opts, env, prompts, out),
        "dry_run" => step_dry_run(ctx, opts, env, prompts, out),
        "real_up" => step_real_up(ctx, opts, env, prompts, out),
        "outro" => step_outro(ctx, opts, prompts, out),
        other => Err(anyhow!("unknown tutorial step: {other}")),
    }
}

fn step_id_to_static(id: &str) -> Option<&'static str> {
    Some(match id {
        "intro" => "intro",
        "check_root" => "check_root",
        "list_packs" => "list_packs",
        "no_packs" => "no_packs",
        "pick_pack" => "pick_pack",
        "show_status" => "show_status",
        "annotate_status" => "annotate_status",
        "concept_targets" => "concept_targets",
        "concept_shell" => "concept_shell",
        "dry_run" => "dry_run",
        "real_up" => "real_up",
        "outro" => "outro",
        _ => return None,
    })
}

fn step_intro(
    ctx: &mut TutorialCtx,
    opts: &Options,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    print_step("tutorial.intro", ctx, opts, out)?;
    if prompts.confirm("Ready to begin?", true)? {
        Ok(Next::Go("check_root"))
    } else {
        Ok(Next::Quit)
    }
}

fn step_check_root(
    ctx: &mut TutorialCtx,
    opts: &Options,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    print_step("tutorial.check_root", ctx, opts, out)?;
    if prompts.confirm("Is this your dotfiles repo?", true)? {
        Ok(Next::Go("list_packs"))
    } else {
        writeln!(
            out,
            "\nOK — exit, cd into your dotfiles repo, and re-run `dodot tutorial`."
        )?;
        Ok(Next::Quit)
    }
}

fn step_list_packs(
    ctx: &mut TutorialCtx,
    _opts: &Options,
    _prompts: &dyn Prompts,
    _out: &mut impl Write,
) -> Result<Next> {
    if ctx.packs.is_empty() {
        Ok(Next::Go("no_packs"))
    } else {
        Ok(Next::Go("pick_pack"))
    }
}

fn step_no_packs(
    ctx: &mut TutorialCtx,
    opts: &Options,
    _prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    print_step("tutorial.no_packs", ctx, opts, out)?;
    Ok(Next::Done)
}

fn step_pick_pack(
    ctx: &mut TutorialCtx,
    opts: &Options,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    print_step("tutorial.pick_pack", ctx, opts, out)?;

    let mut packs = ctx.packs.clone();
    packs.sort_by_key(|p| !p.recommended);
    let labels: Vec<String> = packs
        .iter()
        .map(|p| {
            if p.recommended {
                format!("{}  ← recommended ({})", p.name, p.kind)
            } else {
                format!("{}  ({})", p.name, p.kind)
            }
        })
        .collect();
    let choice = prompts.select("Pick a pack to start with:", labels)?;

    let chosen = choice.split_whitespace().next().unwrap_or("").to_string();
    let pack = packs
        .into_iter()
        .find(|p| p.name == chosen)
        .ok_or_else(|| anyhow!("internal: chosen pack not in list"))?;
    ctx.chosen_pack = Some(pack.name.clone());
    ctx.chosen_pack_kind = Some(pack.kind.clone());
    ctx.has_shell_files = pack.kind.contains("shell");
    ctx.has_install_files = pack.kind.contains("install");

    Ok(Next::Go("show_status"))
}

fn step_show_status(
    ctx: &mut TutorialCtx,
    opts: &Options,
    env: &TutorialEnv,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    let pack = ctx
        .chosen_pack
        .clone()
        .ok_or_else(|| anyhow!("no pack chosen"))?;
    let status = render_dodot_status(env, &pack, opts.mode)?;
    ctx.status_output = Some(status);
    print_step("tutorial.show_status", ctx, opts, out)?;
    prompts.press_enter()?;
    Ok(Next::Go("annotate_status"))
}

fn step_annotate_status(
    ctx: &mut TutorialCtx,
    opts: &Options,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    print_step("tutorial.annotate_status", ctx, opts, out)?;
    prompts.press_enter()?;
    Ok(Next::Go("concept_targets"))
}

fn step_concept_targets(
    ctx: &mut TutorialCtx,
    opts: &Options,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    print_step("tutorial.concept_targets", ctx, opts, out)?;
    if !prompts.confirm("Targets look right?", true)? {
        writeln!(
            out,
            "\nOK — exit and edit your pack (or its `.dodot.toml`) to set explicit targets, then re-run."
        )?;
        return Ok(Next::Quit);
    }

    // The shell-integration step explains the `eval "$(dodot init-sh)"`
    // line, which is what makes shell snippets get sourced and `bin/`
    // dirs get added to PATH. Install scripts and Brewfile run once
    // and don't need init-sh, so they don't trigger this step.
    if ctx.has_shell_files {
        Ok(Next::Go("concept_shell"))
    } else {
        Ok(Next::Go("dry_run"))
    }
}

fn step_concept_shell(
    ctx: &mut TutorialCtx,
    opts: &Options,
    env: &TutorialEnv,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    let integ = lib_tut::detect_shell_integration(env.paths.home_dir());
    ctx.eval_line = integ.eval_line.clone();
    ctx.shell_integration = Some(integ.clone());

    print_step("tutorial.concept_shell", ctx, opts, out)?;

    if integ.line_present {
        return Ok(Next::Go("dry_run"));
    }

    let action = prompts.select(
        "What do you want me to do?",
        vec![
            "Append it to my rc file".into(),
            "Copy to clipboard".into(),
            "Nothing — I'll handle it".into(),
        ],
    )?;
    match action.as_str() {
        "Append it to my rc file" => {
            lib_tut::append_shell_integration(&integ).map_err(|e| anyhow!("{e}"))?;
            writeln!(
                out,
                "\n  ✓ appended to {} (open a new shell to pick it up)",
                integ.rc_path
            )?;
        }
        "Copy to clipboard" => match copy_to_clipboard(&integ.eval_line) {
            Ok(()) => writeln!(out, "\n  ✓ copied to clipboard")?,
            Err(e) => writeln!(
                out,
                "\n  ! couldn't copy to clipboard ({e}); paste manually:\n  {}",
                integ.eval_line
            )?,
        },
        _ => writeln!(
            out,
            "\n  Add to {} when you're ready:\n  {}",
            integ.rc_path, integ.eval_line
        )?,
    }
    Ok(Next::Go("dry_run"))
}

fn step_dry_run(
    ctx: &mut TutorialCtx,
    opts: &Options,
    env: &TutorialEnv,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    let pack = ctx
        .chosen_pack
        .clone()
        .ok_or_else(|| anyhow!("no pack chosen"))?;
    let preview = render_dodot_up(env, &pack, true, opts.mode)?;
    ctx.dry_run_output = Some(preview);
    print_step("tutorial.dry_run", ctx, opts, out)?;
    if prompts.confirm("Apply for real?", true)? {
        Ok(Next::Go("real_up"))
    } else {
        writeln!(out, "\nNothing was applied. Run again any time.")?;
        Ok(Next::Quit)
    }
}

fn step_real_up(
    ctx: &mut TutorialCtx,
    opts: &Options,
    env: &TutorialEnv,
    prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    let pack = ctx
        .chosen_pack
        .clone()
        .ok_or_else(|| anyhow!("no pack chosen"))?;
    let up_out = render_dodot_up(env, &pack, false, opts.mode)?;
    ctx.up_output = Some(up_out);
    let new_status = render_dodot_status(env, &pack, opts.mode)?;
    ctx.status_output = Some(new_status);
    print_step("tutorial.real_up", ctx, opts, out)?;
    prompts.press_enter()?;
    Ok(Next::Go("outro"))
}

fn step_outro(
    ctx: &mut TutorialCtx,
    opts: &Options,
    _prompts: &dyn Prompts,
    out: &mut impl Write,
) -> Result<Next> {
    print_step("tutorial.outro", ctx, opts, out)?;
    Ok(Next::Done)
}

// ── Helpers ────────────────────────────────────────────────────

fn print_step(
    template: &str,
    ctx: &TutorialCtx,
    opts: &Options,
    out: &mut impl Write,
) -> Result<()> {
    let body =
        render::render_tutorial_step(template, ctx, opts.mode).map_err(|e| anyhow!("{e}"))?;
    writeln!(out, "\n{body}")?;
    Ok(())
}

fn build_initial_ctx(env: &TutorialEnv) -> Result<TutorialCtx> {
    let exec_ctx = env.make_ctx(false);
    let packs = lib_tut::discover_and_classify(&exec_ctx)
        .map_err(|e| anyhow!("pack discovery failed: {e}"))?;

    Ok(TutorialCtx {
        dotfiles_root: env.paths.dotfiles_root().display().to_string(),
        via: env.root_origin.clone(),
        packs,
        ..Default::default()
    })
}

fn discover_root_with_origin() -> (PathBuf, String) {
    if let Ok(s) = std::env::var("DOTFILES_ROOT") {
        let p = PathBuf::from(&s);
        if p.exists() {
            return (p, "DOTFILES_ROOT env var".into());
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return (PathBuf::from(s), "git toplevel".into());
            }
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    (cwd, "current directory".into())
}

fn render_dodot_status(env: &TutorialEnv, pack: &str, mode: OutputMode) -> Result<String> {
    let exec_ctx = env.make_ctx(false);
    let result = commands::status::status(Some(&[pack.to_string()]), &exec_ctx)
        .map_err(|e| anyhow!("status: {e}"))?;
    let s = render::render("pack-status", &result, mode).map_err(|e| anyhow!("render: {e}"))?;
    Ok(indent(&s, "  "))
}

fn render_dodot_up(
    env: &TutorialEnv,
    pack: &str,
    dry_run: bool,
    mode: OutputMode,
) -> Result<String> {
    let exec_ctx = env.make_ctx(dry_run);
    let result = commands::up::up_or_status_for_conflict(Some(&[pack.to_string()]), &exec_ctx)
        .map_err(|e| anyhow!("up: {e}"))?;
    let s = render::render("pack-status", &result, mode).map_err(|e| anyhow!("render: {e}"))?;
    Ok(indent(&s, "  "))
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|l| format!("{prefix}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write as IoWrite;
    use std::process::{Command, Stdio};

    let candidates: &[(&str, &[&str])] = &[
        ("pbcopy", &[]),
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];

    for (cmd, args) in candidates {
        if which_cmd(cmd).is_some() {
            let mut child = Command::new(cmd)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            let status = child.wait()?;
            if status.success() {
                return Ok(());
            }
        }
    }
    Err(anyhow!("no clipboard tool found"))
}

fn which_cmd(cmd: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let candidate = PathBuf::from(dir).join(cmd);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use dodot_lib::config::ConfigManager;
    use dodot_lib::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use dodot_lib::shell::NoopSyntaxChecker;
    use dodot_lib::testing::TempEnvironment;

    /// Stub command runner — every install / homebrew shell-out
    /// returns success without doing anything. Tests for the tutorial
    /// shouldn't actually invoke external programs.
    struct NoopCommandRunner;
    impl CommandRunner for NoopCommandRunner {
        fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput, dodot_lib::DodotError> {
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn env_from(temp: &TempEnvironment) -> TutorialEnv {
        let runner: Arc<dyn CommandRunner> = Arc::new(NoopCommandRunner);
        let datastore: Arc<dyn DataStore> = Arc::new(FilesystemDataStore::new(
            temp.fs.clone(),
            temp.paths.clone(),
            runner.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&temp.dotfiles_root).unwrap());
        TutorialEnv {
            fs: temp.fs.clone() as Arc<dyn Fs>,
            paths: temp.paths.clone() as Arc<dyn Pather>,
            datastore,
            command_runner: runner,
            config_manager,
            syntax_checker: Arc::new(NoopSyntaxChecker),
            root_origin: "test fixture".into(),
        }
    }

    fn opts_text() -> Options {
        Options {
            reset: true, // discard any leftover state file
            from: None,
            mode: OutputMode::Text,
        }
    }

    /// Happy path: config-only pack, user accepts every prompt, ends
    /// with the pack actually deployed.
    #[test]
    fn tutorial_deploys_a_config_only_pack_end_to_end() {
        let temp = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible\n")
            .done()
            .build();
        let env = env_from(&temp);

        let prompts = ScriptedPrompts::new([
            ScriptedAnswer::Confirm(true), // intro: ready?
            ScriptedAnswer::Confirm(true), // check_root: is this your repo?
            ScriptedAnswer::Choice(0),     // pick_pack: vim
            ScriptedAnswer::Enter,         // show_status
            ScriptedAnswer::Enter,         // annotate_status
            ScriptedAnswer::Confirm(true), // concept_targets: looks right?
            ScriptedAnswer::Confirm(true), // dry_run: apply for real?
            ScriptedAnswer::Enter,         // real_up
        ]);

        let mut buf: Vec<u8> = Vec::new();
        run_with_prompts(&env, opts_text(), &prompts, &mut buf).unwrap();

        let out = String::from_utf8(buf).unwrap();

        // The driver visited every step we expected.
        assert!(out.contains("Welcome to dodot"), "missing intro: {out}");
        assert!(
            out.contains("Step 1 — find your dotfiles repo"),
            "missing check_root: {out}"
        );
        assert!(
            out.contains("Step 2 — pick a pack"),
            "missing pick_pack: {out}"
        );
        assert!(
            out.contains("Step 3 — what dodot would do"),
            "missing show_status: {out}"
        );
        assert!(
            out.contains("Step 7 — make it real"),
            "missing real_up: {out}"
        );
        assert!(out.contains("You're set up."), "missing outro: {out}");

        // No prompt answers should be left over.
        assert_eq!(
            prompts.remaining(),
            0,
            "tutorial consumed fewer prompts than scripted"
        );

        // Pack actually got deployed — symlink chain exists.
        let user_target = temp.config_home.join("vim").join("vimrc");
        assert!(
            temp.fs.is_symlink(&user_target),
            "expected vim/vimrc to be a symlink at {}",
            user_target.display()
        );
    }

    /// Quitting at the intro should not deploy anything and should
    /// leave the saved-state file behind so the user can resume.
    #[test]
    fn tutorial_quit_at_intro_does_nothing() {
        let temp = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let env = env_from(&temp);

        let prompts = ScriptedPrompts::new([
            ScriptedAnswer::Confirm(false), // intro: not ready
        ]);

        let mut buf: Vec<u8> = Vec::new();
        run_with_prompts(&env, opts_text(), &prompts, &mut buf).unwrap();

        // Pack was not deployed.
        let user_target = temp.config_home.join("vim").join("vimrc");
        assert!(
            !temp.fs.exists(&user_target),
            "should NOT deploy when user quits at intro"
        );
    }

    /// No packs in the repo → tutorial offers `dodot init` and exits.
    #[test]
    fn tutorial_handles_empty_repo_gracefully() {
        let temp = TempEnvironment::builder().build();
        let env = env_from(&temp);

        let prompts = ScriptedPrompts::new([
            ScriptedAnswer::Confirm(true), // intro
            ScriptedAnswer::Confirm(true), // check_root
        ]);

        let mut buf: Vec<u8> = Vec::new();
        run_with_prompts(&env, opts_text(), &prompts, &mut buf).unwrap();

        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("No packs yet"), "should hit no_packs: {out}");
        assert!(
            out.contains("dodot init"),
            "should suggest dodot init: {out}"
        );
    }

    /// User picks the recommended starter when there are multiple
    /// packs of different kinds.
    #[test]
    fn tutorial_recommendation_orders_config_only_first() {
        let temp = TempEnvironment::builder()
            .pack("dev-tools")
            .file("install.sh", "echo")
            .done()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let env = env_from(&temp);

        // Tutorial sorts recommended pack first in the select; vim
        // (config-only) should be index 0 even though dev-tools comes
        // first alphabetically.
        let prompts = ScriptedPrompts::new([
            ScriptedAnswer::Confirm(true), // intro
            ScriptedAnswer::Confirm(true), // check_root
            ScriptedAnswer::Choice(0),     // pick the first (vim, recommended)
            ScriptedAnswer::Enter,         // show_status
            ScriptedAnswer::Enter,         // annotate_status
            ScriptedAnswer::Confirm(true), // concept_targets
            ScriptedAnswer::Confirm(true), // dry_run
            ScriptedAnswer::Enter,         // real_up
        ]);

        let mut buf: Vec<u8> = Vec::new();
        run_with_prompts(&env, opts_text(), &prompts, &mut buf).unwrap();

        let user_target = temp.config_home.join("vim").join("vimrc");
        assert!(
            temp.fs.exists(&user_target),
            "vim should have been deployed (it was the recommended pick)"
        );
        let dev_target = temp.config_home.join("dev-tools").join("install.sh");
        assert!(
            !temp.fs.exists(&dev_target),
            "dev-tools should NOT have been deployed"
        );
    }
}
