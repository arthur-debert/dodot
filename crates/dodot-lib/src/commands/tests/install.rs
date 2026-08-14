//! `dodot install` — the rc-file ladder, the marked block, and the
//! refusals (`docs/proposals/shell-hookup.lex` §4).
//!
//! Every context here carries a *fabricated* shell environment and the
//! default `ProbePolicy::Never`, so nothing in this file can reach the
//! developer's real `$SHELL` or their real `~/.zshrc`. The measured
//! half — spawning a shell and reading the stamp back — is tested in
//! `shell::probe` against fabricated shell scripts.

use crate::commands::install::*;
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::shell::rc::{BLOCK_END, BLOCK_START};
use crate::shell::ShellEnv;
use crate::testing::TempEnvironment;

/// A context whose shell environment is fabricated — never the
/// developer's own. `shell_probe` stays `Never`, so nothing here
/// spawns a process.
fn ctx_for(env: &TempEnvironment, shell_env: ShellEnv) -> ExecutionContext {
    let mut ctx = super::support::make_ctx(env);
    ctx.shell_env = shell_env;
    ctx
}

fn zsh_env() -> ShellEnv {
    ShellEnv {
        shell: Some("/bin/zsh".into()),
        zdotdir: None,
    }
}

fn dry() -> InstallOptions {
    InstallOptions::default()
}

fn write() -> InstallOptions {
    InstallOptions {
        write: true,
        rc: None,
    }
}

// ── Dry ─────────────────────────────────────────────────────

#[test]
fn dry_run_reports_everything_and_writes_nothing() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let rc_path = env.home.join(".zshrc");

    let r = install(&ctx, &dry()).unwrap();

    assert_eq!(r.shell.as_deref(), Some("zsh"));
    assert_eq!(r.rc_path, "~/.zshrc");
    assert!(!r.rc_exists);
    assert_eq!(r.hook_presence, "absent");
    assert!(
        r.hook_line.contains("dodot-init.sh") && r.hook_line.starts_with("[ -f "),
        "the guarded direct-source line, not an eval: {}",
        r.hook_line
    );
    assert!(!r.wrote);
    assert_eq!(r.outcome, None);
    assert!(
        !env.fs.exists(&rc_path),
        "a bare `dodot install` must not create anything"
    );
}

#[test]
fn dry_run_reports_an_existing_hook_as_present() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    env.fs
        .write_file(&env.home.join(".zshrc"), b"eval \"$(dodot init-sh)\"\n")
        .unwrap();

    let r = install(&ctx, &dry()).unwrap();
    assert_eq!(
        r.hook_presence, "manual",
        "a hand-wired eval hookup is present and must be left alone"
    );
}

#[test]
fn an_unsupported_shell_is_refused_with_the_line_and_the_escape_hatch() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/usr/bin/fish".into()),
            zdotdir: None,
        },
    );

    let err = install(&ctx, &write()).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("fish"), "{msg}");
    assert!(msg.contains("dodot-init.sh"), "must print the line: {msg}");
    assert!(msg.contains("--rc"), "must name the escape hatch: {msg}");
}

#[test]
fn an_unsupported_shell_still_works_when_rc_is_given() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/usr/bin/fish".into()),
            zdotdir: None,
        },
    );
    let custom = env.home.join("config.fish");

    let r = install(
        &ctx,
        &InstallOptions {
            write: true,
            rc: Some(custom.clone()),
        },
    )
    .unwrap();
    assert_eq!(r.shell, None);
    assert!(env
        .fs
        .read_to_string(&custom)
        .unwrap()
        .contains(BLOCK_START));
}

// ── Write ───────────────────────────────────────────────────

