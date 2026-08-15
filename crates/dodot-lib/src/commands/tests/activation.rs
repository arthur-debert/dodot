//! Shell-hookup activation surfacing in `up` and `status`
//! (`docs/proposals/shipped/shell-hookup.lex` §5).
//!
//! The evidence ladder itself is unit-tested in
//! `shell::activation`; these tests pin the part users see — which
//! command says what, and when it stays quiet.

use standout_render::OutputMode;

use super::support::make_ctx;
use crate::commands::{status, up};
use crate::fs::Fs;
use crate::paths::Pather;
use crate::render;
use crate::shell::activation;
use crate::testing::TempEnvironment;

/// A pack with one shell source, so `up` has something to deploy and
/// the init script has something to say.
fn env_with_shell_pack() -> TempEnvironment {
    TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build()
}

/// Stand in for a shell that sourced the init script at `generation`:
/// the heartbeat it would have left behind.
fn simulate_activation(env: &TempEnvironment, generation: u64) {
    env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
    env.fs
        .write_file(
            &env.paths.hookup_heartbeat_path(),
            generation.to_string().as_bytes(),
        )
        .unwrap();
}

#[test]
fn up_on_a_fresh_install_ends_with_the_never_activated_warning() {
    let env = env_with_shell_pack();
    let ctx = make_ctx(&env);

    let result = up::up(None, &ctx).unwrap();

    let notice = result
        .shell_hookup
        .expect("a green first up must not stay silent about the missing hookup");
    assert_eq!(notice.state, "never-activated");
    assert_eq!(notice.severity, "warning");
    let hint = notice.hint.expect("never-activated must say what to do");
    assert!(
        hint.contains("dodot-init.sh"),
        "hint must name the manual hook line: {hint}"
    );
}

#[test]
fn up_from_a_live_shell_says_nothing_about_the_hookup() {
    let env = env_with_shell_pack();

    // First up: deploys and writes generation N.
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    simulate_activation(&env, generation);

    // Second up, from a shell that sourced generation N. The run
    // regenerates the script, but the verdict is judged against the
    // generation this shell could have loaded — otherwise every `up`
    // would tell the user their shell is stale.
    let mut ctx = make_ctx(&env);
    ctx.env_init_gen = Some(generation);
    let result = up::up(None, &ctx).unwrap();

    assert_eq!(
        result.shell_hookup, None,
        "a healthy hookup produces no warning noise on up"
    );
}

#[test]
fn status_from_a_stale_shell_hints_at_opening_a_new_shell() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();

    // This terminal sourced an older generation and has been open
    // across the deploy.
    simulate_activation(&env, generation - 1);
    let mut ctx = make_ctx(&env);
    ctx.env_init_gen = Some(generation - 1);

    let notice = status::status(None, &ctx).unwrap().shell_hookup.unwrap();
    assert_eq!(notice.state, "stale-shell");
    assert_eq!(notice.severity, "info");
    assert!(notice.hint.unwrap().contains("Open a new shell"));
}

#[test]
fn status_reports_a_quiet_ok_for_a_healthy_hookup() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    simulate_activation(&env, generation);

    let mut ctx = make_ctx(&env);
    ctx.env_init_gen = Some(generation);

    let notice = status::status(None, &ctx).unwrap().shell_hookup.unwrap();
    assert_eq!(notice.state, "healthy");
    assert_eq!(notice.severity, "ok");
    assert_eq!(notice.hint, None);
}

/// A shell that never sourced init (cron, an editor task runner) is
/// not evidence of a broken hookup as long as some shell activated at
/// the current generation — but only while dodot is *not* attached to
/// a terminal. A detached process cannot tell "I am not a shell
/// session" apart from a shell session, so it defers to the heartbeat.
#[test]
fn status_stays_healthy_when_the_heartbeat_is_current_but_this_process_has_no_stamp() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    simulate_activation(&env, generation);

    let notice = status::status(None, &make_ctx(&env))
        .unwrap()
        .shell_hookup
        .unwrap();
    assert_eq!(notice.state, "healthy");
}

