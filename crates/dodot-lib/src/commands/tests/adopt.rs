//! Integration tests for the `adopt` command.

#![allow(unused_imports)]

use std::sync::Arc;

use crate::commands;
use crate::config::ConfigManager;
use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::render;
use crate::testing::TempEnvironment;
use crate::Result;
use standout_render::OutputMode;

use super::support::{make_ctx, make_ctx_with_fs, make_ctx_with_runner, CannedRunner};

// ── adopt ───────────────────────────────────────────────────

#[test]
fn adopt_moves_file_and_creates_symlink() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "set nocompatible")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let result = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    // The `home.` prefix preserves the round-trip back to ~/.vimrc on `up`.
    env.assert_regular_file(
        &env.dotfiles_root.join("vim/home.vimrc"),
        "set nocompatible",
    );
    assert!(env.fs.is_symlink(&source));

    assert!(result.packs.iter().any(|p| p.name == "vim"));
    let vim = result.packs.iter().find(|p| p.name == "vim").unwrap();
    assert!(vim.files.iter().any(|f| f.name == "home.vimrc"));
}

#[test]
fn adopt_preserves_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;

    // A dotted source isolates executable-bit preservation from the
    // separate refusal of non-dotted $HOME entries.
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("placeholder", "")
        .done()
        .home_file(".script.sh", "#!/bin/sh\necho hi")
        .build();

    let source = env.home.join(".script.sh");
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&source, perms).unwrap();

    let ctx = make_ctx(&env);
    commands::adopt::adopt(
        Some("tools"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let dest = env.dotfiles_root.join("tools/home.script.sh");
    let meta = std::fs::metadata(&dest).unwrap();
    assert_eq!(
        meta.permissions().mode() & 0o777,
        0o755,
        "executable bit should be preserved on adopted file"
    );
}

/// A non-dotted entry in $HOME has no automatic round-trip path under
/// the XDG default, so adopt must refuse rather than silently relocate.
#[test]
fn adopt_refuses_non_dotted_home_entry() {
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("placeholder", "")
        .done()
        .home_file("script.sh", "#!/bin/sh\necho hi")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join("script.sh");
    let err = commands::adopt::adopt(
        Some("tools"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("non-dotted entry in $HOME"),
        "expected refusal message, got: {msg}"
    );
    assert!(
        msg.contains("[symlink.targets]"),
        "refusal should point at [symlink.targets] escape hatch, got: {msg}"
    );
    env.assert_regular_file(&source, "#!/bin/sh\necho hi");
    env.assert_not_exists(&env.dotfiles_root.join("tools/script.sh"));
}

#[test]
fn adopt_destination_conflict_refused_without_force() {
    // Adopt derives `home.vimrc`, so the existing pack file is a conflict.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "existing content")
        .done()
        .home_file(".vimrc", "new content")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let err = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::SymlinkConflict { .. }),
        "expected SymlinkConflict, got: {err}"
    );

    env.assert_regular_file(&source, "new content");
    env.assert_regular_file(
        &env.dotfiles_root.join("vim/home.vimrc"),
        "existing content",
    );
}

#[test]
fn adopt_destination_conflict_resolved_with_force() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    env.assert_regular_file(&env.dotfiles_root.join("vim/home.vimrc"), "NEW");
    assert!(env.fs.is_symlink(&source));
}

