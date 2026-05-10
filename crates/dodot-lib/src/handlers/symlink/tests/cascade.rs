//! Symlink-resolver priority tier tests: `force_home`, `_home/`/`_xdg/` escape hatches,
//! `_app/`, `_lib/` (macOS), `force_app`, `app_aliases`, plus the file-level
//! `home.` / `app.` / `xdg.` / `lib.` prefix conventions.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::path::PathBuf;

use crate::handlers::symlink::*;
use crate::handlers::{Handler, HandlerConfig, HandlerScope, HANDLER_SYMLINK};
use crate::operations::HandlerIntent;
use crate::packs::Pack;
use crate::paths::Pather;
use crate::rules::RuleMatch;

use super::{default_config, test_pather};

// ── Priority 3: force_home ──────────────────────────────────

#[test]
fn force_home_top_level_file() {
    let target = resolve_target("shell", "bashrc", &default_config(), &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.bashrc"));
}

#[test]
fn force_home_subdirectory_file() {
    let target = resolve_target("net", "ssh/config", &default_config(), &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.ssh/config"));
}

#[test]
fn force_home_top_level_dir_wholesale() {
    let target = resolve_target("net", "ssh", &default_config(), &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.ssh"));
}

// ── Priority 2: _home/ and _xdg/ escape hatches ─────────────

#[test]
fn home_prefix_dir_escapes_pack_namespace() {
    // _home/<rest> deploys raw to $HOME/.<rest>, regardless of pack
    // name. Useful when a single pack groups files that belong in
    // $HOME without being in force_home.
    let config = HandlerConfig::default();
    let target = resolve_target("misc", "_home/vim/vimrc", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.vim/vimrc"));
}

#[test]
fn xdg_prefix_dir_escapes_pack_namespace() {
    // _xdg/<rest> deploys raw to $XDG_CONFIG_HOME/<rest>, NOT under
    // the pack name. Useful for packs whose name doesn't match the
    // target program (e.g. a `term-config` pack containing
    // `_xdg/ghostty/config`).
    let config = HandlerConfig::default();
    let target = resolve_target(
        "term-config",
        "_xdg/ghostty/config",
        &config,
        &test_pather(),
    );
    assert_eq!(target, PathBuf::from("/home/alice/.config/ghostty/config"));
}

// ── Priority 2c: _app/ prefix ───────────────────────────────

#[test]
fn app_prefix_routes_to_app_support_root() {
    // _app/<rest> deploys raw under app_support_dir, no pack
    // namespace. The pack-relative path mirrors the on-disk
    // Application Support tree exactly.
    let config = HandlerConfig::default();
    let target = resolve_target(
        "macapps",
        "_app/Code/User/settings.json",
        &config,
        &test_pather(),
    );
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/Code/User/settings.json")
    );
}

#[test]
fn app_prefix_outranks_default() {
    // A pack literally named `Code` with `_app/Code/x` is covered by
    // Priority 2c — the default rule (Priority 6) never sees it.
    let config = HandlerConfig::default();
    let target = resolve_target("Code", "_app/Code/x", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/Code/x")
    );
}

// ── Priority 2d: _lib/ prefix (macOS only) ──────────────────

#[test]
fn lib_prefix_resolution_full_returns_skip_on_non_macos() {
    // The `_lib/` prefix is macOS-only. On every other host the
    // resolver returns `Resolution::Skip` so the symlink handler
    // can omit the intent and surface a soft warning. The test is
    // gated on `cfg!(target_os = "macos")` for the positive case
    // (the skip branch is the *only* branch on Linux CI).
    let config = HandlerConfig::default();
    let resolution = resolve_target_full(
        "macapps",
        "_lib/LaunchAgents/com.example.foo.plist",
        &config,
        &test_pather(),
    );
    if cfg!(target_os = "macos") {
        match resolution {
            Resolution::Path(p) => assert_eq!(
                p,
                PathBuf::from("/home/alice/Library/LaunchAgents/com.example.foo.plist")
            ),
            Resolution::Skip { reason } => {
                panic!("expected Path on macOS, got Skip({reason})")
            }
        }
    } else {
        assert!(
            matches!(resolution, Resolution::Skip { .. }),
            "_lib/ on non-macOS must skip; got {resolution:?}"
        );
    }
}

// ── Priority 4: force_app ───────────────────────────────────

#[test]
fn force_app_routes_first_segment_to_app_support() {
    // `force_app = ["Code"]` makes a top-level `Code/...` entry
    // route to <app_support_dir>/Code/... without a `_app/` prefix
    // in the pack tree.
    let config = HandlerConfig {
        force_app: vec!["Code".into()],
        ..HandlerConfig::default()
    };
    let target = resolve_target(
        "macapps",
        "Code/User/settings.json",
        &config,
        &test_pather(),
    );
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/Code/User/settings.json")
    );
}

#[test]
fn force_app_is_case_sensitive() {
    // `Code` ≠ `code`. Library folder names are case-sensitive on
    // macOS, and conflating `Code` (VS Code) with `code` (a CLI
    // tool's `~/.config/code/`) would route the latter wrong.
    let config = HandlerConfig {
        force_app: vec!["Code".into()],
        ..HandlerConfig::default()
    };
    let target = resolve_target("misc", "code/foo", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/misc/code/foo"));
}

#[test]
fn force_app_loses_to_explicit_app_prefix() {
    // Priority 2c (`_app/`) outranks Priority 4 (`force_app`). A
    // pack that mixes both gets the explicit prefix's routing.
    let config = HandlerConfig {
        force_app: vec!["Code".into()],
        ..HandlerConfig::default()
    };
    let target = resolve_target("misc", "_app/Code/x", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/Code/x")
    );
}

// ── Priority 5: app_aliases ─────────────────────────────────

#[test]
fn app_alias_reroutes_default_rule() {
    // Pack `vscode` aliased to `Code` deploys top-level files to
    // <app_support_dir>/Code/... instead of $XDG/vscode/...
    let mut aliases = std::collections::HashMap::new();
    aliases.insert("vscode".into(), "Code".into());
    let config = HandlerConfig {
        app_aliases: aliases,
        ..HandlerConfig::default()
    };
    let target = resolve_target("vscode", "User/settings.json", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/Code/User/settings.json")
    );
}

#[test]
fn app_alias_loses_to_explicit_xdg_prefix() {
    // Aliases only modify the default rule (Priority 6). A
    // `_xdg/...` entry is Priority 2b and routes raw under XDG —
    // explicit user intent wins over the alias.
    let mut aliases = std::collections::HashMap::new();
    aliases.insert("vscode".into(), "Code".into());
    let config = HandlerConfig {
        app_aliases: aliases,
        ..HandlerConfig::default()
    };
    let target = resolve_target("vscode", "_xdg/Code/User/foo", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/Code/User/foo"));
}

#[test]
fn app_alias_loses_to_home_prefix() {
    // home.X (Priority 1) outranks alias-driven defaults — a
    // `home.foo` file in an aliased pack still routes to ~/.foo.
    let mut aliases = std::collections::HashMap::new();
    aliases.insert("vscode".into(), "Code".into());
    let config = HandlerConfig {
        app_aliases: aliases,
        ..HandlerConfig::default()
    };
    let target = resolve_target("vscode", "home.editorconfig", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.editorconfig"));
}

#[test]
fn app_alias_uses_pack_display_name() {
    // Pack ordering prefix is stripped before alias lookup, just
    // like for the default rule. `010-vscode` aliased as `vscode`
    // → `Code` still routes correctly.
    let mut aliases = std::collections::HashMap::new();
    aliases.insert("vscode".into(), "Code".into());
    let config = HandlerConfig {
        app_aliases: aliases,
        ..HandlerConfig::default()
    };
    let target = resolve_target("010-vscode", "settings.json", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/Code/settings.json")
    );
}

#[test]
fn force_app_outranks_app_alias() {
    // `force_app` is Priority 4, `app_aliases` is Priority 5. If a
    // pack has an alias and a top-level entry whose first segment
    // is in force_app, the force_app routing wins.
    let mut aliases = std::collections::HashMap::new();
    aliases.insert("anything".into(), "AliasedFolder".into());
    let config = HandlerConfig {
        force_app: vec!["Cursor".into()],
        app_aliases: aliases,
        ..HandlerConfig::default()
    };
    let target = resolve_target("anything", "Cursor/x", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/Cursor/x")
    );
}

// ── Priority 1: home. prefix convention ─────────────────────

#[test]
fn home_prefix_routes_top_level_file_to_home() {
    // home.X is the per-file opt-in for $HOME/.X placement, replacing
    // the older `dot.X` prefix in #48.
    let config = HandlerConfig::default();
    let target = resolve_target("git", "home.gitconfig", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.gitconfig"));
}

#[test]
fn home_prefix_works_even_when_pack_not_force_home() {
    let config = HandlerConfig::default();
    let target = resolve_target("misc", "home.vimrc", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.vimrc"));
}

#[test]
fn home_prefix_not_applied_to_subdirs() {
    // home. prefix only works for top-level files; nested
    // home.<x> files keep the literal name under the pack's XDG dir.
    let config = HandlerConfig::default();
    let target = resolve_target("misc", "subdir/home.conf", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/.config/misc/subdir/home.conf")
    );
}

#[test]
fn strip_file_prefix_unit() {
    assert_eq!(
        strip_file_prefix("home.bashrc"),
        Some((FilePrefix::Home, "bashrc"))
    );
    assert_eq!(
        strip_file_prefix("home.vimrc"),
        Some((FilePrefix::Home, "vimrc"))
    );
    assert_eq!(
        strip_file_prefix("app.config"),
        Some((FilePrefix::App, "config"))
    );
    assert_eq!(
        strip_file_prefix("xdg.mimeapps.list"),
        Some((FilePrefix::Xdg, "mimeapps.list"))
    );
    assert_eq!(
        strip_file_prefix("lib.com.example.plist"),
        Some((FilePrefix::Lib, "com.example.plist"))
    );

    assert_eq!(strip_file_prefix("vimrc"), None);
    assert_eq!(strip_file_prefix(".bashrc"), None);
    // Nested files keep prefixes literal — only top-level files opt in.
    assert_eq!(strip_file_prefix("sub/home.conf"), None);
    assert_eq!(strip_file_prefix("sub/app.json"), None);
    // Empty remainders fall through to the default rule rather than
    // targeting a bare directory root (`$HOME/.`, `<app>/`, …).
    assert_eq!(strip_file_prefix("home."), None);
    assert_eq!(strip_file_prefix("app."), None);
    assert_eq!(strip_file_prefix("xdg."), None);
    assert_eq!(strip_file_prefix("lib."), None);
}

/// A file literally named `home.` falls through the priority list to
/// the pack-namespaced XDG default — never to `$HOME/.`. Regression
/// for review item #3 on PR #49.
#[test]
fn literal_home_dot_filename_does_not_target_home_root() {
    let config = HandlerConfig::default();
    let target = resolve_target("misc", "home.", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/misc/home."));
}

/// Same regression for the new file prefixes — bare `app.`, `xdg.`,
/// `lib.` filenames must not target the routing root.
#[test]
fn literal_bare_file_prefix_filenames_do_not_target_root() {
    let config = HandlerConfig::default();
    for name in ["app.", "xdg.", "lib."] {
        let target = resolve_target("misc", name, &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from(format!("/home/alice/.config/misc/{name}")),
            "bare `{name}` should fall through to default rule"
        );
    }
}

// ── Priority 1: app./xdg./lib. file prefixes ────────────────

#[test]
fn app_prefix_routes_top_level_file_to_app_support_root() {
    // app.X is the per-file counterpart to the `_app/` directory
    // prefix. The rest of the filename deploys raw under
    // app_support_dir, with no pack namespacing.
    let config = HandlerConfig::default();
    let target = resolve_target("vscode", "app.settings.json", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/Library/Application Support/settings.json")
    );
}

#[test]
fn xdg_prefix_routes_top_level_file_to_xdg_root() {
    // xdg.X is the per-file counterpart to the `_xdg/` directory
    // prefix — same skip-pack-namespace semantics.
    let config = HandlerConfig::default();
    let target = resolve_target("desktop", "xdg.mimeapps.list", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/mimeapps.list"));
}

#[test]
fn lib_prefix_routes_top_level_file_to_library_on_macos() {
    // lib.X is the per-file counterpart to the `_lib/` directory
    // prefix. macOS-only; non-macOS hosts surface a Skip and a
    // soft warning, parallel to `_lib/`.
    let config = HandlerConfig::default();
    let resolution = resolve_target_full(
        "macapps",
        "lib.com.example.foo.plist",
        &config,
        &test_pather(),
    );
    if cfg!(target_os = "macos") {
        match resolution {
            Resolution::Path(p) => assert_eq!(
                p,
                PathBuf::from("/home/alice/Library/com.example.foo.plist")
            ),
            Resolution::Skip { reason } => {
                panic!("expected Path on macOS, got Skip({reason})")
            }
        }
    } else {
        assert!(
            matches!(resolution, Resolution::Skip { .. }),
            "lib.X on non-macOS must skip; got {resolution:?}"
        );
    }
}

#[test]
fn file_prefixes_only_apply_at_top_level() {
    // Nested `app.X` / `xdg.X` / `lib.X` paths keep the prefix
    // literal — only top-level files opt in (parallel to home.X).
    let config = HandlerConfig::default();
    for name in ["app.settings.json", "xdg.mimeapps.list", "lib.foo.plist"] {
        let nested = format!("subdir/{name}");
        let target = resolve_target("misc", &nested, &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from(format!("/home/alice/.config/misc/{nested}")),
            "nested `{name}` must keep prefix literal"
        );
    }
}

#[test]
fn lib_file_prefix_emits_warning_on_non_macos() {
    // The handler's `warnings_for_matches` surfaces the same
    // macOS-only soft notice for `lib.X` files as it does for the
    // `_lib/` directory prefix on every non-macOS host.
    let env = crate::testing::TempEnvironment::builder()
        .pack("macapps")
        .file("lib.com.example.foo.plist", "# stub plist")
        .done()
        .build();

    let m = RuleMatch {
        relative_path: PathBuf::from("lib.com.example.foo.plist"),
        absolute_path: env.dotfiles_root.join("macapps/lib.com.example.foo.plist"),
        pack: "macapps".into(),
        handler: HANDLER_SYMLINK.into(),
        is_dir: false,
        options: std::collections::HashMap::new(),
        preprocessor_source: None,
        rendered_bytes: None,
    };
    let handler = SymlinkHandler;
    let config = HandlerConfig::default();
    let warnings =
        handler.warnings_for_matches(std::slice::from_ref(&m), &config, env.paths.as_ref());

    if cfg!(target_os = "macos") {
        assert!(
            warnings.is_empty(),
            "lib.X should not warn on macOS; got {warnings:?}"
        );
        let intents = handler
            .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
            .unwrap();
        assert_eq!(intents.len(), 1);
    } else {
        assert_eq!(warnings.len(), 1, "expected one warning, got {warnings:?}");
        assert!(
            warnings[0].contains("macOS-only path"),
            "warning text should mention macOS-only: {warnings:?}"
        );
        let intents = handler
            .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
            .unwrap();
        assert!(
            intents.is_empty(),
            "lib.X on non-macOS must not emit Link intents: {intents:?}"
        );
    }
}
