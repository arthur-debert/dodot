//! Scanner tests.
//!
//! Shared fixtures (`make_pack`, `test_gates`, `host_pair`,
//! `default_rules`) and the scanner integration tests live here.
//! Gate-specific suites are sibling files: filename gates, directory
//! gates (C2), and `[mappings.gates]` glob gates (C4).

#![allow(unused_imports)]

mod dir_gates;
mod gates;
mod mapping_gates;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::fs::Fs;
use crate::gates::{GateTable, HostFacts};
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::rules::{Rule, Scanner};
use crate::testing::TempEnvironment;

pub(super) fn make_pack(name: &str, path: PathBuf) -> Pack {
    Pack::new(name.into(), path, HandlerConfig::default())
}

/// Stable test fixture: built-in gates + a darwin/aarch64 host.
/// Scanner tests that don't care about gates pin a stable host so
/// any incidental `._darwin.*` filename behaves identically.
pub(super) fn test_gates() -> (GateTable, HostFacts) {
    (
        GateTable::with_builtins(),
        HostFacts::for_tests("darwin", "aarch64"),
    )
}

pub(super) fn host_pair(os: &str, arch: &str) -> (GateTable, HostFacts) {
    (GateTable::with_builtins(), HostFacts::for_tests(os, arch))
}

pub(super) fn default_rules() -> Vec<Rule> {
    // Representative subset of the production rules emitted by
    // `config::mappings_to_rules`. Covers the priority ladder
    // (install=20, shell glob=10, catchall=0) so scanner tests
    // exercise the relative ordering, but intentionally omits
    // multiple install/shell extensions, the gates map, and the
    // ignore/skip defaults — those have their own dedicated
    // tests in `config::tests`.
    vec![
        Rule {
            pattern: "bin/".into(),
            handler: "path".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "install.sh".into(),
            handler: "install".into(),
            priority: 20,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*.sh".into(),
            handler: "shell".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*.bash".into(),
            handler: "shell".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*.zsh".into(),
            handler: "shell".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "Brewfile".into(),
            handler: "homebrew".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*".into(),
            handler: "symlink".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        },
    ]
}

// ── Scanner integration tests ───────────────────────────────