#[test]
fn adopt_directory_creates_symlink_and_preserves_contents() {
    // Dotted-directory contents move to
    // pack/_home/<stripped>/, which round-trips back via the `_home/`
    // subtree-escape (Priority 2) on `dodot up`. We use a non-XDG
    // dotted dir so the test stays decoupled from the XDG-source
    // inference rules — adopting `~/.config/` itself is refused
    // explicitly (see `adopt_xdg_root_itself_refused`).
    let env = TempEnvironment::builder()
        .pack("editor")
        .file("placeholder", "")
        .done()
        .home_file(".vim/vimrc", "set nocompatible")
        .home_file(".vim/colors/scheme.vim", "\" colors")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vim");

    commands::adopt::adopt(
        Some("editor"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let pack_dir = env.dotfiles_root.join("editor/_home/vim");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("vimrc"), "set nocompatible");
    env.assert_regular_file(&pack_dir.join("colors/scheme.vim"), "\" colors");

    assert!(env.fs.is_symlink(&source));
    let target = env.fs.readlink(&source).unwrap();
    assert_eq!(target, pack_dir);
}

/// A dotted directory adopted from $HOME (not in force_home) must
/// round-trip via the
/// `_home/` escape hatch on `dodot up`. Without this, the file would
/// silently move from $HOME/.X to $XDG_CONFIG_HOME/<pack>/X.
#[test]
fn adopt_dotted_dir_from_home_round_trips_via_home_escape() {
    let env = TempEnvironment::builder()
        .pack("chats")
        .file("placeholder", "")
        .done()
        .home_file(".weechat/weechat.conf", "[server]")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".weechat");

    commands::adopt::adopt(
        Some("chats"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let pack_dir = env.dotfiles_root.join("chats/_home/weechat");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("weechat.conf"), "[server]");

    commands::up::up(Some(&["chats".into()]), &ctx).unwrap();
    let user_path = env.home.join(".weechat");
    assert!(
        env.fs.is_symlink(&user_path),
        "~/.weechat should be a symlink after re-deploy"
    );
}

/// **Round-trip property** — the critical contract between `adopt` and
/// `resolve_target`. For every `$HOME` source that `adopt` accepts,
/// feeding the `derive_pack_filename` result back through
/// `resolve_target` must return the original source path.
///
/// `derive_pack_filename` encodes the *inverse* of `resolve_target`'s
/// priority rules (force_home, home. prefix, _home/ directory). The
/// two functions are separately implemented but must stay lockstep;
/// this test catches any drift directly.
///
/// Cases cover every accepted branch:
///   - force_home file (`~/.bashrc`)
///   - force_home directory (`~/.ssh`)
///   - dotted non-force_home file (`~/.vimrc`)
///   - dotted non-force_home directory (`~/.weechat`)
///
/// The refused branch (non-dotted $HOME entry) is covered by the
/// explicit refusal test `adopt_refuses_non_dotted_home_entry`.
#[test]
fn pack_filename_round_trips_through_resolve_target() {
    use crate::commands::adopt::derive_pack_filename;
    use crate::handlers::symlink::resolve_target;

    // Default force_home: match what dodot ships (keep this minimal
    // and explicit so test failures point at a real behavior change).
    let force_home: Vec<String> = vec![
        "ssh".into(),
        "gnupg".into(),
        "aws".into(),
        "kube".into(),
        "bashrc".into(),
        "zshrc".into(),
        "profile".into(),
        "inputrc".into(),
    ];
    let config = crate::handlers::HandlerConfig {
        force_home: force_home.clone(),
        ..crate::handlers::HandlerConfig::default()
    };

    let paths = crate::paths::XdgPather::builder()
        .home("/home/alice")
        .dotfiles_root("/home/alice/dotfiles")
        .xdg_config_home("/home/alice/.config")
        .build()
        .unwrap();

    struct Case {
        pack: &'static str,
        // The file/dir name as it would appear inside $HOME (.vimrc, .ssh, …).
        home_name: &'static str,
        is_dir: bool,
        // What `derive_pack_filename` should produce (here as documentation; the
        // test only asserts the round-trip, not the literal pack filename — a
        // future refactor of the inverse rules is allowed to pick a different
        // internal representation as long as the round-trip still holds).
        expected_pack_filename: &'static str,
    }

    let cases = [
        Case {
            pack: "shell",
            home_name: ".bashrc",
            is_dir: false,
            expected_pack_filename: "bashrc",
        },
        Case {
            pack: "net",
            home_name: ".ssh",
            is_dir: true,
            expected_pack_filename: "ssh",
        },
        Case {
            pack: "vim",
            home_name: ".vimrc",
            is_dir: false,
            expected_pack_filename: "home.vimrc",
        },
        Case {
            pack: "chats",
            home_name: ".weechat",
            is_dir: true,
            expected_pack_filename: "_home/weechat",
        },
    ];

    for c in &cases {
        let derived =
            derive_pack_filename(c.home_name, c.is_dir, &force_home).unwrap_or_else(|e| {
                panic!(
                    "derive_pack_filename refused accepted case {:?}: {e}",
                    c.home_name
                )
            });
        assert_eq!(
            derived, c.expected_pack_filename,
            "documentation-expected pack filename drifted for {}",
            c.home_name
        );

        let target = resolve_target(c.pack, &derived, &config, &paths);
        let expected_source = std::path::PathBuf::from(format!("/home/alice/{}", c.home_name));
        assert_eq!(
            target,
            expected_source,
            "round-trip broke for {}: derive_pack_filename → {} → resolve_target → {} \
             (expected back at {})",
            c.home_name,
            derived,
            target.display(),
            expected_source.display(),
        );
    }

    // Refused case: non-dotted entry — no round-trip path exists.
    let refused = derive_pack_filename("my_script.sh", false, &force_home);
    assert!(
        refused.is_err(),
        "non-dotted $HOME entry must be refused, got: {refused:?}"
    );
}

#[test]
fn adopt_preserves_inner_symlinks_as_symlinks() {
    // A dotted directory isolates inner-symlink preservation from the
    // separate refusal of non-dotted $HOME entries.
    let env = TempEnvironment::builder()
        .pack("shell")
        .file("placeholder", "")
        .done()
        .home_file(".mydir/real.txt", "hello")
        .build();

    let inner_target = env.home.join(".mydir/real.txt");
    let inner_link = env.home.join(".mydir/alias");
    env.fs.symlink(&inner_target, &inner_link).unwrap();

    let ctx = make_ctx(&env);
    let source = env.home.join(".mydir");
    commands::adopt::adopt(
        Some("shell"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let copied_link = env.dotfiles_root.join("shell/_home/mydir/alias");
    assert!(
        env.fs.is_symlink(&copied_link),
        "inner symlink should be preserved as a symlink, not followed"
    );
}

/// `~/.config/<X>/<rest>` is a recognized adopt source: the first
/// segment under `$XDG_CONFIG_HOME` is the inferred pack name, and the
/// remainder is the in-pack path. Round-trip is the resolver's default
/// rule — pack `nvim` containing `init.lua` deploys to
/// `$XDG_CONFIG_HOME/nvim/init.lua` on `dodot up`.
#[test]
fn adopt_xdg_nested_file_lands_at_pack_root() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("placeholder", "")
        .done()
        .home_file(".config/nvim/init.lua", "-- config")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/init.lua");

    commands::adopt::adopt(
        Some("nvim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    // No prefix gymnastics: pack `nvim`'s default deploy rule (Priority
    // 4 — `$XDG/<pack>/<rel>`) lands `init.lua` back at the original
    // `~/.config/nvim/init.lua`. So the in-pack name is just `init.lua`.
    let pack_file = env.dotfiles_root.join("nvim/init.lua");
    env.assert_regular_file(&pack_file, "-- config");
    assert!(env.fs.is_symlink(&source));
    let target = env.fs.readlink(&source).unwrap();
    assert_eq!(target, pack_file);
}

/// Pack name can be omitted when the source carries pack structure
/// under `$XDG_CONFIG_HOME`: `dodot adopt ~/.config/nvim/init.lua` (no
/// `--into`) auto-detects pack `nvim` and creates it if missing.
#[test]
fn adopt_xdg_source_infers_pack_and_auto_creates() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/ghostty/config");

    commands::adopt::adopt(
        /*pack_override=*/ None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let pack_dir = env.dotfiles_root.join("ghostty");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("config"), "theme = dark");
    assert!(env.fs.is_symlink(&source));
}

/// Adopting `~/.config/<X>/` (the pack-root directory itself) expands
/// into per-child plans rather than making the directory one big
/// symlink-to-pack-root. Each top-level entry becomes a top-level pack
/// member, so `dodot up` deploys per-entry like any other pack.
#[test]
fn adopt_xdg_pack_root_directory_expands_to_children() {
    let env = TempEnvironment::builder()
        .home_file(".config/helix/config.toml", "theme = \"onedark\"")
        .home_file(".config/helix/themes/extra.toml", "fg = \"white\"")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/helix");

    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let pack_dir = env.dotfiles_root.join("helix");
    env.assert_regular_file(&pack_dir.join("config.toml"), "theme = \"onedark\"");
    env.assert_regular_file(&pack_dir.join("themes/extra.toml"), "fg = \"white\"");
    assert!(env
        .fs
        .is_symlink(&env.home.join(".config/helix/config.toml")));
    assert!(env.fs.is_symlink(&env.home.join(".config/helix/themes")));
    assert!(!env.fs.is_symlink(&source));
}

/// `~/.config/` itself is too broad to adopt as a single unit; refuse
/// explicitly so the user adopts an app subdirectory instead.
#[test]
fn adopt_xdg_root_itself_refused() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/init.lua", "-- config")
        .build();
    let ctx = make_ctx(&env);
    let source = env.config_home.clone();

    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("$XDG_CONFIG_HOME"),
        "expected XDG-root refusal, got: {msg}"
    );
}

/// Pack-root directory expansion under `--into` reroute keeps the
/// `_xdg/<X>/` prefix on each child so the round-trip survives the
/// pack-name change. Without this, expanded children would land at
/// pack root and `dodot up` would deploy them to `$XDG/<override>/...`
/// instead of the original `$XDG/<X>/...`.
#[test]
fn adopt_xdg_pack_root_expansion_with_override_uses_xdg_prefix() {
    let env = TempEnvironment::builder()
        .pack("toolbox")
        .file("placeholder", "")
        .done()
        .home_file(".config/lazygit/config.yml", "gui:\n  theme: dark")
        .home_file(".config/lazygit/themes/x.yml", "fg: white")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/lazygit");

    commands::adopt::adopt(
        Some("toolbox"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    env.assert_regular_file(
        &env.dotfiles_root.join("toolbox/_xdg/lazygit/config.yml"),
        "gui:\n  theme: dark",
    );
    env.assert_regular_file(
        &env.dotfiles_root.join("toolbox/_xdg/lazygit/themes/x.yml"),
        "fg: white",
    );
    assert!(env
        .fs
        .is_symlink(&env.home.join(".config/lazygit/config.yml")));
    assert!(env.fs.is_symlink(&env.home.join(".config/lazygit/themes")));
    assert!(!env.fs.is_symlink(&source));
}

/// `--into <pack>` for an XDG source where the override differs from
/// the inferred pack name uses `_xdg/<X>/<rest>` so round-trip via
/// Priority 2 still lands the deployed file at the original location.
#[test]
fn adopt_xdg_with_into_override_uses_xdg_prefix() {
    let env = TempEnvironment::builder()
        .pack("toolbox")
        .file("placeholder", "")
        .done()
        .home_file(".config/lazygit/config.yml", "gui:\n  theme: dark")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/lazygit/config.yml");

    commands::adopt::adopt(
        Some("toolbox"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    // Round-trip via `_xdg/lazygit/config.yml`: the `_xdg/` prefix
    // bypasses pack-namespacing so the deployed path is still
    // `~/.config/lazygit/config.yml` despite the file living in
    // pack `toolbox`.
    let pack_file = env.dotfiles_root.join("toolbox/_xdg/lazygit/config.yml");
    env.assert_regular_file(&pack_file, "gui:\n  theme: dark");
    assert!(env.fs.is_symlink(&source));
}

/// Adopting a file under `~/Library/Application Support/<X>/` infers
/// pack `<X>`, places the file at `_app/<X>/<rest>` in the pack tree,
/// and round-trips via the resolver's Priority 2c `_app/` prefix back
/// to the original AppSupport location. The `TempEnvironment` pins
/// `app_support_dir` under the temp HOME on every platform so this
/// test runs identically on Linux and macOS.
#[test]
fn adopt_app_support_source_round_trips_through_app_prefix() {
    let env = TempEnvironment::builder()
        .home_file(
            "Library/Application Support/Code/User/settings.json",
            "{\"editor.fontSize\": 14}",
        )
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Code/User/settings.json");

    commands::adopt::adopt(
        /*pack_override=*/ None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    // Pack auto-created at `<dotfiles>/Code/`. The file lives at
    // `_app/Code/User/settings.json` — the prefix is mandatory at
    // natural pack name because the default rule routes through XDG,
    // not app_support_dir.
    let pack_file = env.dotfiles_root.join("Code/_app/Code/User/settings.json");
    env.assert_regular_file(&pack_file, "{\"editor.fontSize\": 14}");

    assert!(env.fs.is_symlink(&source));

    use crate::handlers::symlink::{resolve_target_full, Resolution};
    let resolution = resolve_target_full(
        "Code",
        "_app/Code/User/settings.json",
        &Default::default(),
        env.paths.as_ref(),
    );
    match resolution {
        Resolution::Path(p) => assert_eq!(p, source),
        Resolution::Skip { reason } => panic!("expected Path, got Skip({reason})"),
    }
}

/// Adopting `~/Library/Application Support/<X>/` (the directory
/// itself) expands into per-child plans, mirroring the XDG pack-root
/// expansion. Each top-level entry under the AppSupport folder
/// becomes a top-level pack entry, prefixed with `_app/<X>/`.
#[test]
fn adopt_app_support_pack_root_directory_expands_to_children() {
    let env = TempEnvironment::builder()
        .home_file(
            "Library/Application Support/Cursor/User/settings.json",
            "{}",
        )
        .home_file(
            "Library/Application Support/Cursor/User/keybindings.json",
            "[]",
        )
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Cursor");

    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let pack_dir = env.dotfiles_root.join("Cursor");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("_app/Cursor/User/settings.json"), "{}");
    env.assert_regular_file(&pack_dir.join("_app/Cursor/User/keybindings.json"), "[]");
    assert!(env.fs.is_symlink(&env.app_support.join("Cursor/User")));
    assert!(!env.fs.is_symlink(&source));
}

/// When a user adopts an
/// AppSupport source whose folder name passes the GUI-app heuristic
/// (`Code`, uppercase), adopt emits a tip pointing at the
/// `app_aliases` ergonomic. The pack tree itself is unaffected — the
/// hint is purely advisory.
#[test]
fn adopt_app_support_emits_capitalization_hint() {
    let env = TempEnvironment::builder()
        .home_file("Library/Application Support/Code/User/settings.json", "{}")
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Code/User/settings.json");

    let result = commands::adopt::adopt(
        /*pack_override=*/ None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("app_aliases") && w.contains("Code")),
        "expected an `app_aliases` tip in warnings, got: {:?}",
        result.warnings
    );
}

/// Reverse-DNS bundle-ID folders (`com.colliderli.iina`,
/// `dev.warp.Warp-Stable`) get a much better rename suggestion when
/// the brew probe identifies a matching cask: prefer the cask
/// token (`iina`) over the awful whitespace-strip-lowercase fallback
/// (`comcolliderliiina`).
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS-only enrichment paths")]
fn adopt_app_support_reverse_dns_uses_cask_token_in_tip() {
    let env = TempEnvironment::builder()
        .home_file(
            "Library/Application Support/com.colliderli.iina/input_conf/mine.conf",
            "x",
        )
        .build();

    let runner = Arc::new(CannedRunner::new());
    runner.respond(&["brew", "list", "--cask", "--versions"], "iina 1.4.0\n", 0);
    runner.respond(
        &["brew", "info", "--json=v2", "--cask", "iina"],
        r#"{"casks": [{
            "token": "iina",
            "artifacts": [
                {"app": ["IINA.app"]},
                {"zap": [{"trash": ["~/Library/Application Support/com.colliderli.iina"]}]}
            ]
        }]}"#,
        0,
    );
    let ctx = make_ctx_with_runner(&env, runner);
    let source = env
        .app_support
        .join("com.colliderli.iina/input_conf/mine.conf");

    let result = commands::adopt::adopt(
        /*pack_override=*/ None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let tip = result
        .warnings
        .iter()
        .find(|w| w.contains("app_aliases"))
        .unwrap_or_else(|| panic!("expected an app_aliases tip, got: {:?}", result.warnings));

    assert!(
        tip.contains("renaming the pack to `iina`"),
        "expected cask-token-based rename suggestion (`iina`), got: {tip}"
    );
    assert!(
        !tip.contains("comcolliderliiina"),
        "rename suggestion fell back to lowercase mangling instead of cask token: {tip}"
    );
    assert!(
        tip.contains("matches homebrew cask"),
        "tip should credit the cask source, got: {tip}"
    );
}

/// When no installed cask matches the folder, the tip falls back to
/// the whitespace-strip-lowercase suggestion when brew has no match.
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS-only enrichment paths")]
fn adopt_app_support_falls_back_to_lowercase_when_no_cask_match() {
    // `Tinkerbell` — uppercase enough to trigger the heuristic, but
    // no real cask owns it, so the brew probe returns empty and the
    // tip falls back to the lowercase suggestion.
    let env = TempEnvironment::builder()
        .home_file("Library/Application Support/Tinkerbell/settings.json", "{}")
        .build();

    let runner = Arc::new(CannedRunner::new());
    runner.respond(&["brew", "list", "--cask", "--versions"], "", 0);
    let ctx = make_ctx_with_runner(&env, runner);
    let source = env.app_support.join("Tinkerbell/settings.json");

    let result = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let tip = result
        .warnings
        .iter()
        .find(|w| w.contains("app_aliases"))
        .unwrap_or_else(|| panic!("expected an app_aliases tip, got: {:?}", result.warnings));

    assert!(
        tip.contains("renaming the pack to `tinkerbell`"),
        "expected fallback rename suggestion, got: {tip}"
    );
    assert!(
        !tip.contains("matches homebrew cask"),
        "tip should not claim a cask match when none exists: {tip}"
    );
}

/// The advisory is suppressed when the user passed `--into <pack>`:
/// they already chose their pack name, so suggesting another one
/// would be noise. The pack used here (`Code`) only exists to satisfy
/// `--into`'s typo-guard requirement.
#[test]
fn adopt_app_support_into_override_suppresses_hint() {
    let env = TempEnvironment::builder()
        .pack("Code")
        .file("placeholder", "")
        .done()
        .home_file("Library/Application Support/Code/User/settings.json", "{}")
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Code/User/settings.json");

    let result = commands::adopt::adopt(
        Some("Code"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    assert!(
        !result.warnings.iter().any(|w| w.contains("app_aliases")),
        "expected no app_aliases tip with --into, got: {:?}",
        result.warnings
    );
}

/// Lowercase CLI-tool-style folder names (`nvim`, `helix`, …) don't
/// trigger the heuristic. An XDG adopt of a typical CLI tool stays
/// hint-free.
#[test]
fn adopt_xdg_lowercase_pack_emits_no_hint() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/init.lua", "-- nvim")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/init.lua");

    let result = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    assert!(
        !result.warnings.iter().any(|w| w.contains("app_aliases")),
        "expected no app_aliases tip for plain XDG adopt, got: {:?}",
        result.warnings
    );
}

/// Multiple sources whose inference picks different packs is refused
/// (without `--into`); the message names the conflicting candidates.
#[test]
fn adopt_disagreeing_inferred_packs_refused() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/init.lua", "-- nvim")
        .home_file(".config/helix/config.toml", "# helix")
        .build();

    let ctx = make_ctx(&env);
    let sources = vec![
        env.home.join(".config/nvim/init.lua"),
        env.home.join(".config/helix/config.toml"),
    ];

    let err = commands::adopt::adopt(None, &sources, false, false, false, None, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("different packs"),
        "expected disagreement message, got: {msg}"
    );
    assert!(msg.contains("nvim") && msg.contains("helix"));
}

/// Without `--into`, a HOME source can't infer a pack and adopt fails
/// with a hint pointing at `--into`.
#[test]
fn adopt_home_source_without_into_requires_pack() {
    let env = TempEnvironment::builder()
        .home_file(".vimrc", "set nocompatible")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("--into"), "expected '--into' hint, got: {msg}");
}

