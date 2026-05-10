//! Filename-gate tests: `._<label>` suffix on a basename.

#![allow(unused_imports)]

use std::collections::HashMap;

use crate::gates::{GateTable, HostFacts};
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::rules::{Rule, Scanner};
use crate::testing::TempEnvironment;

use super::{default_rules, host_pair, make_pack};

// ── Gate integration tests ──────────────────────────────────

#[test]
fn gate_passing_strips_suffix_and_routes_to_handler() {
    // install._darwin.sh on darwin → matches `install.sh` mapping,
    // routes to the install handler with stripped relative_path.
    let env = TempEnvironment::builder()
        .pack("mac")
        .file("install._darwin.sh", "#!/bin/sh\necho mac-only")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("mac", env.dotfiles_root.join("mac"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.handler, "install");
    assert_eq!(m.relative_path.to_string_lossy(), "install.sh");
    // absolute_path is the original on-disk file — install handler
    // executes the actual `install._darwin.sh` script.
    assert!(m
        .absolute_path
        .to_string_lossy()
        .ends_with("install._darwin.sh"));
}

#[test]
fn gate_failing_emits_gate_handler_match() {
    // install._linux.sh on darwin → gate fails, surfaces under "gate".
    let env = TempEnvironment::builder()
        .pack("cross")
        .file("install._linux.sh", "#!/bin/sh\napt-get foo")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("cross", env.dotfiles_root.join("cross"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.handler, crate::handlers::HANDLER_GATE);
    // Gated entries keep their original path (with the suffix) so
    // status can render the source name truthfully.
    assert_eq!(m.relative_path.to_string_lossy(), "install._linux.sh");
    // Metadata for the status renderer.
    assert_eq!(m.options.get("gate_label"), Some(&"linux".to_string()));
    assert_eq!(
        m.options.get("gate_predicate"),
        Some(&"os=linux".to_string())
    );
    assert_eq!(m.options.get("gate_host"), Some(&"os=darwin".to_string()));
}

#[test]
fn gate_unknown_label_is_hard_error() {
    let env = TempEnvironment::builder()
        .pack("typo")
        .file("install._darwn.sh", "#!/bin/sh") // typo: darwn
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("typo", env.dotfiles_root.join("typo"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let err = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("darwn"), "missing label: {msg}");
    assert!(msg.contains("typo"), "missing pack: {msg}");
    assert!(msg.contains("install._darwn.sh"), "missing file: {msg}");
}

#[test]
fn gate_compound_user_label_evaluates_and() {
    // arm-mac requires darwin AND aarch64. Pass only when both match.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("setup._arm-mac.sh", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));

    // Build a table with `arm-mac` defined.
    let mut user = HashMap::new();
    let mut arm_mac = HashMap::new();
    arm_mac.insert("os".into(), "darwin".into());
    arm_mac.insert("arch".into(), "aarch64".into());
    user.insert("arm-mac".into(), arm_mac);

    // Case 1: darwin + aarch64 → pass.
    let mut gates = GateTable::with_builtins();
    gates.merge_user(&user).unwrap();
    let host = HostFacts::for_tests("darwin", "aarch64");

    // setup._arm-mac.sh strips to setup.sh, which doesn't match any
    // precise rule, so it falls through to the catchall symlink.
    let mut rules = default_rules();
    rules.push(Rule {
        pattern: "setup.sh".into(),
        handler: "shell".into(),
        priority: 10,
        case_insensitive: false,
        options: HashMap::new(),
    });

    let matches = scanner
        .match_entries(
            &scanner.walk_pack(&pack.path, &[], &gates, &host).unwrap(),
            &rules,
            &pack.name,
            &gates,
            &host,
            &HashMap::new(),
        )
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "shell");
    assert_eq!(matches[0].relative_path.to_string_lossy(), "setup.sh");

    // Case 2: darwin + x86_64 → fail (arch mismatch).
    let host_intel = HostFacts::for_tests("darwin", "x86_64");
    let matches = scanner
        .match_entries(
            &scanner
                .walk_pack(&pack.path, &[], &gates, &host_intel)
                .unwrap(),
            &rules,
            &pack.name,
            &gates,
            &host_intel,
            &HashMap::new(),
        )
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, crate::handlers::HANDLER_GATE);
}

#[test]
fn gate_composes_with_template_extension() {
    // aliases._darwin.sh.tmpl → strips to aliases.sh.tmpl. The
    // template preprocessor still fires on the surviving entry; the
    // matcher itself just sees the .tmpl extension.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("aliases._darwin.sh.tmpl", "alias x=y")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    // .tmpl is preserved → preprocessor will pick it up.
    assert_eq!(m.relative_path.to_string_lossy(), "aliases.sh.tmpl");
    // Falls through to the catchall (symlink) since `aliases.sh.tmpl`
    // isn't in `mappings.shell`. In the real pipeline this match
    // would be replaced by the preprocessor's rendered output before
    // dispatch — that's not the scanner's concern.
    assert_eq!(m.handler, "symlink");
}

#[test]
fn gate_composes_with_home_routing_prefix() {
    // home.bashrc._darwin → strips to home.bashrc. The symlink
    // resolver then routes via the `home.X` priority-1 rule.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("home.bashrc._darwin", "# bashrc")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.relative_path.to_string_lossy(), "home.bashrc");
    assert_eq!(m.handler, "symlink");
}

#[test]
fn gate_mixed_files_in_one_pack() {
    // A pack with darwin-only, linux-only, and unconditional files.
    // On darwin: darwin file passes, linux file is gated out,
    // unconditional file passes.
    let env = TempEnvironment::builder()
        .pack("cross")
        .file("install._darwin.sh", "#!/bin/sh\necho mac")
        .file("install._linux.sh", "#!/bin/sh\necho linux")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("cross", env.dotfiles_root.join("cross"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();

    let by_handler: HashMap<&str, Vec<String>> =
        matches.iter().fold(HashMap::new(), |mut acc, m| {
            acc.entry(m.handler.as_str())
                .or_default()
                .push(m.relative_path.to_string_lossy().to_string());
            acc
        });

    // install._darwin.sh stripped to install.sh → install handler.
    assert_eq!(
        by_handler.get("install"),
        Some(&vec!["install.sh".to_string()])
    );
    // install._linux.sh kept as-is → gate handler.
    assert_eq!(
        by_handler.get(crate::handlers::HANDLER_GATE),
        Some(&vec!["install._linux.sh".to_string()])
    );
    // vimrc → catchall symlink.
    assert_eq!(by_handler.get("symlink"), Some(&vec!["vimrc".to_string()]));
}

#[test]
fn gate_brewfile_extensionless() {
    // Brewfile._darwin → strips to Brewfile, matches homebrew handler.
    let env = TempEnvironment::builder()
        .pack("brew")
        .file("Brewfile._darwin", "brew \"ripgrep\"")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("brew", env.dotfiles_root.join("brew"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "homebrew");
    assert_eq!(matches[0].relative_path.to_string_lossy(), "Brewfile");
}

#[test]
fn gate_arch_label_uses_arm64_alias() {
    // arm64 is an alias for aarch64; built-in.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("aliases._arm64.sh", "alias x=y")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.relative_path.to_string_lossy(), "aliases.sh");
    assert_eq!(m.handler, "shell");
}