#[test]
fn scan_pack_basic() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .file("gvimrc", "set guifont=Mono")
        .file("aliases.sh", "alias vi=vim")
        .file("install.sh", "#!/bin/sh\necho setup")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("vim", env.dotfiles_root.join("vim"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    let handler_map: HashMap<String, Vec<String>> = {
        let mut m: HashMap<String, Vec<String>> = HashMap::new();
        for rm in &matches {
            m.entry(rm.handler.clone())
                .or_default()
                .push(rm.relative_path.to_string_lossy().to_string());
        }
        m
    };

    assert_eq!(handler_map["install"], vec!["install.sh"]);
    assert_eq!(handler_map["shell"], vec!["aliases.sh"]);
    assert!(handler_map["symlink"].contains(&"gvimrc".to_string()));
    assert!(handler_map["symlink"].contains(&"vimrc".to_string()));
}

#[test]
fn scan_pack_skips_hidden_files() {
    let env = TempEnvironment::builder()
        .pack("test")
        .file("visible", "yes")
        .file(".hidden", "no")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();
    let names: Vec<String> = matches
        .iter()
        .map(|m| m.relative_path.to_string_lossy().to_string())
        .collect();

    assert!(names.contains(&"visible".to_string()));
    assert!(!names.contains(&".hidden".to_string()));
}

#[test]
fn scan_pack_skips_special_files() {
    let env = TempEnvironment::builder()
        .pack("test")
        .file("normal", "yes")
        .config("[pack]\nignore = []")
        .done()
        .build();

    // Also manually create .dodotignore (even though it shouldn't be scanned)
    let pack_dir = env.dotfiles_root.join("test");
    env.fs
        .write_file(&pack_dir.join(".dodotignore"), b"")
        .unwrap();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", pack_dir);
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();
    let names: Vec<String> = matches
        .iter()
        .map(|m| m.relative_path.to_string_lossy().to_string())
        .collect();

    assert!(names.contains(&"normal".to_string()));
    assert!(!names.contains(&".dodot.toml".to_string()));
    assert!(!names.contains(&".dodotignore".to_string()));
}

#[test]
fn scan_pack_with_ignore_patterns() {
    let env = TempEnvironment::builder()
        .pack("test")
        .file("keep.txt", "yes")
        .file("skip.bak", "no")
        .file("other.bak", "no")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(
            &pack,
            &rules,
            &["*.bak".to_string()],
            &gates,
            &host,
            &HashMap::new(),
        )
        .unwrap();
    let names: Vec<String> = matches
        .iter()
        .map(|m| m.relative_path.to_string_lossy().to_string())
        .collect();

    assert!(names.contains(&"keep.txt".to_string()));
    assert!(!names.contains(&"skip.bak".to_string()));
    assert!(!names.contains(&"other.bak".to_string()));
}

#[test]
fn scan_pack_ignore_rule_outranks_catchall() {
    // The ignore filter handler at priority 100 wins over the
    // priority-0 catchall, and emits a match with handler="ignore".
    // Status display filters those out before rendering, but at the
    // matcher level they ARE matches — the catchall must not also
    // claim them.
    let env = TempEnvironment::builder()
        .pack("test")
        .file("good.txt", "yes")
        .file("bad.tmp", "no")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));

    let rules = vec![
        Rule {
            pattern: "*.tmp".into(),
            handler: "ignore".into(),
            priority: 100,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*".into(),
            handler: "symlink".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        },
    ];

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    let bad = matches
        .iter()
        .find(|m| m.relative_path.to_string_lossy() == "bad.tmp")
        .expect("bad.tmp must still appear as a match");
    assert_eq!(bad.handler, "ignore");

    let good = matches
        .iter()
        .find(|m| m.relative_path.to_string_lossy() == "good.txt")
        .expect("good.txt must appear as a match");
    assert_eq!(good.handler, "symlink");
}

#[test]
fn scan_pack_priority_ordering() {
    let env = TempEnvironment::builder()
        .pack("test")
        .file("aliases.sh", "# shell")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));

    // Both *.sh and aliases.sh match — higher priority should win
    let rules = vec![
        Rule {
            pattern: "*.sh".into(),
            handler: "generic-shell".into(),
            priority: 5,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "aliases.sh".into(),
            handler: "specific-shell".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*".into(),
            handler: "symlink".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        },
    ];

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "specific-shell");
}

#[test]
fn skip_handler_matches_case_insensitively() {
    // Note: case-only filename variants like "README" and "Readme"
    // collide on case-insensitive filesystems (macOS HFS+/APFS
    // default), so the fixture uses different basenames + casings
    // to prove the match logic works across casings.
    let env = TempEnvironment::builder()
        .pack("test")
        .file("README", "x")
        .file("readme.md", "x")
        .file("License.txt", "x")
        .file("notes.md", "x")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));

    let rules = vec![
        Rule {
            pattern: "README".into(),
            handler: "skip".into(),
            priority: 50,
            case_insensitive: true,
            options: HashMap::new(),
        },
        Rule {
            pattern: "README.*".into(),
            handler: "skip".into(),
            priority: 50,
            case_insensitive: true,
            options: HashMap::new(),
        },
        Rule {
            pattern: "LICENSE.*".into(),
            handler: "skip".into(),
            priority: 50,
            case_insensitive: true,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*".into(),
            handler: "symlink".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        },
    ];

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();
    let by_handler: std::collections::HashMap<&str, Vec<&str>> =
        matches.iter().fold(Default::default(), |mut acc, m| {
            acc.entry(m.handler.as_str())
                .or_default()
                .push(m.relative_path.to_str().unwrap());
            acc
        });

    let mut skipped = by_handler.get("skip").cloned().unwrap_or_default();
    skipped.sort();
    assert_eq!(skipped, vec!["License.txt", "README", "readme.md"]);

    let symlinked = by_handler.get("symlink").cloned().unwrap_or_default();
    assert_eq!(symlinked, vec!["notes.md"]);
}