/// The dead-hookup primary case (#279): the hookup used to work (the
/// heartbeat holds the current generation), then broke — and the shell
/// the user is typing in (tty attached, no stamp) demonstrably did not
/// load dodot. The old high-water-mark heartbeat must not keep
/// certifying "ok"; the rc scan supplies the diagnosis: the hook is
/// absent from the rc file, so name the file and the fix.
#[test]
fn status_from_a_tty_shell_without_a_stamp_reports_the_dead_hookup() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    // Some shell activated back when the hookup still worked…
    simulate_activation(&env, generation);

    // …but the terminal in front of the user sourced nothing.
    let mut ctx = make_ctx(&env);
    ctx.tty = true;
    ctx.shell_env = crate::shell::ShellEnv {
        shell: Some("/bin/zsh".into()),
        zdotdir: None,
    };
    // No ~/.zshrc with a hook exists in this home: the hookup is dead.

    let notice = status::status(None, &ctx).unwrap().shell_hookup.unwrap();
    assert_eq!(notice.state, "shell-not-loaded");
    assert_eq!(notice.severity, "warning");
    assert!(
        notice.message.contains("hasn't loaded dodot"),
        "the message states what is literally true: {}",
        notice.message
    );
    let hint = notice.hint.unwrap();
    assert!(
        hint.contains("~/.zshrc") && hint.contains("dodot install --write"),
        "an absent hook names the rc file and the fix: {hint}"
    );
    assert!(
        !hint.contains("new shell"),
        "no new shell fixes a missing hook: {hint}"
    );
}

/// The second-order case (#279): after `up` has bumped the generation,
/// the heartbeat ages into Old — but when the hook is absent from the
/// rc, "open a new shell" is wrong advice. The rc scan decides.
#[test]
fn status_after_an_up_does_not_advise_a_new_shell_when_the_hook_is_gone() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    // The heartbeat predates the current generation: the classic
    // stale-shell evidence shape.
    simulate_activation(&env, generation - 1);

    let mut ctx = make_ctx(&env);
    ctx.tty = true;
    ctx.shell_env = crate::shell::ShellEnv {
        shell: Some("/bin/zsh".into()),
        zdotdir: None,
    };

    let notice = status::status(None, &ctx).unwrap().shell_hookup.unwrap();
    assert_eq!(notice.state, "shell-not-loaded");
    let hint = notice.hint.unwrap();
    assert!(
        !hint.contains("new shell"),
        "a missing hook must not get stale-shell advice: {hint}"
    );
    assert!(hint.contains("dodot install --write"), "hint: {hint}");
}

/// When the hook *is* wired in the rc, a tty shell without a stamp is
/// most plausibly a shell opened before the hook landed: the advice
/// stays "open a new one", phrased as what the evidence supports.
#[test]
fn status_from_a_tty_shell_with_the_hook_wired_advises_a_new_shell() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    simulate_activation(&env, generation);

    let hook = activation::hook_line(&env.paths.init_script_path(), env.paths.home_dir());
    env.fs
        .write_file(&env.paths.home_dir().join(".zshrc"), hook.as_bytes())
        .unwrap();

    let mut ctx = make_ctx(&env);
    ctx.tty = true;
    ctx.shell_env = crate::shell::ShellEnv {
        shell: Some("/bin/zsh".into()),
        zdotdir: None,
    };

    let notice = status::status(None, &ctx).unwrap().shell_hookup.unwrap();
    assert_eq!(notice.state, "shell-not-loaded");
    assert_eq!(notice.severity, "info");
    assert!(
        notice.hint.unwrap().contains("new shell"),
        "hook present: an old shell is the likely story"
    );
}

/// A live tty shell keeps its quiet ok: the tty signal only breaks the
/// tie when the stamp is absent.
#[test]
fn status_from_a_live_tty_shell_stays_healthy() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    simulate_activation(&env, generation);

    let mut ctx = make_ctx(&env);
    ctx.tty = true;
    ctx.env_init_gen = Some(generation);

    let notice = status::status(None, &ctx).unwrap().shell_hookup.unwrap();
    assert_eq!(notice.state, "healthy");
}

#[test]
fn down_says_nothing_about_the_hookup() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();

    let result = crate::commands::down::down(None, &make_ctx(&env)).unwrap();
    assert_eq!(result.shell_hookup, None);
}

#[test]
fn the_never_activated_notice_renders_as_a_prominent_block() {
    let env = env_with_shell_pack();
    let result = up::up(None, &make_ctx(&env)).unwrap();

    let text = render::render("pack-status", &result, OutputMode::Text).unwrap();
    assert!(
        text.contains("no shell has loaded dodot yet"),
        "the warning must reach rendered output: {text}"
    );
    assert!(
        text.contains("dodot install --write") && text.contains("dodot-init.sh"),
        "the fix — the command and the manual line — must reach rendered output: {text}"
    );

    let tagged = render::render("pack-status", &result, OutputMode::TermDebug).unwrap();
    assert!(
        tagged.contains("[warning]⚠ Deployed, but no shell has loaded dodot yet.[/warning]"),
        "never-activated renders in the warning style: {tagged}"
    );
}
