//! C2: directory-segment gates (`_<label>/`) — when a top-level
//! directory is itself a gate.

#![allow(unused_imports)]

use std::collections::HashMap;

use crate::gates::{GateTable, HostFacts};
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::rules::{Rule, Scanner};
use crate::testing::TempEnvironment;

use super::{default_rules, host_pair, make_pack};

// ── C2: directory-segment gates ─────────────────────────────

#[test]
fn dir_gate_passing_descends_and_flattens() {
    // _darwin/foo.sh on darwin → surfaces as foo.sh at pack root,
    // gate dir is transparent.
    let env = TempEnvironment::builder()
        .pack("cross")
        .file("_darwin/macos.sh", "#!/bin/sh\necho mac")
        .file("shared", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("cross", env.dotfiles_root.join("cross"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    let names: Vec<String> = matches
        .iter()
        .map(|m| m.relative_path.to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"macos.sh".to_string()), "{names:?}");
    assert!(names.contains(&"shared".to_string()), "{names:?}");
    // The gate dir itself must NOT surface as an entry.
    assert!(!names.iter().any(|n| n.starts_with("_darwin")), "{names:?}");
}

#[test]
fn dir_gate_failing_emits_gate_match() {
    // _linux/ on darwin → surfaces as a single gate-handler match
    // for the directory; its contents are not surfaced.
    let env = TempEnvironment::builder()
        .pack("cross")
        .file("_linux/linux.sh", "#!/bin/sh\necho linux")
        .file("shared", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("cross", env.dotfiles_root.join("cross"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();

    // Exactly two matches: the gate dir (failed) and `shared`.
    assert_eq!(matches.len(), 2, "{matches:?}");

    let gate_match = matches
        .iter()
        .find(|m| m.handler == crate::handlers::HANDLER_GATE)
        .expect("expected gate match");
    assert_eq!(gate_match.relative_path.to_string_lossy(), "_linux");
    assert!(gate_match.is_dir);
    assert_eq!(
        gate_match.options.get("gate_label"),
        Some(&"linux".to_string())
    );

    let shared = matches
        .iter()
        .find(|m| m.relative_path.to_string_lossy() == "shared")
        .expect("expected shared file");
    assert_eq!(shared.handler, "symlink");
}

#[test]
fn dir_gate_routing_prefix_is_not_a_gate() {
    // _home/ is a routing-prefix dir, NOT a gate. The scanner
    // surfaces it as a regular top-level dir; the symlink handler
    // takes it from there.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("_home/.bashrc", "# bashrc")
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
    assert_eq!(m.relative_path.to_string_lossy(), "_home");
    assert!(m.is_dir);
    assert_eq!(m.handler, "symlink");
}

#[test]
fn dir_gate_unknown_label_is_hard_error() {
    let env = TempEnvironment::builder()
        .pack("typo")
        .file("_darwn/foo.sh", "x") // typo: darwn
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
    assert!(msg.contains("_darwn"), "missing dir name: {msg}");
}

#[test]
fn dir_gate_nested_inside_passing_gate_still_evaluates() {
    // _darwin/_arm64/x.sh on darwin+aarch64 → both gates pass,
    // file surfaces as x.sh at pack root.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("_darwin/_arm64/install.sh", "x")
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
    assert_eq!(m.relative_path.to_string_lossy(), "install.sh");
    assert_eq!(m.handler, "install");
}

#[test]
fn dir_gate_nested_failing_inner_gate_drops_subtree() {
    // _darwin/_x86_64/install.sh on darwin+aarch64 → outer passes,
    // inner _x86_64 fails → that subtree is dropped (gate match
    // for the inner dir).
    let env = TempEnvironment::builder()
        .pack("p")
        .file("_darwin/_x86_64/install.sh", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
        .unwrap();
    // One gate match for the inner _x86_64 dir; install.sh does
    // NOT surface.
    let gate = matches
        .iter()
        .find(|m| m.handler == crate::handlers::HANDLER_GATE);
    assert!(gate.is_some(), "expected a gate match: {matches:?}");
    assert!(
        !matches
            .iter()
            .any(|m| m.relative_path.to_string_lossy() == "install.sh"),
        "install.sh must not deploy when its enclosing gate fails: {matches:?}"
    );
}

#[test]
fn dir_gate_with_routing_prefix_inside_passing_gate() {
    // The proposed pattern: _darwin/_home/.bashrc on darwin →
    // gate passes, descent surfaces _home/.bashrc as a top-level
    // routing-prefix subtree (which the symlink resolver handles
    // via priority 2a).
    let env = TempEnvironment::builder()
        .pack("p")
        .file("_darwin/_home/.bashrc", "# bashrc")
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
    // After gate strip, the entry surfaces under `_home` at pack
    // root level, exactly as if the user wrote `_home/` directly.
    assert_eq!(m.relative_path.to_string_lossy(), "_home");
    assert!(m.is_dir);
    assert_eq!(m.handler, "symlink");
}
