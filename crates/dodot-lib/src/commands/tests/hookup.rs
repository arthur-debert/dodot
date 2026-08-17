//! Which commands measure shell activation, and when
//! (`docs/proposals/shipped/shell-hookup.lex` §3.1, §5, §9).
//!
//! `shell::probe` covers the spawn mechanics and the verdict rendering;
//! this file covers the decision *around* them — the gate that keeps a
//! healthy machine from ever paying for a shell spawn, the commands
//! that are allowed to spawn one at all, and the verdict taking
//! precedence over the evidence guess it corrects.
//!
//! Every context here carries a fabricated `$SHELL` — a script written
//! into the test's temp home — so nothing in this file runs the
//! developer's real shell or reads their real rc files. A test that
//! wants "no probe happened" asserts on a marker file the fabricated
//! shell would have created had it run.

use std::path::PathBuf;
use std::time::Duration;

use super::support::make_ctx;
use crate::commands::{status, up};
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::shell::activation::{self, INIT_GEN_ENV};
use crate::shell::{ProbePolicy, ShellEnv};
use crate::testing::TempEnvironment;

fn env_with_shell_pack() -> TempEnvironment {
    TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build()
}

/// Where a fabricated shell records that it ran. Its absence after a
/// command is the proof that no probe happened.
fn spawn_marker(env: &TempEnvironment) -> PathBuf {
    env.home.join("probe-ran.marker")
}

/// Write a fake `$SHELL` that touches the spawn marker, then behaves
/// as `rc_behaviour` says before running the probe command.
///
/// Named `zsh` on purpose: the basename is what the rc ladder reads,
/// so the failed-probe diagnosis resolves `~/.zshrc` *inside the test's
/// temp home* — a real ladder walk, still nowhere near the developer's
/// own rc file.
fn fake_shell(env: &TempEnvironment, rc_behaviour: &str) -> PathBuf {
    let path = env.home.join("fake-bin/zsh");
    env.fs.mkdir_all(path.parent().unwrap()).unwrap();
    let script = format!(
        "#!/bin/sh\n: > {marker}\n{rc_behaviour}\neval \"$2\"\n",
        marker = spawn_marker(env).display(),
    );
    env.fs
        .write_file_with_mode(&path, script.as_bytes(), 0o755)
        .unwrap();
    path
}

/// A context allowed to probe, wired to a fabricated shell.
fn probing_ctx(env: &TempEnvironment, rc_behaviour: &str) -> ExecutionContext {
    let shell = fake_shell(env, rc_behaviour);
    let mut ctx = make_ctx(env);
    ctx.shell_probe = ProbePolicy::Gated {
        timeout: Duration::from_secs(10),
    };
    ctx.shell_env = ShellEnv {
        shell: Some(shell.display().to_string()),
        zdotdir: None,
    };
    ctx
}

/// An rc behaviour that sources dodot's real generated init script —
/// a genuine activation, heartbeat and all.
fn sources_dodot(env: &TempEnvironment) -> String {
    format!(". \"{}\"", env.paths.init_script_path().display())
}

/// The heartbeat a shell running this dodot would have left behind at
/// `generation`.
fn simulate_activation(env: &TempEnvironment, generation: u64) {
    env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
    env.fs
        .write_file(
            &env.paths.hookup_heartbeat_path(),
            format!("{generation} {}", activation::running_version()).as_bytes(),
        )
        .unwrap();
}

// ── The gate ────────────────────────────────────────────────────

#[test]
fn a_healthy_machine_never_spawns_a_probe() {
    let env = env_with_shell_pack();

    // One `up` to deploy, then a heartbeat at that generation: some
    // shell has activated since the last regeneration, which is proof
    // enough. The gate must stop before the spawn.
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    simulate_activation(&env, generation);

    let ctx = probing_ctx(&env, &sources_dodot(&env));
    let result = up::up(None, &ctx).unwrap();

    assert!(
        !env.fs.exists(&spawn_marker(&env)),
        "a fresh heartbeat is proof; spawning a shell anyway is the cost we promised not to pay"
    );
    // The footer still renders — it just costs nothing to produce.
    assert_eq!(result.shell_hookup.map(|n| n.state), Some("healthy".into()));
}

/// Permission to spawn is not a spawn. The gate above declines on a
/// fresh heartbeat — which is *exactly* the case #279 gave the session
/// signal to overrule: some other shell activated, and the one the
/// user is typing in demonstrably did not. Reading "will we measure?"
/// off the policy instead of off the gate meant `up` announced a
/// healthy hookup for that session while `status`, one command later
/// in the same terminal, correctly called it out — neither of them
/// having spawned anything.
#[test]
fn up_does_not_certify_a_session_it_never_measured() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    // Another shell activated; this one carries no stamp and is
    // attached to a terminal.
    simulate_activation(&env, generation);

    let mut ctx = probing_ctx(&env, &sources_dodot(&env));
    ctx.tty = true;
    let up_notice = up::up(None, &ctx).unwrap().shell_hookup.unwrap();

    assert!(
        !env.fs.exists(&spawn_marker(&env)),
        "the gate declines on a fresh heartbeat — no spawn happened"
    );
    assert_eq!(
        up_notice.state, "shell-not-loaded",
        "no measurement replaced the session evidence, so the session evidence stands"
    );

    // And the two commands agree, which is the point.
    let status_notice = status::status(None, &ctx).unwrap().shell_hookup.unwrap();
    assert_eq!(status_notice.state, up_notice.state);
    assert_eq!(status_notice.message, up_notice.message);
}

