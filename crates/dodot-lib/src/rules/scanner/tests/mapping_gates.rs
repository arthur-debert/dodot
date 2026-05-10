//! C4: [mappings.gates] glob-based gating — gate by glob pattern
//! against a relative path, configured at the rule layer.

#![allow(unused_imports)]

use std::collections::HashMap;

use crate::gates::{GateTable, HostFacts};
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::rules::{Rule, Scanner};
use crate::testing::TempEnvironment;

use super::{default_rules, host_pair, make_pack};

// ── C4: [mappings.gates] glob-based gating ─────────────────

#[test]
fn mappings_gate_failing_drops_file() {
    // [mappings.gates] = { "install-mac.sh" = "linux" } on darwin →
    // file is gated out.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("install-mac.sh", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let mut mappings_gates = HashMap::new();
    mappings_gates.insert("install-mac.sh".to_string(), "linux".to_string());

    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
        .unwrap();
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.handler, crate::handlers::HANDLER_GATE);
    assert_eq!(m.options.get("gate_label"), Some(&"linux".to_string()));
}

#[test]
fn mappings_gate_passing_does_not_alter_dispatch() {
    // [mappings.gates] = { "config-mac.toml" = "darwin" } on darwin →
    // file passes; rule matching proceeds as if no gate were set.
    // config-mac.toml isn't in any default mapping so it falls
    // through to the catchall symlink — same as without the gate.
    // (We use a non-shell extension here so the wildcard `*.sh`
    // shell default doesn't claim the file; this test is about
    // gate semantics, not handler routing.)
    let env = TempEnvironment::builder()
        .pack("p")
        .file("config-mac.toml", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let mut mappings_gates = HashMap::new();
    mappings_gates.insert("config-mac.toml".to_string(), "darwin".to_string());

    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
        .unwrap();
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.handler, "symlink");
    assert_eq!(m.relative_path.to_string_lossy(), "config-mac.toml");
}

#[test]
fn mappings_gate_glob_matches_subpath() {
    // [mappings.gates] = { "setup/*.sh" = "linux" } on darwin →
    // setup/foo.sh is gated out.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("setup/foo.sh", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let mut mappings_gates = HashMap::new();
    mappings_gates.insert("setup/*.sh".to_string(), "linux".to_string());

    let matches = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
        .unwrap();
    // Top-level "setup" dir is what surfaces from walk_pack (per
    // the existing depth-1 contract). The mapping pattern matches
    // its child path "setup/foo.sh" — but the scanner only sees
    // "setup" as a top-level dir and the mapping doesn't match
    // that. So this test documents the C4 limit: globs are
    // matched against the relative path the scanner surfaces, not
    // against arbitrary nested paths the symlink handler would
    // recurse into.
    //
    // For the C4 v1 surface, glob-based gating works on top-level
    // entries. Files inside subdirectories deploy via the symlink
    // handler's wholesale link of the parent.
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].handler, "symlink");
    assert_eq!(matches[0].relative_path.to_string_lossy(), "setup");
}

#[test]
fn mappings_gate_conflict_with_basename_gate_errors() {
    // File has BOTH a filename gate (`._darwin`) AND a
    // [mappings.gates] entry → hard error.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("install._darwin.sh", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let mut mappings_gates = HashMap::new();
    mappings_gates.insert("install._darwin.sh".to_string(), "linux".to_string());

    let err = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("gate-routing conflict"), "{msg}");
    assert!(msg.contains("install._darwin.sh"), "{msg}");
}

#[test]
fn mappings_gate_unknown_label_errors() {
    let env = TempEnvironment::builder()
        .pack("p")
        .file("foo.sh", "x")
        .done()
        .build();
    let scanner = Scanner::new(env.fs.as_ref());
    let pack = make_pack("p", env.dotfiles_root.join("p"));
    let (gates, host) = host_pair("darwin", "aarch64");
    let mut mappings_gates = HashMap::new();
    mappings_gates.insert("foo.sh".to_string(), "darwn".to_string()); // typo

    let err = scanner
        .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("darwn"), "{msg}");
    assert!(msg.contains("[mappings.gates]"), "{msg}");
}