#[test]
fn write_creates_a_missing_rc_with_the_marked_block() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());

    let r = install(&ctx, &write()).unwrap();
    assert!(r.wrote);
    assert_eq!(r.outcome.as_deref(), Some("created"));
    // The report describes the world after the run, not before it.
    assert!(r.rc_exists);
    assert_eq!(r.hook_presence, "managed-block");

    let body = env.fs.read_to_string(&env.home.join(".zshrc")).unwrap();
    assert!(body.contains(BLOCK_START) && body.contains(BLOCK_END));
    assert!(
        body.contains(&r.hook_line),
        "the block must carry the one hook line: {body}"
    );
    assert!(
        !body.contains("dodot init-sh"),
        "direct source, not eval: {body}"
    );
}

#[test]
fn write_is_idempotent_and_never_duplicates_the_block() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let rc_path = env.home.join(".zshrc");
    env.fs
        .write_file(&rc_path, b"export PATH=/usr/local/bin:$PATH\n")
        .unwrap();

    let first = install(&ctx, &write()).unwrap();
    assert_eq!(first.outcome.as_deref(), Some("appended"));
    let after_first = env.fs.read_to_string(&rc_path).unwrap();

    let second = install(&ctx, &write()).unwrap();
    assert_eq!(second.outcome.as_deref(), Some("unchanged"));
    assert_eq!(env.fs.read_to_string(&rc_path).unwrap(), after_first);
    assert_eq!(after_first.matches(BLOCK_START).count(), 1);
    assert!(
        after_first.starts_with("export PATH=/usr/local/bin:$PATH\n"),
        "user content must survive: {after_first}"
    );
}

#[test]
fn an_existing_eval_hookup_is_left_exactly_where_it_is() {
    // Compatibility (spec §6): people who wired `eval "$(dodot
    // init-sh)"` by hand keep working, and `--write` is additive —
    // dodot owns its block and nothing else in the file.
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let rc_path = env.home.join(".zshrc");
    let manual = "# my rc\neval \"$(dodot init-sh)\"\nalias ll='ls -l'\n";
    env.fs.write_file(&rc_path, manual.as_bytes()).unwrap();

    install(&ctx, &write()).unwrap();

    let body = env.fs.read_to_string(&rc_path).unwrap();
    assert!(
        body.starts_with(manual),
        "the hand-wired hookup must survive verbatim: {body}"
    );
    assert!(body.contains(BLOCK_START));
}

#[test]
fn write_replaces_a_stale_block_in_place() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let rc_path = env.home.join(".zshrc");
    let stale = format!("{BLOCK_START}\n. \"$HOME/old/path/dodot-init.sh\"\n{BLOCK_END}\n");
    env.fs.write_file(&rc_path, stale.as_bytes()).unwrap();

    let r = install(&ctx, &write()).unwrap();
    assert_eq!(r.outcome.as_deref(), Some("replaced"));
    let body = env.fs.read_to_string(&rc_path).unwrap();
    assert!(!body.contains("old/path"), "{body}");
    assert_eq!(body.matches(BLOCK_START).count(), 1);
}

#[test]
fn write_follows_a_symlinked_rc_into_the_dotfiles_repo() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let repo_rc = env.home.join("dotfiles/zsh/zshrc");
    env.fs.mkdir_all(repo_rc.parent().unwrap()).unwrap();
    env.fs.write_file(&repo_rc, b"# repo rc\n").unwrap();
    env.fs.symlink(&repo_rc, &env.home.join(".zshrc")).unwrap();

    let r = install(&ctx, &write()).unwrap();

    assert!(env
        .fs
        .read_to_string(&repo_rc)
        .unwrap()
        .contains(BLOCK_START));
    assert!(
        env.fs.is_symlink(&env.home.join(".zshrc")),
        "the link itself must survive — we write through it"
    );
    assert_eq!(r.rc_link_source.as_deref(), Some("~/.zshrc"));
    assert!(
        r.notes.iter().any(|n| n.contains("dotfiles/zsh/zshrc")),
        "landing in the repo must be announced: {:?}",
        r.notes
    );
}

