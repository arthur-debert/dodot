//! Integration-style tests for the symlink handler: routing-override
//! conflicts, custom `[symlink.targets]` overrides, wholesale-vs-
//! per-file directory behaviour, and the `_lib/` warning emission.

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

// ── Routing-override conflicts ──────────────────────────────

#[test]
fn targets_plus_home_prefix_is_a_conflict() {
    // `[symlink.targets]` and the `home.` filename prefix both
    // declare where one file goes — refuse to silently let one win.
    let mut config = HandlerConfig::default();
    config
        .targets
        .insert("home.bashrc".into(), "/etc/bashrc".into());
    let env = crate::testing::TempEnvironment::builder()
        .pack("shell")
        .file("home.bashrc", "# bash")
        .done()
        .build();
    let m = RuleMatch {
        relative_path: PathBuf::from("home.bashrc"),
        absolute_path: env.dotfiles_root.join("shell/home.bashrc"),
        pack: "shell".into(),
        handler: HANDLER_SYMLINK.into(),
        is_dir: false,
        options: std::collections::HashMap::new(),
        preprocessor_source: None,
        rendered_bytes: None,
    };
    let err = SymlinkHandler
        .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("home.bashrc"), "msg: {msg}");
    assert!(msg.contains("/etc/bashrc"), "msg: {msg}");
    assert!(msg.contains("shell"), "msg: {msg}");
}

#[test]
fn targets_plus_directory_prefix_is_a_conflict() {
    // Same conflict for subtree prefixes: `_xdg/foo` + a
    // `[symlink.targets]` entry for the same path errors out.
    let mut config = HandlerConfig::default();
    config
        .targets
        .insert("_xdg/ghostty/config".into(), "/etc/ghostty.conf".into());
    let env = crate::testing::TempEnvironment::builder()
        .pack("term")
        .file("_xdg/ghostty/config", "")
        .done()
        .build();
    let m = RuleMatch {
        relative_path: PathBuf::from("_xdg/ghostty/config"),
        absolute_path: env.dotfiles_root.join("term/_xdg/ghostty/config"),
        pack: "term".into(),
        handler: HANDLER_SYMLINK.into(),
        is_dir: false,
        options: std::collections::HashMap::new(),
        preprocessor_source: None,
        rendered_bytes: None,
    };
    let err = SymlinkHandler
        .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("_xdg/ghostty/config"), "msg: {msg}");
}

#[test]
fn targets_without_prefix_is_not_a_conflict() {
    // `[symlink.targets]` for a plain (non-prefixed) filename keeps
    // working — that's the canonical use case for the override.
    let mut config = HandlerConfig::default();
    config
        .targets
        .insert("misterious.conf".into(), "/var/etc/misterious.conf".into());
    let env = crate::testing::TempEnvironment::builder()
        .pack("etc")
        .file("misterious.conf", "")
        .done()
        .build();
    let m = RuleMatch {
        relative_path: PathBuf::from("misterious.conf"),
        absolute_path: env.dotfiles_root.join("etc/misterious.conf"),
        pack: "etc".into(),
        handler: HANDLER_SYMLINK.into(),
        is_dir: false,
        options: std::collections::HashMap::new(),
        preprocessor_source: None,
        rendered_bytes: None,
    };
    let intents = SymlinkHandler
        .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
        .expect("plain target overrides without prefix should resolve");
    assert_eq!(intents.len(), 1);
    if let HandlerIntent::Link { user_path, .. } = &intents[0] {
        assert_eq!(user_path, &PathBuf::from("/var/etc/misterious.conf"));
    }
}

#[test]
fn has_routing_prefix_unit() {
    // File-level prefixes
    assert!(has_routing_prefix("home.bashrc"));
    assert!(has_routing_prefix("app.settings.json"));
    assert!(has_routing_prefix("xdg.mimeapps.list"));
    assert!(has_routing_prefix("lib.com.example.plist"));
    // Subtree prefixes
    assert!(has_routing_prefix("_home/vimrc"));
    assert!(has_routing_prefix("_xdg/ghostty/config"));
    assert!(has_routing_prefix("_app/Code/User/settings.json"));
    assert!(has_routing_prefix("_lib/LaunchAgents/foo.plist"));
    // Bare prefix dir names too — the catchall scanner can match
    // those at the top level when nothing else inside them does.
    assert!(has_routing_prefix("_home"));
    assert!(has_routing_prefix("_app"));
    // Plain names — no prefix.
    assert!(!has_routing_prefix("vimrc"));
    assert!(!has_routing_prefix("subdir/home.conf"));
    // Empty-rest forms fall through, so they don't carry routing
    // intent for conflict purposes.
    assert!(!has_routing_prefix("home."));
    assert!(!has_routing_prefix("app."));
}

// ── Custom target overrides ─────────────────────────────────