#[test]
fn adopt_already_adopted_source_is_skipped() {
    // A direct source link is unmanaged until `dodot up` upgrades it to the full chain.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "content")
        .done()
        .build();

    let source = env.home.join(".vimrc");
    let pack_file = env.dotfiles_root.join("vim/vimrc");
    env.fs.symlink(&pack_file, &source).unwrap();

    let ctx = make_ctx(&env);
    let result = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let warning = result
        .warnings
        .iter()
        .find(|w| w.contains("skipped"))
        .unwrap_or_else(|| panic!("expected a skipped warning, got: {:?}", result.warnings));
    assert!(
        warning.contains("direct symlink to pack source"),
        "expected #44 'direct symlink' wording, got: {warning}"
    );
    assert!(
        warning.contains("dodot up vim"),
        "warning should point user at `dodot up vim`, got: {warning}"
    );
    assert!(env.fs.is_symlink(&source));
    env.assert_regular_file(&pack_file, "content");
}

/// When the source is fully managed (the user
/// symlink points at dodot's data_dir), adopt reports it as already
/// managed rather than suggesting an upgrade.
#[test]
fn adopt_fully_managed_source_keeps_original_skip_message() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "content")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(Some(&["vim".into()]), &ctx).unwrap();

    let source = env.home.join(".config/vim/vimrc");
    assert!(env.fs.is_symlink(&source));

    let result = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let warning = result
        .warnings
        .iter()
        .find(|w| w.contains("skipped"))
        .unwrap_or_else(|| panic!("expected a skipped warning, got: {:?}", result.warnings));
    assert!(
        warning.contains("already managed by dodot"),
        "fully-managed case should keep original wording, got: {warning}"
    );
    assert!(
        !warning.contains("direct symlink"),
        "fully-managed case should NOT use the #44 'direct symlink' wording, got: {warning}"
    );
}

/// `dodot up` auto-replaces a pre-existing regular
/// file whose content is byte-identical to the pack source — no
/// `--force` needed, no conflict reported.
#[test]
fn up_auto_replaces_content_equivalent_pre_existing_file() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = test")
        .done()
        .home_file(".gitconfig", "[user]\n  name = test")
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();

    assert_eq!(
        result.message.as_deref(),
        Some("Packs deployed."),
        "no errors expected for content-equivalent file, got: {:?}",
        result.message
    );
    let user_path = env.home.join(".gitconfig");
    assert!(
        env.fs.is_symlink(&user_path),
        "user file should now be a symlink"
    );
    assert_eq!(
        env.fs.read_to_string(&user_path).unwrap(),
        "[user]\n  name = test"
    );
    let status = commands::status::status(None, &ctx).unwrap();
    let file = &status.packs[0].files[0];
    assert_eq!(file.status, "deployed");
}

/// `dodot up` still refuses (without `--force`) when
/// the pre-existing file's content differs from the source. The
/// auto-replace only kicks in for content-equivalent files.
#[test]
fn up_still_refuses_content_different_pre_existing_file() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = new")
        .done()
        .home_file(".gitconfig", "[user]\n  name = old")
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();

    assert_eq!(
        result.message.as_deref(),
        Some("Packs deployed with errors."),
        "different content should still conflict, got: {:?}",
        result.message
    );
    env.assert_file_contents(&env.home.join(".gitconfig"), "[user]\n  name = old");
}

/// `status` does not flag a content-equivalent
/// pre-existing file as PendingConflict (since `up` will handle it
/// without `--force`). Stays plain `pending`, no footnote.
#[test]
fn status_does_not_flag_content_equivalent_file_as_conflict() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = test")
        .done()
        .home_file(".gitconfig", "[user]\n  name = test")
        .build();

    let ctx = make_ctx(&env);
    let status = commands::status::status(None, &ctx).unwrap();
    let file = &status.packs[0].files[0];

    assert_eq!(
        file.status, "pending",
        "content-equivalent file should be plain pending (auto-replaceable), got: {}",
        file.status
    );
    assert!(
        file.note_ref.is_none(),
        "no note_ref for auto-replaceable case"
    );
    assert!(
        status.notes.is_empty(),
        "no notes for auto-replaceable case, got: {:?}",
        status.notes
    );
}

#[test]
fn adopt_relative_path_with_curdir_normalizes() {
    // `dodot adopt mypack ./.vimrc` run from HOME must not be rejected as
    // "nested" — the `.` component should normalize away so parent == HOME.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "content")
        .build();

    let prev_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&env.home).unwrap();
    let ctx = make_ctx(&env);
    let result = commands::adopt::adopt(
        Some("vim"),
        &[std::path::PathBuf::from("./.vimrc")],
        false,
        false,
        false,
        None,
        &ctx,
    );
    std::env::set_current_dir(prev_cwd).unwrap();

    result.expect("adopt should accept ./.vimrc when CWD is HOME");
    env.assert_regular_file(&env.dotfiles_root.join("vim/home.vimrc"), "content");
    assert!(env.fs.is_symlink(&env.home.join(".vimrc")));
}

#[test]
fn adopt_ignored_pack_refused() {
    let env = TempEnvironment::builder()
        .pack("disabled")
        .file("placeholder", "")
        .ignored()
        .done()
        .home_file(".vimrc", "x")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        Some("disabled"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::PackInvalid { .. }),
        "expected PackInvalid, got: {err}"
    );
}

#[test]
fn adopt_filename_matching_pack_ignore_refused() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .config("[pack]\nignore = [\"*.bak\"]")
        .done()
        .home_file(".vimrc.bak", "old")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc.bak");
    let err = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("ignore"),
        "expected ignore-pattern message, got: {msg}"
    );
}

/// Validating a run reads what the other packs claim; it does not
/// evaluate them. A pack holding a template that cannot render is no
/// reason to refuse an unrelated adoption — and, more to the point,
/// nothing is rendered on the way to finding that out. Rendering during
/// validation would write the output and its baseline into the
/// datastore (and prompt the user's secret provider) before adopt has
/// decided whether the run goes ahead at all, and a refusal or a
/// `--dry-run` would leave that behind. Same contract as `status`,
/// `docs/proposals/secrets.lex` §7.4.
#[test]
fn adopt_validation_does_not_render_another_packs_templates() {
    let env = TempEnvironment::builder()
        .pack("broken")
        .file("config.toml.tmpl", "{{ missing_var }}")
        .done()
        .pack("target")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "content")
        .build();

    let before = tree_snapshot(&env.data_dir);

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    commands::adopt::adopt(
        Some("target"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    env.assert_regular_file(&env.dotfiles_root.join("target/home.vimrc"), "content");
    assert!(env.fs.is_symlink(&source));
    assert_eq!(
        tree_snapshot(&env.data_dir),
        before,
        "validating an adoption must not write anything to the datastore"
    );
}

/// The other half of that: reading passively still reads the claims. The
/// unrenderable template deploys to the very path this adoption would
/// claim, and the run is refused on that basis — a template's deployed
/// name comes from its filename, so passive planning surfaces the claim
/// without ever evaluating the content.
#[test]
fn adopt_deploy_conflict_refused_against_an_unrendered_template() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("bashrc.tmpl", "{{ missing_var }}")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected the cross-pack conflict to surface, got: {err}"
    );
    env.assert_regular_file(&source, "new");
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
}

/// An `externals.toml` declares each target it claims *inside the
/// file*, so validation has to read the file to learn them. When that
/// manifest is a template dodot has never rendered, there is nothing to
/// read: the entry surfaces as a placeholder, the handler emits no
/// `Fetch`, and a clean conflict report would be a report about claims
/// nobody looked at. Adopt refuses instead of publishing into the
/// collision — the same refusal it makes for a pack it cannot scan.
#[test]
fn adopt_refused_while_an_externals_manifest_is_still_unrendered() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file(
            "externals.toml.tmpl",
            r#"
            [bashrc]
            type   = "file"
            url    = "https://example.com/bashrc"
            target = "~/{{ name }}"
            sha256 = "abc"
        "#,
        )
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let before = tree_snapshot(&env.data_dir);
    // `--no-provision` is an `up`-only flag; adopt always plans with
    // the code-execution handlers on, and `externals` is one of them.
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    match &err {
        crate::DodotError::ConflictCheckIncomplete { unresolved } => {
            assert_eq!(unresolved.len(), 1, "expected one unresolved claim: {err}");
            assert_eq!(unresolved[0].pack, "unix");
            assert_eq!(unresolved[0].source, "externals.toml.tmpl");
        }
        other => panic!("expected ConflictCheckIncomplete, got: {other}"),
    }
    let msg = format!("{err}");
    assert!(
        msg.contains("externals.toml.tmpl") && msg.contains("dodot up"),
        "the refusal must name the file to render and how to render it, got: {msg}"
    );

    env.assert_regular_file(&source, "new");
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
    assert_eq!(
        tree_snapshot(&env.data_dir),
        before,
        "refusing must not render the template it refused over"
    );
}

/// Render `pack`'s templates the way a `dodot up` would, leaving the
/// baseline a later passive plan reads. The registry is the one
/// planning builds for that pack, so the baseline's `context_hash`
/// matches what the adopt run recomputes.
///
/// Only the preprocessing half of `up` runs: an `externals.toml` whose
/// targets are fetched would need the network, and every question here
/// is about the manifest's contents rather than what fetching it does.
fn render_pack_templates(
    env: &TempEnvironment,
    ctx: &ExecutionContext,
    pack_name: &str,
    file: &str,
) {
    let pack_path = env.dotfiles_root.join(pack_name);
    let pack_config = ctx.config_manager.config_for_pack(&pack_path).unwrap();
    let root_config = ctx.config_manager.root_config().unwrap();
    let (registry, _secrets) = crate::preprocessing::default_registry(
        &pack_config.preprocessor,
        &root_config.secret,
        ctx.paths.as_ref(),
        ctx.command_runner.clone(),
    )
    .unwrap();

    let pack =
        crate::packs::Pack::new(pack_name.to_string(), pack_path.clone(), Default::default());
    crate::preprocessing::pipeline::preprocess_pack(
        vec![crate::rules::PackEntry {
            relative_path: file.into(),
            absolute_path: pack_path.join(file),
            is_dir: false,
            gate_failure: None,
        }],
        &registry,
        &pack,
        ctx.fs.as_ref(),
        ctx.datastore.as_ref(),
        ctx.paths.as_ref(),
        crate::preprocessing::PreprocessMode::Active,
        false,
    )
    .expect("rendering the pack's template must succeed");
}

/// An externals manifest whose single entry, `name`, fetches to
/// `target`. Written as a template, though most of these tests have
/// nothing to substitute — what they turn on is the edit between one
/// render and the next, not the templating.
fn externals_manifest(name: &str, target: &str) -> String {
    format!(
        r#"
        [{name}]
        type   = "file"
        url    = "https://example.com/{name}"
        target = "{target}"
        sha256 = "abc"
    "#
    )
}