#[test]
fn skip_handler_outranks_precise_handler() {
    // Priority 50 (skip) must beat the priority 10 mappings routes
    // — otherwise a `mappings.shell = ["README.sh"]` mistake would
    // silently source a README as a shell file.
    let env = TempEnvironment::builder()
        .pack("test")
        .file("README.sh", "x")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));

    let rules = vec![
        Rule {
            pattern: "README.*".into(),
            handler: "skip".into(),
            priority: 50,
            case_insensitive: true,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*.sh".into(),
            handler: "shell".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        },
    ];

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "skip");
}

#[test]
fn ignore_rule_outranks_skip() {
    // mappings.ignore (priority 100) must win over mappings.skip
    // (priority 50) — silent-drop is the stronger signal than
    // listed-but-not-acted-on.
    let env = TempEnvironment::builder()
        .pack("test")
        .file("README.md", "x")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));

    let rules = vec![
        Rule {
            pattern: "README.md".into(),
            handler: "ignore".into(),
            priority: 100,
            case_insensitive: false,
            options: HashMap::new(),
        },
        Rule {
            pattern: "README.*".into(),
            handler: "skip".into(),
            priority: 50,
            case_insensitive: true,
            options: HashMap::new(),
        },
        Rule {
            pattern: "*".into(),
            handler: "symlink".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        },
    ];

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "ignore");
}