#[test]
fn custom_target_absolute_path() {
    let mut config = HandlerConfig::default();
    config
        .targets
        .insert("misterious.conf".into(), "/var/etc/misterious.conf".into());

    let target = resolve_target("pack", "misterious.conf", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/var/etc/misterious.conf"));
}

#[test]
fn custom_target_relative_path() {
    let mut config = HandlerConfig::default();
    config.targets.insert(
        "home-bound.conf".into(),
        "my-documents/home-bound.conf".into(),
    );

    let target = resolve_target("pack", "home-bound.conf", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/.config/my-documents/home-bound.conf")
    );
}

#[test]
fn custom_target_overrides_all_layers() {
    // Even a force_home file can be overridden.
    let mut config = default_config();
    config
        .targets
        .insert("bashrc".into(), "/custom/bashrc".into());

    let target = resolve_target("shell", "bashrc", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/custom/bashrc"));
}

#[test]
fn no_custom_target_falls_through_to_pack_namespaced_default() {
    let config = HandlerConfig::default();
    let target = resolve_target("vim", "vimrc", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/vim/vimrc"));
}

// ── Wholesale vs per-file dir behavior ──────────────────────

fn build_dir_match(env: &crate::testing::TempEnvironment, pack: &str, dir: &str) -> RuleMatch {
    RuleMatch {
        relative_path: PathBuf::from(dir),
        absolute_path: env.dotfiles_root.join(pack).join(dir),
        pack: pack.into(),
        handler: HANDLER_SYMLINK.into(),
        is_dir: true,
        options: std::collections::HashMap::new(),
        preprocessor_source: None,
        rendered_bytes: None,
    }
}

#[test]
fn plain_top_level_dir_produces_single_wholesale_intent() {
    let env = crate::testing::TempEnvironment::builder()
        .pack("warp")
        .file("themes/nord.yaml", "a")
        .file("themes/vs_code.yaml", "b")
        .done()
        .build();
    let m = build_dir_match(&env, "warp", "themes");
    let handler = SymlinkHandler;
    let paths = crate::paths::XdgPather::builder()
        .home(&env.home)
        .dotfiles_root(&env.dotfiles_root)
        .build()
        .unwrap();
    let intents = handler
        .to_intents(&[m], &HandlerConfig::default(), &paths, env.fs.as_ref())
        .unwrap();
    assert_eq!(intents.len(), 1, "plain dir -> single wholesale intent");
    if let HandlerIntent::Link {
        source, user_path, ..
    } = &intents[0]
    {
        assert!(source.ends_with("warp/themes"));
        // Under #48, top-level dirs deploy under the pack's XDG dir.
        assert!(
            user_path.ends_with(".config/warp/themes"),
            "user_path={}",
            user_path.display()
        );
    } else {
        panic!("expected Link intent");
    }
}

#[test]
fn dir_with_protected_path_falls_back_to_per_file_and_skips_protected() {
    let env = crate::testing::TempEnvironment::builder()
        .pack("secret")
        .file("ssh/config", "Host *")
        .file("ssh/id_rsa", "DO NOT LINK")
        .done()
        .build();
    let m = build_dir_match(&env, "secret", "ssh");
    let handler = SymlinkHandler;
    let config = HandlerConfig {
        protected_paths: vec!["ssh/id_rsa".into()],
        force_home: vec!["ssh".into()],
        ..HandlerConfig::default()
    };
    let paths = crate::paths::XdgPather::builder()
        .home(&env.home)
        .dotfiles_root(&env.dotfiles_root)
        .build()
        .unwrap();
    let intents = handler
        .to_intents(&[m], &config, &paths, env.fs.as_ref())
        .unwrap();
    assert_eq!(
        intents.len(),
        1,
        "only ssh/config should be linked; id_rsa skipped. Got: {intents:?}"
    );
    if let HandlerIntent::Link {
        source, user_path, ..
    } = &intents[0]
    {
        assert!(source.ends_with("ssh/config"));
        // force_home=["ssh"] routes subdir config to $HOME/.ssh/config
        assert!(user_path.ends_with(".ssh/config"));
    } else {
        panic!("expected Link intent");
    }
}

#[test]
fn per_file_fallback_skips_special_and_pack_ignored_files() {
    // When per-file mode kicks in (because of a protected_path),
    // the recursion must apply the same filters the scanner uses:
    // dodot's own files and pack-ignore globs like `.DS_Store`.
    let env = crate::testing::TempEnvironment::builder()
        .pack("cfg")
        .file("ssh/config", "Host *")
        .file("ssh/id_rsa", "secret")
        .file("ssh/.DS_Store", "garbage")
        .file("ssh/.dodot.toml", "# pack config")
        .done()
        .build();
    let m = build_dir_match(&env, "cfg", "ssh");
    let handler = SymlinkHandler;
    let config = HandlerConfig {
        protected_paths: vec!["ssh/id_rsa".into()],
        pack_ignore: vec![".DS_Store".into()],
        ..HandlerConfig::default()
    };
    let paths = crate::paths::XdgPather::builder()
        .home(&env.home)
        .dotfiles_root(&env.dotfiles_root)
        .build()
        .unwrap();
    let intents = handler
        .to_intents(&[m], &config, &paths, env.fs.as_ref())
        .unwrap();
    assert_eq!(
        intents.len(),
        1,
        "only ssh/config should be linked. Got: {intents:?}"
    );
    if let HandlerIntent::Link { source, .. } = &intents[0] {
        assert!(source.ends_with("ssh/config"));
    }
}

// ── _lib/ warnings emission ─────────────────────────────────

#[test]
fn lib_prefix_emits_warning_on_non_macos() {
    // The symlink handler's `warnings_for_matches` surfaces the
    // soft "_lib/<rest> — macOS-only path, skipping" notice on
    // every non-macOS host. On macOS the rule resolves as a real
    // path, so the warnings list stays empty.
    let env = crate::testing::TempEnvironment::builder()
        .pack("macapps")
        .file("_lib/LaunchAgents/com.example.foo.plist", "# stub plist")
        .done()
        .build();

    let m = RuleMatch {
        relative_path: PathBuf::from("_lib/LaunchAgents/com.example.foo.plist"),
        absolute_path: env
            .dotfiles_root
            .join("macapps/_lib/LaunchAgents/com.example.foo.plist"),
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
            "_lib/ should not warn on macOS; got {warnings:?}"
        );
        // And the intent is generated as a real Link.
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
        // And the intent is *omitted* — `to_intents` skips it.
        let intents = handler
            .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
            .unwrap();
        assert!(
            intents.is_empty(),
            "_lib/ on non-macOS must not emit Link intents: {intents:?}"
        );
    }
}

#[test]
fn top_level_app_and_lib_dirs_force_per_file_mode() {
    // Regression: a top-level `_app` or `_lib` directory MUST NOT
    // be wholesale-linked — that would bake the prefix into the
    // deploy path (`~/.config/<pack>/_app/...`). Same per-file
    // forcing as `_home` and `_xdg`. Discovered by the bats e2e
    // suite for `_app/`.
    for prefix in ["_app", "_lib"] {
        let env = crate::testing::TempEnvironment::builder()
            .pack("macapps")
            .file(&format!("{prefix}/Code/x.json"), "x")
            .done()
            .build();
        let m = build_dir_match(&env, "macapps", prefix);
        let handler = SymlinkHandler;
        let intents = handler
            .to_intents(
                &[m],
                &HandlerConfig::default(),
                env.paths.as_ref(),
                env.fs.as_ref(),
            )
            .unwrap();
        // _lib/ on non-macOS produces no intent (skipped). On
        // macOS it produces 1 link. _app/ produces 1 link
        // everywhere (collapses to xdg on Linux but still emits).
        let expected = match prefix {
            "_lib" if !cfg!(target_os = "macos") => 0,
            _ => 1,
        };
        assert_eq!(
            intents.len(),
            expected,
            "prefix={prefix}: expected {expected} intents, got {intents:?}"
        );
        // The user_path should NOT contain the literal prefix —
        // the resolver stripped it. (Skip this check when no
        // intents were produced.)
        if let Some(HandlerIntent::Link { user_path, .. }) = intents.first() {
            assert!(
                !user_path.to_string_lossy().contains(&format!("/{prefix}/")),
                "prefix={prefix} leaked into deploy path: {}",
                user_path.display()
            );
        }
    }
}

#[test]
fn dir_with_targets_override_falls_back_to_per_file() {
    let env = crate::testing::TempEnvironment::builder()
        .pack("app")
        .file("config/main.toml", "x")
        .file("config/aux.toml", "y")
        .done()
        .build();
    let m = build_dir_match(&env, "app", "config");
    let handler = SymlinkHandler;
    let mut targets = std::collections::HashMap::new();
    targets.insert("config/main.toml".into(), "/etc/main.toml".into());
    let config = HandlerConfig {
        targets,
        ..HandlerConfig::default()
    };
    let paths = crate::paths::XdgPather::builder()
        .home(&env.home)
        .dotfiles_root(&env.dotfiles_root)
        .build()
        .unwrap();
    let intents = handler
        .to_intents(&[m], &config, &paths, env.fs.as_ref())
        .unwrap();
    // Both files should get per-file intents — targets override forces
    // per-file mode so main.toml gets the explicit path.
    assert_eq!(intents.len(), 2, "intents: {intents:?}");
    let main = intents
            .iter()
            .find(|i| matches!(i, HandlerIntent::Link { source, .. } if source.ends_with("config/main.toml")))
            .expect("main.toml intent");
    if let HandlerIntent::Link { user_path, .. } = main {
        assert_eq!(user_path, &PathBuf::from("/etc/main.toml"));
    }
}