/// A rendered manifest answers for the pack, so adoption proceeds: the
/// refusal below has to come from the edit, not from a templated
/// `externals.toml` being present at all.
#[test]
fn adopt_proceeds_against_an_externals_manifest_rendered_from_the_current_template() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file(
            "externals.toml.tmpl",
            &externals_manifest("other", "~/.other"),
        )
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    render_pack_templates(&env, &ctx, "unix", "externals.toml.tmpl");

    let source = env.home.join(".bashrc");
    commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .expect("a manifest rendered from the current template claims ~/.other, not ~/.bashrc");

    env.assert_regular_file(&env.dotfiles_root.join("work/bashrc"), "new");
    assert!(env.fs.is_symlink(&source));
}

/// The baseline records what the template said at the last `dodot up`.
/// Editing the template to claim the very path being adopted makes that
/// record wrong about what the pack now deploys — and it is a record
/// only `dodot up` may replace, since re-rendering here would resolve
/// the manifest's secrets and write its output for a run the user has
/// not agreed to. Adopt refuses on the gap instead of publishing into
/// the collision the next `up` would reject.
#[test]
fn adopt_refused_while_an_externals_manifest_is_rendered_from_an_older_template() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file(
            "externals.toml.tmpl",
            &externals_manifest("other", "~/.other"),
        )
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    render_pack_templates(&env, &ctx, "unix", "externals.toml.tmpl");

    // The edit `dodot up` has not seen: this manifest now claims the
    // path about to be adopted.
    env.fs
        .write_file(
            &env.dotfiles_root.join("unix/externals.toml.tmpl"),
            externals_manifest("bashrc", "~/.bashrc").as_bytes(),
        )
        .unwrap();

    let before = tree_snapshot(&env.data_dir);
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    match &err {
        crate::DodotError::ConflictCheckIncomplete { unresolved } => {
            assert_eq!(unresolved.len(), 1, "expected one unresolved claim: {err}");
            assert_eq!(unresolved[0].pack, "unix");
            assert_eq!(unresolved[0].source, "externals.toml.tmpl");
        }
        other => panic!("expected ConflictCheckIncomplete, got: {other}"),
    }

    env.assert_regular_file(&source, "new");
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
    assert_eq!(
        tree_snapshot(&env.data_dir),
        before,
        "refusing must not re-render the template it refused over"
    );
}

/// The same gap reached without touching the template: a rendered
/// target can come from `vars`, and changing one changes what the next
/// `up` fetches. Nothing on disk differs, which is exactly why the
/// baseline's context hash has to be the thing that answers.
#[test]
fn adopt_refused_while_an_externals_manifest_was_rendered_with_other_vars() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file(
            "externals.toml.tmpl",
            &externals_manifest("other", "~/{{ target_name }}"),
        )
        .config("[preprocessor.template.vars]\ntarget_name = \".other\"\n")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    render_pack_templates(&env, &ctx, "unix", "externals.toml.tmpl");

    // Same template bytes, different value for what they interpolate.
    env.fs
        .write_file(
            &env.dotfiles_root.join("unix/.dodot.toml"),
            b"[preprocessor.template.vars]\ntarget_name = \".bashrc\"\n",
        )
        .unwrap();

    // A fresh context: `ConfigManager` caches per pack path, so the
    // one that rendered above would keep serving the old vars.
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::ConflictCheckIncomplete { .. }),
        "a render made with vars this run no longer has cannot answer for the pack, got: {err}"
    );
    env.assert_regular_file(&source, "new");
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
}

/// `--force` does not bypass an incomplete analysis either. It overrides
/// what dodot knows to be in the way; it cannot override what dodot has
/// not looked at.
#[test]
fn unrendered_externals_refusal_not_bypassed_by_force() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file(
            "externals.toml.tmpl",
            r#"
            [bashrc]
            type   = "file"
            url    = "https://example.com/bashrc"
            target = "~/{{ name }}"
            sha256 = "abc"
        "#,
        )
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        true,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::ConflictCheckIncomplete { .. }),
        "--force must not bypass an analysis dodot could not complete, got: {err}"
    );
}

/// The claim itself is real, and a readable manifest proves it: the same
/// `externals.toml` as plain content collides with the adoption and the
/// run is refused on the collision, not on incompleteness.
#[test]
fn adopt_deploy_conflict_refused_against_an_externals_manifest() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file(
            "externals.toml",
            r#"
            [bashrc]
            type   = "file"
            url    = "https://example.com/bashrc"
            target = "~/.bashrc"
            sha256 = "abc"
        "#,
        )
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected the externals target to collide with the adoption, got: {err}"
    );
    env.assert_regular_file(&source, "new");
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
}

/// And the refusal is narrow: once the template has been rendered once,
/// its baseline carries the rendered manifest and passive planning reads
/// the claims straight out of it. Adopt then reports the collision it
/// was previously unable to see — no second refusal, no re-render.
#[test]
fn adopt_reads_externals_claims_from_the_cached_baseline() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file(
            "externals.toml.tmpl",
            r#"
            [bashrc]
            type   = "file"
            url    = "https://example.com/bashrc"
            target = "~/{{ name }}"
            sha256 = "abc"
        "#,
        )
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    // Stand in for the `dodot up` that would have written this.
    let rendered = br#"
            [bashrc]
            type   = "file"
            url    = "https://example.com/bashrc"
            target = "~/.bashrc"
            sha256 = "abc"
        "#;
    let source_path = env.dotfiles_root.join("unix/externals.toml.tmpl");
    let source_bytes = std::fs::read(&source_path).unwrap();
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    crate::preprocessing::baseline::Baseline::build(
        &source_path,
        rendered,
        &source_bytes,
        None,
        None,
    )
    .write(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        "unix",
        "preprocessed",
        &crate::preprocessing::baseline::cache_filename_for(std::path::Path::new("externals.toml")),
    )
    .unwrap();

    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "a baselined manifest must be read, not refused as unresolved, got: {err}"
    );
    env.assert_regular_file(&source, "new");
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
}

/// Every path under `root`, sorted — what a test compares before and
/// after to say a directory was left alone. Missing root reads as empty,
/// which is the state a run that wrote nothing leaves it in; any other
/// read failure is the test's own bug and panics rather than reading as
/// an empty directory that compares equal to whatever was there.
fn tree_snapshot(root: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, prefix: &str, out: &mut Vec<String>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => panic!("reading {}: {e}", dir.display()),
        };
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("listing {}: {e}", dir.display()));
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.path().is_dir() && !entry.path().is_symlink() {
                walk(&entry.path(), &format!("{name}/"), out);
            }
            out.push(name);
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort();
    out
}

#[test]
fn adopt_deploy_conflict_refused() {
    // Two packs would both end up claiming ~/.bashrc after adoption.
    // Using `bashrc` because it's in `force_home` — different packs both
    // deploy it to ~/.bashrc, producing a real cross-pack conflict.
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("bashrc", "existing")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected CrossPackConflict, got: {err}"
    );

    env.assert_regular_file(&source, "new");
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
}

#[test]
fn adopt_deploy_conflict_not_bypassed_by_force() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("bashrc", "existing")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
        std::slice::from_ref(&source),
        true, // --force should NOT bypass deploy conflicts
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "--force must not bypass deploy conflicts, got: {err}"
    );
}

#[test]
fn adopt_dry_run_makes_no_changes() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "content")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let result = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        true, // dry-run
        None,
        &ctx,
    )
    .unwrap();
    assert!(result.dry_run);

    env.assert_regular_file(&source, "content");
    assert!(!env.fs.is_symlink(&source));
    env.assert_not_exists(&env.dotfiles_root.join("vim/home.vimrc"));
}

#[test]
fn adopt_no_follow_keeps_source_symlink_as_symlink() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file("real_vimrc", "real content")
        .build();

    let real = env.home.join("real_vimrc");
    let source = env.home.join(".vimrc");
    env.fs.symlink(&real, &source).unwrap();

    let ctx = make_ctx(&env);
    commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        true, // --no-follow
        false,
        None,
        &ctx,
    )
    .unwrap();

    let pack_copy = env.dotfiles_root.join("vim/home.vimrc");
    assert!(
        env.fs.is_symlink(&pack_copy),
        "--no-follow should preserve source symlink as a symlink in the pack"
    );
    assert!(env.fs.is_symlink(&source));
}

#[cfg(unix)]
#[test]
fn adopt_force_preserves_old_content_when_copy_fails() {
    // With --force, the old destination must remain intact if the copy of
    // the new source fails.
    use std::os::unix::fs::PermissionsExt;

    // Skip when DAC permissions don't block this process (e.g. running as
    // root in a container/sandbox — CAP_DAC_READ_SEARCH bypasses chmod 000).
    // The test fundamentally requires read() to fail on the source file.
    let probe = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(probe.path(), b"x").unwrap();
    std::fs::set_permissions(probe.path(), std::fs::Permissions::from_mode(0o000)).unwrap();
    let can_be_blocked_by_chmod = std::fs::read(probe.path()).is_err();
    let _ = std::fs::set_permissions(probe.path(), std::fs::Permissions::from_mode(0o644));
    if !can_be_blocked_by_chmod {
        eprintln!(
            "skipping adopt_force_preserves_old_content_when_copy_fails: \
             process bypasses DAC permissions (running as root?)"
        );
        return;
    }

    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .build();

    let source = env.home.join(".vimrc");
    // chmod 000 makes the file unreadable, so the copy phase fails at
    // read-time without tripping preflight (which uses lstat only).
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o000)).unwrap();

    let ctx = make_ctx(&env);
    let result = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        None,
        &ctx,
    );

    // Restore perms so drop-cleanup works regardless of assertion outcome.
    let _ = std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644));

    assert!(
        result.is_err(),
        "adopt should fail when the source is unreadable"
    );
    env.assert_regular_file(&env.dotfiles_root.join("vim/home.vimrc"), "OLD");
    env.assert_regular_file(&source, "NEW");
    let leftover = env.fs.read_dir(&env.dotfiles_root.join("vim")).unwrap();
    for entry in leftover {
        assert!(
            !entry.name.contains("dodot-adopt-stage"),
            "stage file leaked into pack: {}",
            entry.name
        );
    }
}

#[test]
fn adopt_no_follow_on_dangling_symlink_succeeds() {
    // A dangling symlink under --no-follow: readability check must inspect
    // the link itself (lstat), not try to follow it into a non-existent
    // target.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .build();

    let source = env.home.join(".dangling");
    env.fs
        .symlink(std::path::Path::new("/does/not/exist"), &source)
        .unwrap();

    let ctx = make_ctx(&env);
    commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        true, // --no-follow
        false,
        None,
        &ctx,
    )
    .expect("adopt with --no-follow on a dangling symlink should succeed");

    let pack_copy = env.dotfiles_root.join("vim/home.dangling");
    assert!(env.fs.is_symlink(&pack_copy));
    let target = env.fs.readlink(&pack_copy).unwrap();
    assert_eq!(target, std::path::PathBuf::from("/does/not/exist"));
}

#[test]
fn adopt_nonexistent_source_errors() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".does-not-exist");
    let err = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    assert!(matches!(err, crate::DodotError::Fs { .. }), "got: {err}");
}

#[test]
fn adopt_empty_sources_errors() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .build();
    let ctx = make_ctx(&env);
    let err =
        commands::adopt::adopt(Some("vim"), &[], false, false, false, None, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("no files"), "got: {msg}");
}

// ── adopt: pack not found hint ─────────────────────────────