#[test]
fn write_honors_the_zdotdir_ladder() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/bin/zsh".into()),
            zdotdir: Some("$HOME/.config/zsh".into()),
        },
    );

    let r = install(&ctx, &write()).unwrap();
    assert_eq!(r.rc_path, "~/.config/zsh/.zshrc");
    assert!(env
        .fs
        .read_to_string(&env.home.join(".config/zsh/.zshrc"))
        .unwrap()
        .contains(BLOCK_START));
}

#[test]
fn rc_override_wins_over_the_whole_ladder() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let custom = env.home.join("somewhere/else.sh");

    let r = install(
        &ctx,
        &InstallOptions {
            write: true,
            rc: Some(custom.clone()),
        },
    )
    .unwrap();
    assert_eq!(r.rc_path, "~/somewhere/else.sh");
    assert!(env.fs.exists(&custom));
    assert!(
        !env.fs.exists(&env.home.join(".zshrc")),
        "the ladder's target must be left alone when --rc is given"
    );
}

#[test]
fn macos_bash_gets_the_login_shell_chain() {
    let env = TempEnvironment::builder().build();
    let mut ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/bin/bash".into()),
            zdotdir: None,
        },
    );
    ctx.host_facts = std::sync::Arc::new(crate::gates::HostFacts {
        os: "darwin".into(),
        arch: "arm64".into(),
        hostname: None,
        username: None,
    });

    let r = install(&ctx, &write()).unwrap();

    assert_eq!(r.rc_path, "~/.bashrc");
    let profile = env
        .fs
        .read_to_string(&env.home.join(".bash_profile"))
        .unwrap();
    assert!(
        profile.contains(".bashrc"),
        "Terminal opens login shells; without the chain the hook is never read: {profile}"
    );
    assert!(
        r.notes.iter().any(|n| n.contains("login shells")),
        "the extra file must be announced: {:?}",
        r.notes
    );
}

/// The chain is rung 2 of the ladder, and `--rc` overrides the whole
/// ladder (spec §4.3 rung 5). Chaining an override that points at the
/// profile would write a second block with the *same markers* over the
/// hook block, leaving the user a file that chains to a `.bashrc`
/// nobody wired — and a report claiming `managed-block`.
#[test]
fn an_rc_override_at_the_profile_keeps_its_hook_and_gets_no_chain() {
    let env = TempEnvironment::builder().build();
    let mut ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/bin/bash".into()),
            zdotdir: None,
        },
    );
    ctx.host_facts = std::sync::Arc::new(crate::gates::HostFacts {
        os: "darwin".into(),
        arch: "arm64".into(),
        hostname: None,
        username: None,
    });
    let profile = env.home.join(".bash_profile");

    let r = install(
        &ctx,
        &InstallOptions {
            write: true,
            rc: Some(profile.clone()),
        },
    )
    .unwrap();

    let body = env.fs.read_to_string(&profile).unwrap();
    assert!(
        body.contains(&r.hook_line),
        "the hook the user asked for must survive the chain step: {body}"
    );
    assert_eq!(
        body.matches(BLOCK_START).count(),
        1,
        "one block, and it is the hook's: {body}"
    );
    assert!(
        !body.contains(".bashrc"),
        "login shells read this file directly; there is nothing to chain to: {body}"
    );
    assert_eq!(r.hook_presence, "managed-block");
}

/// A `--rc` override anywhere else is left alone too — no chain, and no
/// `~/.bash_profile` conjured next to a file the user chose.
#[test]
fn an_rc_override_elsewhere_gets_no_macos_chain() {
    let env = TempEnvironment::builder().build();
    let mut ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/bin/bash".into()),
            zdotdir: None,
        },
    );
    ctx.host_facts = std::sync::Arc::new(crate::gates::HostFacts {
        os: "darwin".into(),
        arch: "arm64".into(),
        hostname: None,
        username: None,
    });
    let custom = env.home.join(".config/bash/rc.sh");

    install(
        &ctx,
        &InstallOptions {
            write: true,
            rc: Some(custom.clone()),
        },
    )
    .unwrap();

    assert!(env
        .fs
        .read_to_string(&custom)
        .unwrap()
        .contains(BLOCK_START));
    assert!(
        !env.fs.exists(&env.home.join(".bash_profile")),
        "--rc overrides the whole ladder, chain included"
    );
}