#[test]
fn scan_pack_directory_entry() {
    let env = TempEnvironment::builder()
        .pack("test")
        .file("bin/my-script", "#!/bin/sh")
        .file("normal", "x")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("test", env.dotfiles_root.join("test"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    let bin_match = matches
        .iter()
        .find(|m| m.relative_path.to_string_lossy() == "bin");
    assert!(bin_match.is_some(), "bin directory should match");
    assert_eq!(bin_match.unwrap().handler, "path");
    assert!(bin_match.unwrap().is_dir);
}

#[test]
fn nested_install_sh_is_not_matched_by_install_rule() {
    // A file named install.sh that lives deep inside a directory
    // must NOT activate the install handler. Only a top-level
    // install.sh triggers it.
    let env = TempEnvironment::builder()
        .pack("sneaky")
        .file("config/install.sh", "echo boom")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("sneaky", env.dotfiles_root.join("sneaky"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    assert!(
        !matches.iter().any(|m| m.handler == "install"),
        "nested install.sh should not route to install handler: {matches:?}"
    );
}

/// Wildcard shell defaults: any `*.{sh,bash,zsh}` at the pack
/// root routes to the shell handler, not just the legacy
/// `aliases`/`profile`/`login`/`env` allowlist. Pack authors don't
/// have to rename `path.sh`, `functions.zsh`, or `50_prompt.bash`
/// to a fixed allowlist to get them sourced.
#[test]
fn shell_glob_defaults_source_arbitrary_names_at_pack_root() {
    let env = TempEnvironment::builder()
        .pack("shell")
        .file("path.sh", "export PATH=...")
        .file("functions.zsh", "function f() {}")
        .file("50_prompt.bash", "PS1='>'")
        .file("aliases.sh", "alias x=y")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("shell", env.dotfiles_root.join("shell"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    let mut shell_files: Vec<String> = matches
        .iter()
        .filter(|m| m.handler == "shell")
        .map(|m| m.relative_path.to_string_lossy().to_string())
        .collect();
    shell_files.sort();
    assert_eq!(
        shell_files,
        vec!["50_prompt.bash", "aliases.sh", "functions.zsh", "path.sh",],
        "all *.{{sh,bash,zsh}} at pack root should source: {matches:?}"
    );
}

/// install.sh wins over the priority-10 `*.sh` shell glob — the
/// install rule sits at priority 20 specifically so the install
/// hook never gets accidentally sourced. Without this, dodot would
/// silently turn the user's one-shot install script into something
/// that runs every shell startup.
#[test]
fn shell_glob_defaults_dont_steal_install_sh() {
    let env = TempEnvironment::builder()
        .pack("toolchain")
        .file("install.sh", "#!/bin/sh\necho setup")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("toolchain", env.dotfiles_root.join("toolchain"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "install");
    assert_eq!(matches[0].relative_path.to_string_lossy(), "install.sh");
}

/// Skip rules sit at priority 50, above the priority-10 shell
/// glob — a `README.sh` (unlikely but possible) is still skipped
/// rather than sourced as a shell file. Defaults ship with the
/// `README.*` skip pattern, so this is the realistic configuration.
#[test]
fn shell_glob_defaults_dont_override_skip_rules() {
    let env = TempEnvironment::builder()
        .pack("docs")
        .file("README.sh", "this should be skipped, not sourced")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("docs", env.dotfiles_root.join("docs"));
    let mut rules = default_rules();
    rules.push(Rule {
        pattern: "README.*".into(),
        handler: crate::handlers::HANDLER_SKIP.into(),
        priority: 50,
        case_insensitive: true,
        options: HashMap::new(),
    });

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, crate::handlers::HANDLER_SKIP);
}

/// `*.sh.tmpl` (a template file) does NOT match `*.sh` — the glob
/// only matches names ending in `.sh`. Templates pass through to
/// the catchall, where the preprocessor picks them up and rewrites
/// the rendered match before dispatch.
#[test]
fn shell_glob_does_not_match_template_extension() {
    let env = TempEnvironment::builder()
        .pack("p")
        .file("aliases.sh.tmpl", "alias x={{.var}}")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "symlink");
    assert_eq!(
        matches[0].relative_path.to_string_lossy(),
        "aliases.sh.tmpl"
    );
}

/// Recursion safety: a nested `~/.config/<wm>/scripts/foo.sh`
/// (window-manager helper script invoked by another tool, not
/// sourced into the shell) must NOT route to the shell handler.
/// The depth-1 invariant ensures only the top-level `wmconf/`
/// directory is matched; the nested .sh stays inside the
/// directory tree the symlink handler manages.
#[test]
fn shell_glob_does_not_recurse_into_subdirectories() {
    let env = TempEnvironment::builder()
        .pack("hypr")
        .file("hypr.conf", "# config")
        .file("scripts/workspace-switch.sh", "#!/bin/sh\nhyprctl ...")
        .file("scripts/launcher.sh", "#!/bin/sh\nrofi -show drun")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("hypr", env.dotfiles_root.join("hypr"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    // No nested entry should surface — the scanner is depth-1.
    assert!(
        !matches
            .iter()
            .any(|m| m.relative_path.to_string_lossy().contains('/')),
        "no nested matches expected: {matches:?}"
    );
    // No shell-handler match should exist at all — the `scripts/`
    // dir is matched as a dir and falls to the symlink catchall.
    assert!(
        !matches.iter().any(|m| m.handler == "shell"),
        "nested scripts must not route to shell: {matches:?}"
    );
}

#[test]
fn scan_pack_returns_only_top_level_entries() {
    // Under the top-level-only scanner, nested files are not surfaced
    // as individual matches. The containing dir is the matched entry;
    // handlers (symlink wholesale, path, …) decide how to recurse.
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("nvim/init.lua", "require('config')")
        .file("nvim/lua/plugins.lua", "return {}")
        .done()
        .build();

    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("nvim", env.dotfiles_root.join("nvim"));
    let rules = default_rules();

    let (gates, host) = test_gates();
    let matches = scanner
        .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
        .unwrap();

    let relpaths: Vec<String> = matches
        .iter()
        .map(|m| m.relative_path.to_string_lossy().to_string())
        .collect();

    assert!(
        relpaths.iter().any(|p| p == "nvim"),
        "top-level nvim dir should match: {relpaths:?}"
    );
    assert!(
        !relpaths.iter().any(|p| p.contains('/')),
        "no nested paths expected: {relpaths:?}"
    );
}