#[test]
fn adopt_nonexistent_pack_returns_pack_not_found() {
    let env = TempEnvironment::builder()
        .home_file(".vimrc", "set nocompatible")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        Some("newpack"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::PackNotFound { .. }),
        "expected PackNotFound, got: {err}"
    );
}

// ── Safe new-pack adoption path ──────────────────────────────────
//
// `docs/proposals/adopt-safety.lex` §5 orders adopt as plan → prepare
// → validate → publish → replace sources → finish, and §2.3 is what the
// order buys: nothing reaches a final pack path until every check that
// can refuse the run has passed. These tests pin that for an inferred
// pack that does not exist yet — the case where "wrote nothing" and
// "created no pack" are the same claim.

/// Names of the run-scoped preparation directories currently sitting in
/// the dotfiles root. Every adopt path is supposed to leave this empty,
/// on the way out of a refusal and a success alike.
fn preparation_dirs(env: &TempEnvironment) -> Vec<String> {
    env.list_dir_names(&env.dotfiles_root)
        .into_iter()
        .filter(|n| n.starts_with(".dodot-adopt-"))
        .collect()
}

fn pack_names(env: &TempEnvironment) -> Vec<String> {
    let mgr = crate::config::ConfigManager::new(&env.dotfiles_root).unwrap();
    let ignore = mgr.root_config().unwrap().pack.ignore;
    crate::packs::discover_packs(env.fs.as_ref(), &env.dotfiles_root, &ignore)
        .unwrap()
        .into_iter()
        .map(|p| p.display_name)
        .collect()
}

/// Two entries where one contains the other have no publication order
/// that comes out right (§5.1), so Plan rejects the pair — before the
/// inferred pack or any preparation directory exists.
#[test]
fn adopt_overlapping_sources_refused_during_plan() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/lua/init.lua", "-- init")
        .build();

    let ctx = make_ctx(&env);
    let dir = env.home.join(".config/nvim/lua");
    let file = env.home.join(".config/nvim/lua/init.lua");

    let err = commands::adopt::adopt(
        None,
        &[dir.clone(), file.clone()],
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("contains") && msg.contains("adopt the outer one alone"),
        "expected an overlap refusal naming both entries, got: {msg}"
    );

    env.assert_not_exists(&env.dotfiles_root.join("nvim"));
    assert!(
        preparation_dirs(&env).is_empty(),
        "a Plan refusal must leave no preparation directory"
    );
    assert!(
        !env.fs.is_symlink(&dir),
        "source directory must be untouched"
    );
    env.assert_regular_file(&file, "-- init");
}

/// The same refusal when one side of the overlap arrives through
/// pack-root directory expansion rather than the command line (§5.1).
#[test]
fn adopt_overlap_through_directory_expansion_refused_during_plan() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/lua/init.lua", "-- init")
        .build();

    let ctx = make_ctx(&env);
    let pack_root = env.home.join(".config/nvim");
    let file = env.home.join(".config/nvim/lua/init.lua");

    let err = commands::adopt::adopt(
        None,
        &[pack_root, file.clone()],
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("contains"),
        "expected an overlap refusal, got: {err}"
    );
    env.assert_not_exists(&env.dotfiles_root.join("nvim"));
    assert!(preparation_dirs(&env).is_empty());
    env.assert_regular_file(&file, "-- init");
}

/// Two sources with no containment between them can still nest once
/// inference has placed them, and that refusal says what actually went
/// wrong. Here `~/.config/other/lua` lands at `_xdg/other/lua` because
/// `--into nvim` reroutes it, and `~/.config/nvim/_xdg/other` is already
/// written in that encoding, so one pack path sits inside the other.
/// Neither source contains the other, and telling the user to "adopt the
/// outer one alone" would name a directory that carries nothing of the
/// other source.
#[test]
fn adopt_overlapping_in_pack_paths_refused_with_their_pack_paths() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("placeholder", "")
        .done()
        .home_file(".config/other/lua/init.lua", "-- other")
        .home_file(".config/nvim/_xdg/other/keep", "-- kept")
        .build();

    let ctx = make_ctx(&env);
    let rerouted = env.home.join(".config/other/lua");
    let already_encoded = env.home.join(".config/nvim/_xdg/other");

    let err = commands::adopt::adopt(
        Some("nvim"),
        &[rerouted.clone(), already_encoded.clone()],
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("_xdg/other/lua") && msg.contains("would land at"),
        "expected a refusal naming both pack paths, got: {msg}"
    );
    assert!(
        !msg.contains("contains"),
        "neither source contains the other, so the refusal must not say so: {msg}"
    );
    // And Plan refused before writing: both sources are as they were.
    env.assert_regular_file(&rerouted.join("init.lua"), "-- other");
    env.assert_regular_file(&already_encoded.join("keep"), "-- kept");
    assert!(preparation_dirs(&env).is_empty());
}

/// Prospective content is copied beneath a `.dodot-adopt-` directory in
/// the dotfiles root, and a pack scan running at that moment reports the
/// user's packs rather than the half-copied one (§5.2).
#[test]
fn adopt_new_pack_stages_under_a_preparation_dir_no_pack_scan_reads() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    // What the copy step saw: where each byte landed, and which packs
    // were discoverable while it was landing there.
    type CopyObservations = Arc<std::sync::Mutex<Vec<(std::path::PathBuf, Vec<String>)>>>;
    let observed: CopyObservations = Arc::new(std::sync::Mutex::new(Vec::new()));

    let root = env.dotfiles_root.clone();
    let probe_fs = env.fs.clone() as Arc<dyn Fs>;
    let sink = observed.clone();
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| {
        if let super::support::FsOp::CopyFile { to, .. } = op {
            let visible = crate::packs::discover_packs(probe_fs.as_ref(), &root, &[])
                .unwrap()
                .into_iter()
                .map(|p| p.name)
                .collect();
            sink.lock().unwrap().push((to.to_path_buf(), visible));
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/ghostty/config");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1, "expected one copy into preparation");
    let (dest, visible_packs) = &observed[0];
    let staged = dest.strip_prefix(&env.dotfiles_root).unwrap();
    let prep_dir = staged.components().next().unwrap().as_os_str();
    assert!(
        prep_dir.to_string_lossy().starts_with(".dodot-adopt-"),
        "prospective content must land under a .dodot-adopt- directory, got: {}",
        staged.display()
    );
    assert_eq!(
        staged,
        std::path::Path::new(prep_dir).join("ghostty/config"),
        "preparation lays content out at the in-pack path the plan assigned"
    );
    assert!(
        visible_packs.is_empty(),
        "a pack scan during preparation must read neither the preparation \
         directory nor the pack being prepared, saw: {visible_packs:?}"
    );

    // And the run still finished: the pack published and is discoverable.
    assert_eq!(pack_names(&env), vec!["ghostty".to_string()]);
    assert!(preparation_dirs(&env).is_empty());
}

/// A copy failure removes the preparation directory and leaves the
/// inferred pack absent and every source unchanged (§6).
#[test]
fn adopt_new_pack_copy_failure_removes_preparation_and_creates_no_pack() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), |op| match op {
        super::support::FsOp::CopyFile { from, .. } => Err(crate::DodotError::Other(format!(
            "injected copy failure: {}",
            from.display()
        ))),
        _ => Ok(()),
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/ghostty/config");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("injected copy failure"),
        "expected the injected failure to surface, got: {err}"
    );
    env.assert_not_exists(&env.dotfiles_root.join("ghostty"));
    assert!(
        preparation_dirs(&env).is_empty(),
        "a copy failure must remove the preparation directory"
    );
    env.assert_regular_file(&source, "theme = dark");
}

/// The cross-pack deployment conflict analysis evaluates the prospective
/// pack out of the preparation directory (§5.3). Without that, nothing
/// would claim `~/.config/ghostty/config` twice, because the prospective
/// entry is not at a final pack path when the analysis runs.
#[test]
fn adopt_new_pack_deploy_conflict_refused_leaves_no_pack() {
    let env = TempEnvironment::builder()
        .pack("other")
        .file("_xdg/ghostty/config", "from the other pack")
        .done()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/ghostty/config");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected CrossPackConflict against the prospective pack, got: {err}"
    );
    env.assert_not_exists(&env.dotfiles_root.join("ghostty"));
    assert!(preparation_dirs(&env).is_empty());
    env.assert_regular_file(&source, "theme = dark");
    env.assert_file_contents(
        &env.dotfiles_root.join("other/_xdg/ghostty/config"),
        "from the other pack",
    );
}

/// `--dry-run` runs the same plan, preparation and validation as a real
/// invocation, reports the plan, and changes no final path (§5.3).
#[test]
fn adopt_new_pack_dry_run_reports_the_plan_and_writes_nothing() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/ghostty/config");
    let result = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        true, // --dry-run
        None,
        &ctx,
    )
    .unwrap();

    assert!(result.dry_run);
    let pack = result
        .packs
        .iter()
        .find(|p| p.name == "ghostty")
        .expect("dry-run reports the plan for the pack it would publish");
    assert!(
        pack.files.iter().any(|f| f.name == "config"),
        "expected the planned in-pack entry, got: {:?}",
        pack.files.iter().map(|f| &f.name).collect::<Vec<_>>()
    );

    env.assert_not_exists(&env.dotfiles_root.join("ghostty"));
    assert!(
        preparation_dirs(&env).is_empty(),
        "--dry-run must remove its preparation directory"
    );
    env.assert_regular_file(&source, "theme = dark");
    assert!(!env.fs.is_symlink(&source));

    // The same invocation without --dry-run then does what was reported.
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();
    env.assert_file_contents(&env.dotfiles_root.join("ghostty/config"), "theme = dark");
}

/// A new pack is published with one rename, and a failure at that rename
/// leaves no pack at all (§5.4).
#[test]
fn adopt_new_pack_publication_failure_leaves_no_pack() {
    let env = TempEnvironment::builder()
        .home_file(".config/helix/config.toml", "theme = \"onedark\"")
        .home_file(".config/helix/themes/extra.toml", "fg = \"white\"")
        .build();

    let pack_path = env.dotfiles_root.join("helix");
    let target = pack_path.clone();
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| match op {
        super::support::FsOp::RenameNoReplace { to, .. } if to == target => {
            Err(crate::DodotError::Other("injected publish failure".into()))
        }
        _ => Ok(()),
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/helix");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("injected publish failure"),
        "expected the injected failure to surface, got: {err}"
    );
    env.assert_not_exists(&pack_path);
    assert!(preparation_dirs(&env).is_empty());
    env.assert_regular_file(
        &env.home.join(".config/helix/config.toml"),
        "theme = \"onedark\"",
    );
    env.assert_regular_file(
        &env.home.join(".config/helix/themes/extra.toml"),
        "fg = \"white\"",
    );
}

