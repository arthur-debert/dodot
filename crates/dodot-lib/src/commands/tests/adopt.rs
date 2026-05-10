//! Integration tests for the `adopt` command (and the related "adopt: pack not found hint" UX section).

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

use super::support::{make_ctx, make_ctx_with_runner, CannedRunner};

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

    // File should have moved into pack with the `home.` prefix (post-#48
    // adopt rename — preserves the round-trip back to ~/.vimrc on `up`),
    // content preserved.
    env.assert_regular_file(
        &env.dotfiles_root.join("vim/home.vimrc"),
        "set nocompatible",
    );
    // Symlink should exist at original location
    assert!(env.fs.is_symlink(&source));

    // Status output should include the vim pack with the adopted file
    assert!(result.packs.iter().any(|p| p.name == "vim"));
    let vim = result.packs.iter().find(|p| p.name == "vim").unwrap();
    assert!(vim.files.iter().any(|f| f.name == "home.vimrc"));
}

#[test]
fn adopt_preserves_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;

    // Uses a dotted file (post-#2: non-dotted $HOME entries are
    // refused for round-trip safety). The test's intent is exec-bit
    // preservation, not the dot-or-not policy.
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("placeholder", "")
        .done()
        .home_file(".script.sh", "#!/bin/sh\necho hi")
        .build();

    let source = env.home.join(".script.sh");
    // Mark source as executable
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

/// Regression for review item #2 on PR #49: a non-dotted entry in
/// $HOME has no automatic round-trip path under the post-#48 XDG
/// default — adopt must refuse rather than silently relocate.
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
    // Source untouched, no pack copy created.
    env.assert_regular_file(&source, "#!/bin/sh\necho hi");
    env.assert_not_exists(&env.dotfiles_root.join("tools/script.sh"));
}

#[test]
fn adopt_destination_conflict_refused_without_force() {
    // Destination conflict: pack already has `home.vimrc`. Adopt of
    // `~/.vimrc` derives `home.vimrc` as the pack filename (post-#48
    // adopt rename), so the existing file blocks the adoption.
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

    // Original file untouched; existing pack file untouched.
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
    // Dotted-directory adoption from $HOME directly: contents move to
    // pack/_home/<stripped>/, which round-trips back via the `_home/`
    // subtree-escape (Priority 2) on `dodot up`. We use a non-XDG
    // dotted dir so the test stays decoupled from the XDG-source
    // inference rules — adopting `~/.config/` itself is now refused
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

    // Original path is now a symlink to the pack copy.
    assert!(env.fs.is_symlink(&source));
    let target = env.fs.readlink(&source).unwrap();
    assert_eq!(target, pack_dir);
}

/// Regression for review item #1 on PR #49: a dotted directory adopted
/// from $HOME (not in force_home) must round-trip back via the
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

    // Adopted under chats/_home/weechat (the `_home/` per-subtree
    // routing tells the symlink handler to deploy back to $HOME/.X).
    let pack_dir = env.dotfiles_root.join("chats/_home/weechat");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("weechat.conf"), "[server]");

    // Re-deploying with `dodot up` puts the symlink back at $HOME/.weechat
    // — the round-trip the rename was designed to preserve.
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
    // Uses a dotted directory (post-#2: non-dotted $HOME entries are
    // refused). Test intent: inner symlinks are preserved during the
    // copy phase. The `_home/` path comes from #1's dotted-dir
    // round-trip rename.
    let env = TempEnvironment::builder()
        .pack("shell")
        .file("placeholder", "")
        .done()
        .home_file(".mydir/real.txt", "hello")
        .build();

    // Create an inner symlink: .mydir/alias -> .mydir/real.txt
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

    // The inner link should still be a symlink inside the pack copy.
    let copied_link = env.dotfiles_root.join("shell/_home/mydir/alias");
    assert!(
        env.fs.is_symlink(&copied_link),
        "inner symlink should be preserved as a symlink, not followed"
    );
}