/// The other half of the same rule: when a spawn *does* happen, it
/// answers the question and the session signal stays out of it.
#[test]
fn a_measured_verdict_still_outranks_the_session_signal() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();

    // No heartbeat and no stamp, so the gate fires and the fabricated
    // shell — which really does source the init script — measures a
    // working hookup. A tty-attached session would otherwise have
    // reported "this shell did not load dodot".
    let mut ctx = probing_ctx(&env, &sources_dodot(&env));
    ctx.tty = true;
    let notice = up::up(None, &ctx).unwrap().shell_hookup.unwrap();

    assert!(env.fs.exists(&spawn_marker(&env)), "the gate fired");
    assert_eq!(notice.state, "healthy");
}

#[test]
fn a_live_shell_is_proof_enough_on_its_own() {
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();

    let mut ctx = probing_ctx(&env, &sources_dodot(&env));
    ctx.env_stamp = activation::EnvStamp::at(generation);
    up::up(None, &ctx).unwrap();

    assert!(
        !env.fs.exists(&spawn_marker(&env)),
        "the calling shell carries a current stamp — nothing left to measure"
    );
}

#[test]
fn status_never_spawns_a_shell_whatever_the_policy_says() {
    // Spec §9: status reports evidence; only `up` and `install` may
    // spawn. The policy below says probing is allowed, and status
    // must still refuse.
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();

    let ctx = probing_ctx(&env, &sources_dodot(&env));
    let result = status::status(None, &ctx).unwrap();

    assert!(!env.fs.exists(&spawn_marker(&env)));
    assert_eq!(
        result.shell_hookup.map(|n| n.state),
        Some("never-activated".into()),
        "status still reports, from evidence alone"
    );
}

#[test]
fn a_dry_run_measures_nothing() {
    let env = env_with_shell_pack();
    let mut ctx = probing_ctx(&env, &sources_dodot(&env));
    ctx.dry_run = true;

    up::up(None, &ctx).unwrap();

    assert!(
        !env.fs.exists(&spawn_marker(&env)),
        "a dry run wrote no script; measuring would report on a world it did not create"
    );
}

// ── The measurement ─────────────────────────────────────────────

#[test]
fn a_first_up_ends_on_a_measured_verdict_when_the_shell_activates() {
    let env = env_with_shell_pack();
    let ctx = probing_ctx(&env, &sources_dodot(&env));

    let result = up::up(None, &ctx).unwrap();

    assert!(
        env.fs.exists(&spawn_marker(&env)),
        "the probe must have run"
    );
    let notice = result
        .shell_hookup
        .expect("a measured verdict is the point of the first up");
    assert_eq!(notice.state, "healthy");
    assert_eq!(notice.severity, "ok");
    assert_eq!(notice.message, crate::shell::activation::HEALTHY_MESSAGE);
    // Targeted verification does not write the heartbeat; the footer
    // still reports the measured event without pretending it was
    // ordinary activation evidence.
    assert_eq!(
        notice.evidence,
        format!(
            "Verified just now by dodot {}.",
            activation::running_version()
        )
    );
}

#[test]
fn a_first_up_whose_shell_does_not_activate_names_the_missing_hook() {
    let env = env_with_shell_pack();
    // The shell runs and sources nothing — the fresh-install story.
    let ctx = probing_ctx(&env, "# no dodot hook here");

    let result = up::up(None, &ctx).unwrap();

    let notice = result.shell_hookup.expect("a broken hookup is news");
    assert_eq!(notice.state, "verified-broken");
    assert_eq!(notice.severity, "error");
    let hint = notice.hint.unwrap();
    assert!(
        hint.contains("dodot install --write"),
        "the fix must be named: {hint}"
    );
}

#[test]
fn a_measured_broken_hookup_beats_the_stale_shell_guess() {
    // The carried-over WS01 problem: with evidence alone, a hookup
    // that worked and then broke reads as a stale shell, so the user
    // is told to open a new shell — which fixes nothing. Once the
    // probe has run and found no stamp, the measurement wins.
    let env = env_with_shell_pack();
    up::up(None, &make_ctx(&env)).unwrap();
    let generation =
        activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
    // A shell activated once, before the current generation...
    simulate_activation(&env, generation - 1);

    // ...and today's shell no longer loads dodot at all.
    let ctx = probing_ctx(&env, "# the hook line was deleted");
    let result = up::up(None, &ctx).unwrap();

    let notice = result.shell_hookup.unwrap();
    assert_eq!(notice.state, "verified-broken");
    assert!(
        !notice.hint.unwrap().contains("Open a new shell"),
        "no new shell fixes a deleted hook line"
    );
}

#[test]
fn an_unverifiable_probe_degrades_instead_of_wedging_up() {
    let env = env_with_shell_pack();
    let mut ctx = probing_ctx(&env, &sources_dodot(&env));
    // A $SHELL that cannot be executed at all.
    ctx.shell_env = ShellEnv {
        shell: Some(env.home.join("not-a-real-shell").display().to_string()),
        zdotdir: None,
    };

    let result = up::up(None, &ctx).expect("a failed probe must never wedge `up`");

    let notice = result
        .shell_hookup
        .expect("evidence still has something to say");
    assert_eq!(
        notice.state, "never-activated",
        "degrade to the evidence answer"
    );
    let hint = notice.hint.unwrap();
    assert!(
        hint.contains("could not verify") && hint.contains("not measured activation"),
        "the degradation must be labeled, not passed off as a measurement: {hint}"
    );
}

#[test]
fn the_probed_shell_does_not_inherit_this_process_stamp() {
    // An inherited stamp would make an unhooked shell look verified.
    let env = env_with_shell_pack();
    let _guard = crate::testing::EnvVarGuard::set(INIT_GEN_ENV, "999999");
    let ctx = probing_ctx(&env, "# sources nothing");

    let result = up::up(None, &ctx).unwrap();

    assert_eq!(
        result.shell_hookup.map(|n| n.state),
        Some("verified-broken".into())
    );
}