/// The successful side of the same rename: one rename onto the pack
/// path, the whole pack visible after it, sources replaced with links to
/// the published entries, and no marker adopt wrote left in the pack
/// (§5.2, §5.4, §5.5).
#[test]
fn adopt_new_pack_publishes_with_one_rename_and_replaces_sources() {
    let env = TempEnvironment::builder()
        .home_file(".config/helix/config.toml", "theme = \"onedark\"")
        .home_file(".config/helix/themes/extra.toml", "fg = \"white\"")
        .build();

    let pack_path = env.dotfiles_root.join("helix");
    // Every rename that lands on the pack path, by what it moved there.
    let publishes: Arc<std::sync::Mutex<Vec<std::path::PathBuf>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = publishes.clone();
    let target = pack_path.clone();
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| {
        if let super::support::FsOp::RenameNoReplace { from, to } = op {
            if to == target {
                sink.lock().unwrap().push(from.to_path_buf());
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/helix");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let publishes = publishes.lock().unwrap();
    assert_eq!(
        publishes.len(),
        1,
        "a new pack is published with exactly one rename, got: {publishes:?}"
    );
    let staged = publishes[0].strip_prefix(&env.dotfiles_root).unwrap();
    assert!(
        staged
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .starts_with(".dodot-adopt-"),
        "the rename moves the prepared tree onto the pack path, got: {}",
        staged.display()
    );
    assert_eq!(staged.file_name().unwrap(), "helix");
    env.assert_file_contents(&pack_path.join("config.toml"), "theme = \"onedark\"");
    env.assert_file_contents(&pack_path.join("themes/extra.toml"), "fg = \"white\"");

    // Discoverable immediately, and carrying nothing adopt wrote.
    assert_eq!(pack_names(&env), vec!["helix".to_string()]);
    env.assert_not_exists(&pack_path.join(".dodotignore"));
    let mut published = env.list_dir_names(&pack_path);
    published.sort();
    assert_eq!(published, vec!["config.toml", "themes"]);

    // Sources now link to the published entries.
    env.assert_symlink(
        &env.home.join(".config/helix/config.toml"),
        &pack_path.join("config.toml"),
    );
    env.assert_symlink(
        &env.home.join(".config/helix/themes"),
        &pack_path.join("themes"),
    );
    assert!(preparation_dirs(&env).is_empty());
}

/// If the pack path comes into existence after planning, publication
/// refuses rather than merging into it (§5.4). Nothing is published yet,
/// so refusing costs the run nothing — and the directory that appeared
/// is left exactly as it was found.
#[test]
fn adopt_new_pack_refuses_when_the_pack_path_appears_after_planning() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let pack_path = env.dotfiles_root.join("ghostty");
    let racer = pack_path.clone();
    // The copy into preparation is the window: planning has finished and
    // publication has not started.
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| {
        if let super::support::FsOp::CopyFile { .. } = op {
            std::fs::create_dir_all(racer.join("nested")).unwrap();
            std::fs::write(racer.join("nested/other.toml"), b"someone else's").unwrap();
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/ghostty/config");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("appeared") && msg.contains("refusing to merge into it"),
        "expected a publication refusal naming the race, got: {msg}"
    );
    // The pack that appeared kept its own content — adopt merged nothing.
    env.assert_file_contents(&pack_path.join("nested/other.toml"), "someone else's");
    assert_eq!(env.list_dir_names(&pack_path), vec!["nested".to_string()]);
    assert!(preparation_dirs(&env).is_empty());
    env.assert_regular_file(&source, "theme = dark");
}

/// The tighter half of the same guarantee: the pack path appears at the
/// last possible instant — after publication has begun and before the
/// kernel moves the tree. A publication that tested the path and then
/// renamed would replace the newcomer here; a no-replace rename refuses
/// it, so what the other writer put there survives untouched.
#[test]
fn adopt_new_pack_refuses_when_the_pack_path_appears_at_the_instant_of_publication() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let pack_path = env.dotfiles_root.join("ghostty");
    let racer = pack_path.clone();
    // An empty directory, which is what a plain `rename` replaces
    // without a word — the case a check-then-rename publication cannot
    // see coming.
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| {
        if let super::support::FsOp::RenameNoReplace { to, .. } = op {
            if to == racer {
                std::fs::create_dir(&racer).unwrap();
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/ghostty/config");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("appeared") && msg.contains("refusing to merge into it"),
        "expected a publication refusal naming the race, got: {msg}"
    );
    // The newcomer is exactly as the other writer left it, and adopt
    // published nothing into it.
    assert!(env.fs.is_dir(&pack_path));
    assert!(
        env.list_dir_names(&pack_path).is_empty(),
        "the directory that appeared must be left as it was found, got: {:?}",
        env.list_dir_names(&pack_path)
    );
    assert!(preparation_dirs(&env).is_empty());
    env.assert_regular_file(&source, "theme = dark");
}

/// The preparation directory is claimed, not merely named: a name
/// already taken — a concurrent run's, or a leftover from a killed one —
/// sends the run to the next name instead of filling a directory
/// someone else may also be filling, validating, or deleting.
#[test]
fn adopt_new_pack_claims_another_name_when_the_preparation_name_is_taken() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    // Squat the first name adopt tries, right before it tries it, and
    // leave content in it so a run that adopted the directory instead of
    // refusing it would be visible.
    let squatted: Arc<std::sync::Mutex<Option<std::path::PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let sink = squatted.clone();
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| {
        if let super::support::FsOp::MkdirExclusive { path } = op {
            let mut first = sink.lock().unwrap();
            if first.is_none() {
                std::fs::create_dir(path).unwrap();
                std::fs::write(path.join("squatter"), b"another run's").unwrap();
                *first = Some(path.to_path_buf());
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/ghostty/config");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    // The run published its own pack from a directory of its own.
    assert_eq!(pack_names(&env), vec!["ghostty".to_string()]);
    env.assert_file_contents(&env.dotfiles_root.join("ghostty/config"), "theme = dark");

    // And never touched the one it found taken.
    let squatted = squatted.lock().unwrap().clone().expect("a name was taken");
    env.assert_file_contents(&squatted.join("squatter"), "another run's");
    assert_eq!(
        preparation_dirs(&env),
        vec![squatted.file_name().unwrap().to_string_lossy().to_string()],
        "only the taken directory survives; the run removed its own"
    );
}

/// Failing to create the pack directory inside a freshly claimed
/// preparation root removes the root, so the promise that a preparation
/// failure leaves nothing behind covers the allocation step too (§6).
#[test]
fn adopt_new_pack_preparation_child_failure_removes_the_claimed_root() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), |op| match op {
        // The pack directory inside the preparation root — the one
        // `mkdir_all` that lands under a `.dodot-adopt-` name.
        super::support::FsOp::MkdirAll { path }
            if path
                .parent()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n.to_string_lossy().starts_with(".dodot-adopt-")) =>
        {
            Err(crate::DodotError::Other(
                "injected preparation failure".into(),
            ))
        }
        _ => Ok(()),
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/ghostty/config");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("injected preparation failure"),
        "expected the injected failure to surface, got: {err}"
    );
    assert!(
        preparation_dirs(&env).is_empty(),
        "the root claimed for this run must not outlive the failure, got: {:?}",
        preparation_dirs(&env)
    );
    env.assert_not_exists(&env.dotfiles_root.join("ghostty"));
    env.assert_regular_file(&source, "theme = dark");
}

/// A leftover preparation directory — what a process killed mid-publish
/// leaves behind — is ignored by pack discovery and by the next adopt
/// run, which neither publishes from it nor deletes it (§5.4).
#[test]
fn adopt_ignores_but_does_not_touch_a_leftover_preparation_directory() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let leftover = env.dotfiles_root.join(".dodot-adopt-deadbeef");
    std::fs::create_dir_all(leftover.join("nvim")).unwrap();
    std::fs::write(leftover.join("nvim/init.lua"), b"-- half-copied").unwrap();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/ghostty/config");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    // The run published its own pack and read nothing from the leftover.
    assert_eq!(pack_names(&env), vec!["ghostty".to_string()]);
    env.assert_not_exists(&env.dotfiles_root.join("nvim"));
    // And left the leftover alone, for the user to inspect and remove.
    env.assert_file_contents(&leftover.join("nvim/init.lua"), "-- half-copied");
}

/// A classification refusal (§5.1) is a Plan refusal like any other: the
/// inferred pack the sources named is not created on the way to finding
/// out that adopt will not adopt them.
#[test]
fn adopt_classification_refusal_creates_no_inferred_pack() {
    let env = TempEnvironment::builder()
        .home_file(".config/junk/.DS_Store", "noise")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/junk/.DS_Store");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("ignore"),
        "expected an ignore-pattern refusal, got: {err}"
    );
    env.assert_not_exists(&env.dotfiles_root.join("junk"));
    assert!(preparation_dirs(&env).is_empty());
    env.assert_regular_file(&source, "noise");
}

// ── Recoverable existing-pack publication ────────────────────────
//
// `docs/proposals/adopt-safety.lex` §5.4 splits publication in two. A
// new pack is one rename and is atomic; a pack that already exists takes
// a sequence of renames and is *recoverable* instead — adopt undoes its
// own renames on failure and says what it put back. These tests pin the
// second half: what the pack holds after a refusal, after a failure at
// entry N, and after a `--force` run that never got to commit.

/// The recorded content of one path in a [`file_snapshot`]: raw bytes
/// for a regular file, the link target for a symlink.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Snapshot {
    File(Vec<u8>),
    Link(std::path::PathBuf),
}

impl Snapshot {
    /// The expected content of a text file, for an assertion written
    /// against a literal.
    fn file(contents: &str) -> Snapshot {
        Snapshot::File(contents.as_bytes().to_vec())
    }
}

/// Every file under `root` as `relative path => content`, sorted. Two
/// of these taken either side of a refused run is how a test says the
/// pack was left byte-identical.
///
/// Bytes rather than text, and every read is unwrapped: reading each
/// file as a lossy or defaulted string would let a changed file compare
/// equal to the one before it — non-UTF-8 content reads the same
/// whatever the bytes are, and a read that fails reads as empty — which
/// is precisely the change these tests exist to catch. An absent `root`
/// snapshots as empty; anything present has to be readable.
fn file_snapshot(root: &std::path::Path) -> Vec<(String, Snapshot)> {
    fn walk(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, Snapshot)>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => panic!("reading {}: {e}", dir.display()),
        };
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("listing {}: {e}", dir.display()));
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            let path = entry.path();
            if path.is_symlink() {
                let target = std::fs::read_link(&path)
                    .unwrap_or_else(|e| panic!("reading link {}: {e}", path.display()));
                out.push((name, Snapshot::Link(target)));
            } else if path.is_dir() {
                walk(&path, &format!("{name}/"), out);
            } else {
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
                out.push((name, Snapshot::File(bytes)));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort();
    out
}

/// The `.displaced` subtree of whichever preparation directory is
/// currently in the dotfiles root, as `relative path => content`.
/// Empty when no run is in flight or nothing has been displaced.
fn displaced_snapshot(env: &TempEnvironment) -> Vec<(String, Snapshot)> {
    preparation_dirs(env)
        .into_iter()
        .flat_map(|name| file_snapshot(&env.dotfiles_root.join(name).join(".displaced")))
        .collect()
}

/// Validation reads the pack's current entries composed with the
/// prepared ones (§5.3), so it catches a conflict the *prepared* entry
/// causes — and it catches it without a final pack path having been
/// written, so the refusal leaves the pack byte-identical (§6).
///
/// The `--force` entry in the same run is what makes the second half
/// worth asserting: before this, a destination `--force` was allowed to
/// replace could be overwritten by an earlier plan and stay overwritten
/// after a later check refused the run (§1.2).
#[test]
fn adopt_existing_pack_refused_by_validation_stays_byte_identical() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("bashrc", "unix owns ~/.bashrc")
        .done()
        .pack("work")
        .file("home.vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .home_file(".bashrc", "also new")
        .build();

    let pack = env.dotfiles_root.join("work");
    let before = file_snapshot(&pack);

    let ctx = make_ctx(&env);
    // `.vimrc` would replace `work/home.vimrc` under --force; `.bashrc`
    // would land at `work/bashrc`, which `unix/bashrc` already claims.
    let sources = vec![env.home.join(".vimrc"), env.home.join(".bashrc")];
    let err =
        commands::adopt::adopt(Some("work"), &sources, true, false, false, None, &ctx).unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected the prepared entry's conflict to refuse the run, got: {err}"
    );
    assert_eq!(
        file_snapshot(&pack),
        before,
        "a refused check must leave the existing pack byte-identical"
    );
    env.assert_regular_file(&env.home.join(".vimrc"), "NEW");
    env.assert_regular_file(&env.home.join(".bashrc"), "also new");
    assert!(preparation_dirs(&env).is_empty());
}