/// The chain still fires for a `~/.bashrc` that lives in a dotfiles
/// repo: bash opens `~/.bashrc` whatever it points at, so the nominal
/// path is what the rung-2 rule reads.
#[test]
fn macos_bash_chains_a_symlinked_bashrc_too() {
    let env = TempEnvironment::builder().build();
    let mut ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/bin/bash".into()),
            zdotdir: None,
        },
    );
    ctx.host_facts = std::sync::Arc::new(crate::gates::HostFacts {
        os: "darwin".into(),
        arch: "arm64".into(),
        hostname: None,
        username: None,
    });
    let repo_rc = env.home.join("dotfiles/bash/bashrc");
    env.fs.mkdir_all(repo_rc.parent().unwrap()).unwrap();
    env.fs.write_file(&repo_rc, b"# repo rc\n").unwrap();
    env.fs.symlink(&repo_rc, &env.home.join(".bashrc")).unwrap();

    install(&ctx, &write()).unwrap();

    assert!(env
        .fs
        .read_to_string(&repo_rc)
        .unwrap()
        .contains(BLOCK_START));
    assert!(env
        .fs
        .read_to_string(&env.home.join(".bash_profile"))
        .unwrap()
        .contains(".bashrc"));
}

#[test]
fn linux_bash_gets_no_profile_chain() {
    let env = TempEnvironment::builder().build();
    let mut ctx = ctx_for(
        &env,
        ShellEnv {
            shell: Some("/bin/bash".into()),
            zdotdir: None,
        },
    );
    ctx.host_facts = std::sync::Arc::new(crate::gates::HostFacts {
        os: "linux".into(),
        arch: "x86_64".into(),
        hostname: None,
        username: None,
    });

    install(&ctx, &write()).unwrap();
    assert!(
        !env.fs.exists(&env.home.join(".bash_profile")),
        "the chain is a macOS-only fix; elsewhere .bashrc is read directly"
    );
}

// ── Rendering ───────────────────────────────────────────────

/// Render an `InstallResult` through the real template.
fn rendered(result: &InstallResult, mode: standout_render::OutputMode) -> String {
    standout_render::render_with_output(
        crate::render::TEMPLATE_INSTALL,
        result,
        &crate::render::create_theme(),
        mode,
    )
    .expect("the install template must render")
}

#[test]
fn the_dry_report_reaches_rendered_output() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let r = install(&ctx, &dry()).unwrap();

    let text = rendered(&r, standout_render::OutputMode::Text);
    assert!(text.contains("--write"), "{text}");
    assert!(text.contains("zsh"), "{text}");
    assert!(text.contains("~/.zshrc"), "{text}");
    assert!(text.contains(&r.hook_line), "{text}");

    // Every style tag the template uses must exist in the theme —
    // an unknown one renders as `[name?]` in TermDebug.
    let tagged = rendered(&r, standout_render::OutputMode::TermDebug);
    for line in tagged.lines() {
        assert!(!line.contains("?]"), "unknown style tag: {line}");
    }
}

#[test]
fn the_written_report_names_the_file_and_the_notes() {
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let r = install(&ctx, &write()).unwrap();

    let text = rendered(&r, standout_render::OutputMode::Text);
    assert!(text.contains("~/.zshrc"), "{text}");
    assert!(text.contains("created"), "{text}");
}

#[test]
fn nothing_spawns_a_shell_when_the_policy_says_never() {
    // The whole suite runs with `ProbePolicy::Never`, so this pins
    // the guarantee itself: a written install with no probe policy
    // still reports, from evidence, without measuring.
    let env = TempEnvironment::builder().build();
    let ctx = ctx_for(&env, zsh_env());
    let r = install(&ctx, &write()).unwrap();
    // No init script deployed yet, so evidence has nothing to say.
    assert_eq!(r.shell_hookup, None);
}