/// `~/.config/<X>/<rest>` is now a recognized adopt source: the first
/// segment under `$XDG_CONFIG_HOME` is the inferred pack name, and the
/// remainder is the in-pack path. Round-trip is the resolver's default
/// rule — pack `nvim` containing `init.lua` deploys to
/// `$XDG_CONFIG_HOME/nvim/init.lua` on `dodot up`. (Pre-inference, this
/// case was refused as a nested source.)
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

    // Pack auto-created at `<dotfiles>/ghostty/`, file landed at root.
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

    // Each top-level child of `~/.config/helix/` became its own pack
    // entry — `config.toml` (file) and `themes/` (dir) — both at pack
    // root, not nested under another `helix/`.
    let pack_dir = env.dotfiles_root.join("helix");
    env.assert_regular_file(&pack_dir.join("config.toml"), "theme = \"onedark\"");
    env.assert_regular_file(&pack_dir.join("themes/extra.toml"), "fg = \"white\"");
    // Original entries are now symlinks at their original paths
    // (one per top-level child, not one for the whole helix/ dir).
    assert!(env
        .fs
        .is_symlink(&env.home.join(".config/helix/config.toml")));
    assert!(env.fs.is_symlink(&env.home.join(".config/helix/themes")));
    // Parent directory `~/.config/helix/` itself stays a real directory
    // — only its children became symlinks.
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
/// instead of the original `$XDG/<X>/...`. (Regression for Copilot
/// review on PR #85.)
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

    // Each expanded child lives under `toolbox/_xdg/lazygit/...` so the
    // resolver's Priority 2 `_xdg/` prefix routes back to
    // `~/.config/lazygit/<child>` regardless of the override pack name.
    env.assert_regular_file(
        &env.dotfiles_root.join("toolbox/_xdg/lazygit/config.yml"),
        "gui:\n  theme: dark",
    );
    env.assert_regular_file(
        &env.dotfiles_root.join("toolbox/_xdg/lazygit/themes/x.yml"),
        "fg: white",
    );
    // Each original child is now a symlink (per-child expansion); the
    // pack-root dir itself stays a real directory.
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

    // Original deploy location is now a symlink — and the symlink
    // chain points (eventually) back at the pack copy. Resolve via
    // resolve_target_full to confirm round-trip.
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

    // The pack-root directory expanded: the single child of `Cursor/`
    // is `User/`, so the pack contains `_app/Cursor/User/` (a
    // directory whose contents come along).
    let pack_dir = env.dotfiles_root.join("Cursor");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("_app/Cursor/User/settings.json"), "{}");
    env.assert_regular_file(&pack_dir.join("_app/Cursor/User/keybindings.json"), "[]");
    // The expanded child (`User/`) at the original AppSupport
    // location is now a symlink, but the parent `Cursor/` itself
    // stays a real directory.
    assert!(env.fs.is_symlink(&env.app_support.join("Cursor/User")));
    assert!(!env.fs.is_symlink(&source));
}

/// M5 capitalization-heuristic advisory: when a user adopts an
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
/// the M6 brew probe identifies a matching cask: prefer the cask
/// token (`iina`) over the awful whitespace-strip-lowercase fallback
/// (`comcolliderliiina`). Real IINA case from user testing on PR #91.
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

    // The good outcome: tip suggests `iina` as the rename target.
    assert!(
        tip.contains("renaming the pack to `iina`"),
        "expected cask-token-based rename suggestion (`iina`), got: {tip}"
    );
    // And explicitly NOT the whitespace-strip-lowercase fallback,
    // which would be `comcolliderliiina` for this folder.
    assert!(
        !tip.contains("comcolliderliiina"),
        "rename suggestion fell back to lowercase mangling instead of cask token: {tip}"
    );
    // The tip credits the cask so the user knows where the
    // recommendation came from.
    assert!(
        tip.contains("matches homebrew cask"),
        "tip should credit the cask source, got: {tip}"
    );
}

/// When no installed cask matches the folder, the tip falls back to
/// the original whitespace-strip-lowercase suggestion. Pins the
/// fallback so a refactor doesn't accidentally regress the no-cask
/// path (the heuristic still triggers on uppercase folders even when
/// brew has nothing to say).
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

    // Fallback suggestion: lowercased pack name (no spaces here, but
    // the casing transformation still applies).
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
    // Direct symlink to pack source — adopt skips with a #44 message
    // pointing the user at `dodot up` to upgrade to the full chain.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "content")
        .done()
        .build();

    // Pre-link home file to the pack.
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
    // Source still a symlink, pack file untouched.
    assert!(env.fs.is_symlink(&source));
    env.assert_regular_file(&pack_file, "content");
}