/// Publication creates the intermediate directories the plan needs and
/// no others, then renames each prepared entry into its final path
/// (§5.4).
#[test]
fn adopt_existing_pack_creates_only_the_intermediate_dirs_the_plan_needs() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("init.lua", "-- existing")
        .done()
        .home_file(".config/nvim/lua/plugins/init.lua", "-- plugins")
        .build();

    let pack = env.dotfiles_root.join("nvim");
    let created: Arc<std::sync::Mutex<Vec<std::path::PathBuf>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = created.clone();
    let pack_probe = pack.clone();
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| {
        if let super::support::FsOp::MkdirExclusive { path } = op {
            if path.starts_with(&pack_probe) {
                sink.lock().unwrap().push(path.to_path_buf());
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/nvim/lua/plugins/init.lua");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    assert_eq!(
        created.lock().unwrap().clone(),
        vec![pack.join("lua"), pack.join("lua/plugins")],
        "publication creates exactly the intermediate directories the plan needs"
    );
    env.assert_regular_file(&pack.join("lua/plugins/init.lua"), "-- plugins");
    env.assert_regular_file(&pack.join("init.lua"), "-- existing");
    env.assert_symlink(&source, &pack.join("lua/plugins/init.lua"));
    assert!(preparation_dirs(&env).is_empty());
}

/// Under `--force`, the existing destination is renamed into the
/// preparation directory before the prepared entry is published, and it
/// stays there until every source has been replaced (§5.4, §5.6) — the
/// window in which #378 can still put an individual entry back.
#[test]
fn adopt_existing_pack_force_retains_displaced_content_through_source_replacement() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("init.lua", "-- OLD")
        .done()
        .home_file(".config/nvim/init.lua", "-- NEW")
        .build();

    let pack = env.dotfiles_root.join("nvim");

    // What the pack and the preparation directory held at the moment
    // source replacement created its symlink.
    type Seen = Arc<std::sync::Mutex<Vec<(String, Vec<(String, Snapshot)>)>>>;
    let seen: Seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    let probe_env = env.dotfiles_root.clone();
    let pack_probe = pack.clone();
    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| {
        if let super::support::FsOp::Symlink { .. } = op {
            let published = std::fs::read_to_string(pack_probe.join("init.lua")).unwrap();
            let displaced = std::fs::read_dir(&probe_env)
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(".dodot-adopt-"))
                .flat_map(|e| file_snapshot(&e.path().join(".displaced")))
                .collect();
            sink.lock().unwrap().push((published, displaced));
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/nvim/init.lua");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "expected one source replacement");
    let (published, displaced) = &seen[0];
    assert_eq!(published, "-- NEW", "the prepared entry publishes first");
    assert_eq!(
        displaced,
        &vec![("init.lua".to_string(), Snapshot::file("-- OLD"))],
        "the displaced destination waits in the preparation directory \
         while sources are replaced"
    );

    // Finish discards it, which is where the user's --force takes effect.
    env.assert_regular_file(&pack.join("init.lua"), "-- NEW");
    env.assert_symlink(&source, &pack.join("init.lua"));
    assert!(preparation_dirs(&env).is_empty());
    assert!(displaced_snapshot(&env).is_empty());
}

/// A failure at entry N of M restores the pre-adopt content of every
/// entry publication changed through N, removes the intermediate
/// directories it created and left empty, leaves the sources untouched,
/// and names what it restored (§5.4, §6).
#[test]
fn adopt_existing_pack_failure_at_entry_two_restores_entry_one() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("init.lua", "-- existing")
        .done()
        .home_file(".config/nvim/lua/plugins/one.lua", "-- one")
        .home_file(".config/nvim/lua/plugins/two.lua", "-- two")
        .build();

    let pack = env.dotfiles_root.join("nvim");
    let before = file_snapshot(&pack);

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), |op| {
        if let super::support::FsOp::RenameNoReplace { to, .. } = op {
            if to.ends_with("two.lua") {
                return Err(crate::DodotError::Other("injected publish failure".into()));
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let sources = vec![
        env.home.join(".config/nvim/lua/plugins/one.lua"),
        env.home.join(".config/nvim/lua/plugins/two.lua"),
    ];
    let err = commands::adopt::adopt(None, &sources, false, false, false, None, &ctx).unwrap_err();

    match &err {
        crate::DodotError::PublicationRolledBack { restored, .. } => assert_eq!(
            restored,
            &vec!["lua/plugins/one.lua".to_string()],
            "the report names the entry publication put back"
        ),
        other => panic!("expected PublicationRolledBack, got: {other}"),
    }
    assert!(
        err.to_string().contains("injected publish failure"),
        "the report keeps the failure that caused the rollback: {err}"
    );
    assert_eq!(
        file_snapshot(&pack),
        before,
        "rollback restores the entries it published and removes the \
         intermediate directories it created"
    );
    env.assert_not_exists(&pack.join("lua"));
    env.assert_regular_file(&sources[0], "-- one");
    env.assert_regular_file(&sources[1], "-- two");
    assert!(preparation_dirs(&env).is_empty());
}

/// A `--force` publication failure puts every displaced destination
/// back, rather than committing an overwrite for a run that ended in a
/// refusal (§1.2, §6). Entries one and two were displaced and published;
/// entry three was displaced and then failed to publish.
#[test]
fn adopt_existing_pack_force_failure_restores_every_displaced_destination() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "OLD-1")
        .file("home.gvimrc", "OLD-2")
        .file("home.exrc", "OLD-3")
        .done()
        .home_file(".vimrc", "NEW-1")
        .home_file(".gvimrc", "NEW-2")
        .home_file(".exrc", "NEW-3")
        .build();

    let pack = env.dotfiles_root.join("vim");
    let before = file_snapshot(&pack);

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), |op| {
        if let super::support::FsOp::RenameNoReplace { to, .. } = op {
            if to.ends_with("home.exrc") {
                return Err(crate::DodotError::Other("injected publish failure".into()));
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let sources = vec![
        env.home.join(".vimrc"),
        env.home.join(".gvimrc"),
        env.home.join(".exrc"),
    ];
    let err =
        commands::adopt::adopt(Some("vim"), &sources, true, false, false, None, &ctx).unwrap_err();

    match &err {
        crate::DodotError::PublicationRolledBack { restored, .. } => assert_eq!(
            restored,
            &vec![
                "home.vimrc".to_string(),
                "home.gvimrc".to_string(),
                "home.exrc".to_string(),
            ],
            "every entry publication displaced is named, including the one \
             whose publish is what failed"
        ),
        other => panic!("expected PublicationRolledBack, got: {other}"),
    }
    assert_eq!(
        file_snapshot(&pack),
        before,
        "--force must not commit an overwrite for a run that then refused"
    );
    env.assert_regular_file(&sources[0], "NEW-1");
    env.assert_regular_file(&sources[1], "NEW-2");
    env.assert_regular_file(&sources[2], "NEW-3");
    assert!(preparation_dirs(&env).is_empty());
}

/// A failure before anything is displaced leaves the pack exactly as it
/// was and says so: there is no restored list because nothing had moved.
#[test]
fn adopt_existing_pack_failure_before_displacement_restores_nothing() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .build();

    let pack = env.dotfiles_root.join("vim");
    let before = file_snapshot(&pack);

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), |op| {
        if let super::support::FsOp::Rename { to, .. } = op {
            if to.to_string_lossy().contains(".displaced") {
                return Err(crate::DodotError::Other("injected displace failure".into()));
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        true,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    match &err {
        crate::DodotError::PublicationRolledBack { restored, .. } => {
            assert!(
                restored.is_empty(),
                "nothing had been published or displaced, got: {restored:?}"
            );
        }
        other => panic!("expected PublicationRolledBack, got: {other}"),
    }
    assert!(
        err.to_string()
            .contains("the failure came before the first entry was published"),
        "the report says nothing had moved: {err}"
    );
    assert_eq!(file_snapshot(&pack), before);
    env.assert_regular_file(&source, "NEW");
    assert!(preparation_dirs(&env).is_empty());
}

/// The same recovery for the shapes a file-only test misses: a directory
/// source displacing a directory destination under `--force`, alongside
/// a nested destination whose intermediate directories publication had
/// to create.
#[test]
fn adopt_existing_pack_failure_restores_a_displaced_directory_and_clears_nested_dirs() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("lua/plugins/old.lua", "-- pack's own")
        .done()
        .home_file(".config/nvim/lua/init.lua", "-- adopted")
        .home_file(".config/nvim/after/ftplugin/rust.lua", "-- after")
        .build();

    let pack = env.dotfiles_root.join("nvim");
    let before = file_snapshot(&pack);

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), |op| {
        if let super::support::FsOp::RenameNoReplace { to, .. } = op {
            if to.ends_with("rust.lua") {
                return Err(crate::DodotError::Other("injected publish failure".into()));
            }
        }
        Ok(())
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let sources = vec![
        env.home.join(".config/nvim/lua"),
        env.home.join(".config/nvim/after/ftplugin/rust.lua"),
    ];
    let err = commands::adopt::adopt(None, &sources, true, false, false, None, &ctx).unwrap_err();

    match &err {
        // Only `lua`: the nested entry is the one publication stopped
        // on, and with no prior content at its destination it displaced
        // nothing, so there was nothing of it to restore.
        crate::DodotError::PublicationRolledBack { restored, .. } => {
            assert_eq!(restored, &vec!["lua".to_string()])
        }
        other => panic!("expected PublicationRolledBack, got: {other}"),
    }
    assert_eq!(
        file_snapshot(&pack),
        before,
        "the displaced directory comes back whole and the directories \
         created for the nested entry come out"
    );
    env.assert_not_exists(&pack.join("after"));
    // The directory source is still a real directory, not a symlink.
    assert!(!env.fs.is_symlink(&sources[0]));
    env.assert_regular_file(&sources[0].join("init.lua"), "-- adopted");
    env.assert_regular_file(&sources[1], "-- after");
    assert!(preparation_dirs(&env).is_empty());
}

/// The successful path for those same shapes: a directory source
/// replacing a directory destination under `--force`, and a nested
/// destination created on the way. Source replacement still happens and
/// the CLI behavior around it is unchanged.
#[test]
fn adopt_existing_pack_publishes_directories_and_nested_destinations() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("lua/plugins/old.lua", "-- pack's own")
        .done()
        .home_file(".config/nvim/lua/init.lua", "-- adopted")
        .home_file(".config/nvim/after/ftplugin/rust.lua", "-- after")
        .build();

    let pack = env.dotfiles_root.join("nvim");
    let ctx = make_ctx(&env);
    let sources = vec![
        env.home.join(".config/nvim/lua"),
        env.home.join(".config/nvim/after/ftplugin/rust.lua"),
    ];
    commands::adopt::adopt(None, &sources, true, false, false, None, &ctx).unwrap();

    assert_eq!(
        file_snapshot(&pack),
        vec![
            (
                "after/ftplugin/rust.lua".to_string(),
                Snapshot::file("-- after")
            ),
            ("lua/init.lua".to_string(), Snapshot::file("-- adopted")),
        ],
        "the displaced `lua/` is gone at Finish and the adopted tree stands"
    );
    env.assert_symlink(&sources[0], &pack.join("lua"));
    env.assert_symlink(&sources[1], &pack.join("after/ftplugin/rust.lua"));
    assert!(preparation_dirs(&env).is_empty());
}

/// A process killed mid-publication leaves an intermediate pack and an
/// identifiable `.dodot-adopt-` directory behind (§5.4). Neither is a
/// later run's to use or to clean up: pack discovery skips the
/// preparation directory on its name, and adopt publishes only from the
/// directory it created itself.
#[test]
fn adopt_into_existing_pack_leaves_a_leftover_preparation_directory_alone() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("init.lua", "-- existing")
        // The intermediate pack a killed run would have left: one entry
        // of a two-entry publication, already at its final path.
        .file("lua/one.lua", "-- published before the kill")
        .done()
        .home_file(".config/nvim/opts.lua", "-- opts")
        .build();

    // The killed run's preparation directory, with its unpublished entry
    // and the destination it had displaced.
    let leftover = env.dotfiles_root.join(".dodot-adopt-deadbeef");
    std::fs::create_dir_all(leftover.join("nvim/lua")).unwrap();
    std::fs::write(leftover.join("nvim/lua/two.lua"), b"-- never published").unwrap();
    std::fs::create_dir_all(leftover.join(".displaced")).unwrap();
    std::fs::write(leftover.join(".displaced/init.lua"), b"-- displaced").unwrap();
    let leftover_before = file_snapshot(&leftover);

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/opts.lua");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap();

    assert_eq!(
        pack_names(&env),
        vec!["nvim".to_string()],
        "pack discovery reads the leftover as nothing at all"
    );
    assert_eq!(
        file_snapshot(&leftover),
        leftover_before,
        "a later adopt neither publishes from a leftover nor deletes one"
    );
    // The intermediate pack stands as the killed run left it, and this
    // run's own entry published alongside it.
    let pack = env.dotfiles_root.join("nvim");
    env.assert_regular_file(&pack.join("lua/one.lua"), "-- published before the kill");
    env.assert_regular_file(&pack.join("opts.lua"), "-- opts");
    env.assert_symlink(&source, &pack.join("opts.lua"));
    assert_eq!(
        preparation_dirs(&env),
        vec![".dodot-adopt-deadbeef".to_string()]
    );
}

/// `--dry-run` into a pack that already exists reports the plan and
/// writes nothing — not into the pack, and not a preparation directory
/// left behind (§5.3).
#[test]
fn adopt_existing_pack_dry_run_writes_no_final_path() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("init.lua", "-- existing")
        .done()
        .home_file(".config/nvim/lua/plugins/init.lua", "-- plugins")
        .build();

    let pack = env.dotfiles_root.join("nvim");
    let before = file_snapshot(&pack);

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/lua/plugins/init.lua");
    let result = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        true, // --dry-run
        None,
        &ctx,
    )
    .unwrap();

    assert!(result.dry_run);
    assert_eq!(file_snapshot(&pack), before);
    env.assert_not_exists(&pack.join("lua"));
    env.assert_regular_file(&source, "-- plugins");
    assert!(preparation_dirs(&env).is_empty());
}

/// A rollback step can fail too, and when it does the run must not
/// claim the pack is back the way it was — the caller would then
/// discard the preparation directory holding the only copy of the
/// displaced content.
///
/// Publication fails at entry three; the rename that puts entry two's
/// displaced destination back fails in turn. The two entries whose
/// pre-adopt content did return are reported as restored, entry two is
/// reported with the path its content is at, and the preparation
/// directory survives the run.
#[test]
fn adopt_existing_pack_rollback_that_cannot_restore_keeps_the_displaced_content() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "OLD-1")
        .file("home.gvimrc", "OLD-2")
        .file("home.exrc", "OLD-3")
        .done()
        .home_file(".vimrc", "NEW-1")
        .home_file(".gvimrc", "NEW-2")
        .home_file(".exrc", "NEW-3")
        .build();

    let pack = env.dotfiles_root.join("vim");

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), |op| {
        match op {
            // Publication stops here.
            super::support::FsOp::RenameNoReplace { to, .. } if to.ends_with("home.exrc") => {
                Err(crate::DodotError::Other("injected publish failure".into()))
            }
            // …and this entry's displaced destination cannot go back.
            super::support::FsOp::Rename { from, to }
                if from.to_string_lossy().contains(".displaced") && to.ends_with("home.gvimrc") =>
            {
                Err(crate::DodotError::Other("injected restore failure".into()))
            }
            _ => Ok(()),
        }
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let sources = vec![
        env.home.join(".vimrc"),
        env.home.join(".gvimrc"),
        env.home.join(".exrc"),
    ];
    let err =
        commands::adopt::adopt(Some("vim"), &sources, true, false, false, None, &ctx).unwrap_err();

    match &err {
        crate::DodotError::PublicationRollbackIncomplete {
            restored, stranded, ..
        } => {
            assert_eq!(
                restored,
                &vec!["home.vimrc".to_string(), "home.exrc".to_string()],
                "only the entries whose pre-adopt content actually went back"
            );
            assert_eq!(stranded.len(), 1, "got: {stranded:?}");
            assert_eq!(stranded[0].in_pack, "home.gvimrc");
            assert!(
                stranded[0].at.contains(".dodot-adopt-") && stranded[0].at.contains(".displaced"),
                "the report points at where the content actually is: {}",
                stranded[0].at
            );
        }
        other => panic!("expected PublicationRollbackIncomplete, got: {other}"),
    }

    // The entries that could be restored were.
    env.assert_regular_file(&pack.join("home.vimrc"), "OLD-1");
    env.assert_regular_file(&pack.join("home.exrc"), "OLD-3");
    // The one that could not is still on disk, and the run left the
    // preparation directory in place rather than deleting it.
    assert_eq!(preparation_dirs(&env).len(), 1);
    assert_eq!(
        displaced_snapshot(&env),
        vec![("home.gvimrc".to_string(), Snapshot::file("OLD-2"))],
        "the pre-adopt content the rollback could not move is still there"
    );
    // Sources are untouched, as on every publication failure.
    env.assert_regular_file(&sources[0], "NEW-1");
    env.assert_regular_file(&sources[1], "NEW-2");
    env.assert_regular_file(&sources[2], "NEW-3");
}

/// Rollback removes the intermediate directories it created only while
/// they are still empty, and the kernel is what decides that: content
/// another process writes into one after adopt would have looked
/// survives, because `remove_dir_empty` refuses a directory that is not
/// empty inside the same operation that would remove it.
#[test]
fn adopt_existing_pack_rollback_keeps_a_directory_that_gained_content() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("init.lua", "-- existing")
        .done()
        .home_file(".config/nvim/lua/plugins/init.lua", "-- plugins")
        .build();

    let pack = env.dotfiles_root.join("nvim");
    let racer = pack.join("lua/someone-elses.lua");
    let race_target = pack.join("lua");
    let racer_probe = racer.clone();

    let fs = super::support::InterposedFs::wrap(env.fs.clone(), move |op| match op {
        super::support::FsOp::RenameNoReplace { to, .. } if to.ends_with("init.lua") => {
            Err(crate::DodotError::Other("injected publish failure".into()))
        }
        // Another process writes into the directory adopt created,
        // after adopt would have observed it empty.
        super::support::FsOp::RemoveDirEmpty { path } if path == race_target => {
            std::fs::write(&racer_probe, b"-- not adopt's to delete").unwrap();
            Ok(())
        }
        _ => Ok(()),
    });

    let ctx = make_ctx_with_fs(&env, fs);
    let source = env.home.join(".config/nvim/lua/plugins/init.lua");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::PublicationRolledBack { .. }),
        "got: {err}"
    );
    env.assert_regular_file(&racer, "-- not adopt's to delete");
    // The innermost directory is gone — nothing raced it — while the
    // one that gained content stands with what was put there.
    env.assert_not_exists(&pack.join("lua/plugins"));
    env.assert_regular_file(&source, "-- plugins");
}

/// Validation reads the tree publication will leave, not the union of
/// before and after (§5.3). An entry `--force` replaces will not be in
/// the published pack, so the targets it claims today must not refuse
/// the run — which also lets `adopt --force` repair a cross-pack
/// conflict that already exists.
///
/// Here `work/externals.toml` declares `~/.bashrc`, which `unix` also
/// claims: the repo is already in conflict. The adopted file replaces
/// that manifest with one declaring `~/.zshrc`, so the claim that
/// collides is not in the tree publication leaves and the run goes
/// ahead.
#[test]
fn adopt_force_validates_the_tree_publication_leaves_not_the_one_on_disk() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("home.bashrc", "unix owns ~/.bashrc")
        .done()
        .pack("work")
        .file(
            "externals.toml",
            r#"
            [bashrc]
            type   = "file"
            url    = "https://example.com/bashrc"
            target = "~/.bashrc"
            sha256 = "abc"
        "#,
        )
        .done()
        .home_file(
            ".config/work/externals.toml",
            r#"
            [zshrc]
            type   = "file"
            url    = "https://example.com/zshrc"
            target = "~/.zshrc"
            sha256 = "def"
        "#,
        )
        .build();

    let pack = env.dotfiles_root.join("work");
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    let source = env.home.join(".config/work/externals.toml");
    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        None,
        &ctx,
    )
    .expect("the replacement drops the colliding claim, so validation accepts");

    assert!(
        std::fs::read_to_string(pack.join("externals.toml"))
            .unwrap()
            .contains("~/.zshrc"),
        "the adopted manifest is what the pack now holds"
    );
    env.assert_symlink(&source, &pack.join("externals.toml"));
    assert!(preparation_dirs(&env).is_empty());
}

/// A conflict the `--force` run does *not* remove still refuses it: the
/// overlay drops only the entries publication replaces, and every other
/// pack keeps every claim it has.
///
/// Same shape as above, with the adopted manifest declaring the very
/// target `unix` claims.
#[test]
fn adopt_force_still_refuses_a_conflict_the_replacement_leaves_standing() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("home.bashrc", "unix owns ~/.bashrc")
        .done()
        .pack("work")
        .file(
            "externals.toml",
            r#"
            [zshrc]
            type   = "file"
            url    = "https://example.com/zshrc"
            target = "~/.zshrc"
            sha256 = "def"
        "#,
        )
        .done()
        .home_file(
            ".config/work/externals.toml",
            r#"
            [bashrc]
            type   = "file"
            url    = "https://example.com/bashrc"
            target = "~/.bashrc"
            sha256 = "abc"
        "#,
        )
        .build();

    let pack = env.dotfiles_root.join("work");
    let before = file_snapshot(&pack);

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    let source = env.home.join(".config/work/externals.toml");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "got: {err}"
    );
    assert_eq!(
        file_snapshot(&pack),
        before,
        "a refused run leaves the existing pack byte-identical"
    );
    assert!(preparation_dirs(&env).is_empty());
}

/// A regular file where an intermediate directory has to go is refused
/// at that level and named, rather than descending past it and failing
/// later with the rename's error. The pack is left as it was.
#[test]
fn adopt_existing_pack_refuses_an_intermediate_that_is_not_a_directory() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("lua", "-- a file, where a directory has to go")
        .done()
        .home_file(".config/nvim/lua/plugins/init.lua", "-- plugins")
        .build();

    let pack = env.dotfiles_root.join("nvim");
    let before = file_snapshot(&pack);

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/lua/plugins/init.lua");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("is not a directory"),
        "the refusal names the level that is not a directory: {err}"
    );
    assert_eq!(file_snapshot(&pack), before);
    env.assert_not_exists(&pack.join("lua/plugins"));
    env.assert_regular_file(&source, "-- plugins");
    assert!(preparation_dirs(&env).is_empty());
}