/// Regression for #44: when the source is fully managed (the user
/// symlink points at dodot's data_dir), adopt skips with the original
/// "already managed by dodot" wording — no upgrade needed.
#[test]
fn adopt_fully_managed_source_keeps_original_skip_message() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "content")
        .done()
        .build();

    let ctx = make_ctx(&env);
    // First, deploy normally so user_path goes through the dodot chain.
    // Under #48 the default deploy target is $XDG_CONFIG_HOME/<pack>/<file>.
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

/// Regression for #44: `dodot up` auto-replaces a pre-existing regular
/// file whose content is byte-identical to the pack source — no
/// `--force` needed, no conflict reported.
#[test]
fn up_auto_replaces_content_equivalent_pre_existing_file() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = test")
        .done()
        // Same content as the pack source.
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
    // ~/.gitconfig is now a symlink (the dodot chain), not a regular file.
    let user_path = env.home.join(".gitconfig");
    assert!(
        env.fs.is_symlink(&user_path),
        "user file should now be a symlink"
    );
    // Content reaching the user is unchanged.
    assert_eq!(
        env.fs.read_to_string(&user_path).unwrap(),
        "[user]\n  name = test"
    );
    // And status agrees: deployed, not a conflict.
    let status = commands::status::status(None, &ctx).unwrap();
    let file = &status.packs[0].files[0];
    assert_eq!(file.status, "deployed");
}

/// Regression for #44: `dodot up` still refuses (without `--force`) when
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
    // Original content preserved.
    env.assert_file_contents(&env.home.join(".gitconfig"), "[user]\n  name = old");
}

/// Regression for #44: `status` does NOT flag a content-equivalent
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

    // Run with CWD = HOME so the relative path resolves naturally.
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

#[test]
fn adopt_broken_pack_blocks_conflict_check() {
    // If another pack fails intent collection, adoption must refuse rather
    // than silently proceed — otherwise the conflict check produces a false
    // negative and we'd mutate into a state `dodot up` would later reject.
    let env = TempEnvironment::builder()
        .pack("broken")
        .file("config.toml.tmpl", "{{ missing_var }}")
        .done()
        .pack("target")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "content")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        Some("target"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        None,
        &ctx,
    )
    .unwrap_err();

    // The error surfaces from the broken pack's intent collection
    // (template render failure), not a silent success.
    assert!(
        matches!(err, crate::DodotError::TemplateRender { .. }),
        "expected the broken pack's error to surface, got: {err}"
    );

    // Home untouched; no pack copy left behind.
    env.assert_regular_file(&source, "content");
    env.assert_not_exists(&env.dotfiles_root.join("target/vimrc"));
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

    // Home untouched.
    env.assert_regular_file(&source, "new");
    // Pack copy rolled back.
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

    // Nothing changed at home.
    env.assert_regular_file(&source, "content");
    assert!(!env.fs.is_symlink(&source));
    // No copy in pack.
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

    // ~/.vimrc is a symlink to ~/real_vimrc
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

    // The pack copy should be a symlink (not a regular file with copied content).
    let pack_copy = env.dotfiles_root.join("vim/home.vimrc");
    assert!(
        env.fs.is_symlink(&pack_copy),
        "--no-follow should preserve source symlink as a symlink in the pack"
    );
    // Home path replaced with a symlink into the pack.
    assert!(env.fs.is_symlink(&source));
}

#[cfg(unix)]
#[test]
fn adopt_force_preserves_old_content_when_copy_fails() {
    // With --force, the old destination must remain intact if the copy of
    // the new source fails. Previously copy_all removed the dest before
    // copying, so a copy failure silently lost the old content.
    use std::os::unix::fs::PermissionsExt;

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
    // The old pack content must survive the failed --force adoption.
    env.assert_regular_file(&env.dotfiles_root.join("vim/home.vimrc"), "OLD");
    // Home file also untouched.
    env.assert_regular_file(&source, "NEW");
    // No lingering stage file in the pack.
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
    // target. Regression test: check_readable previously used fs.is_dir +
    // fs.stat, both of which follow symlinks and would fail here.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .build();

    // Create ~/.dangling -> /does/not/exist (target intentionally missing).
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

    // The pack copy should itself be a symlink (preserving the dangling link).
    // Post-#48 adopt rename: ~/.dangling → vim/home.dangling.
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
