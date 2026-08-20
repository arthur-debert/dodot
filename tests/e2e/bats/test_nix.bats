#!/usr/bin/env bats
# E2E tests for the nix handler against a stubbed `nix` binary.
#
# The stub (helpers/nix_stub.bash) covers the single subcommand the
# handler invokes — `nix profile install ... --argstr manifest
# <path> --expr <wrapper>` — and logs each install for assertion.
# Real Nix is out of scope for the bats suite (no nix binary in CI;
# expensive setup); tier-0 unit tests in
# `crates/dodot-lib/src/handlers/nix.rs` cover argv construction and
# the wrapper expression itself, and the wrapper was executed against
# Nix 2.35.2 in a `nixos/nix` container — see
# `docs/adr/0006-declare-the-manifest-argument-never-infer-it.md`.

setup() {
    load helpers/setup
    sandbox_setup
    load helpers/nix_stub
    nix_stub_setup
}

teardown() {
    sandbox_teardown
}

@test "packages.nix triggers nix profile install on first up" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep fd ]'

    dodot up

    [ "$(nix_stub_install_count)" = "1" ]
    assert_sentinel_exists "tools" "nix" "packages.nix-*"
}

@test "second up does not re-run when manifest is unchanged" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'

    dodot up
    dodot up

    # One install total — the second up saw the matching sentinel and
    # short-circuited.
    [ "$(nix_stub_install_count)" = "1" ]
}

@test "edited manifest reports older version and does not auto-rerun" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'
    dodot up

    # Edit the manifest — same shape, different content hash.
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep fd ]'

    run dodot status tools
    assert_output_contains "older version"

    dodot up
    # Still one install — notify-don't-rerun policy held.
    [ "$(nix_stub_install_count)" = "1" ]
}

@test "--provision-rerun applies the edited manifest" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'
    dodot up

    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep fd ]'
    dodot up --provision-rerun

    [ "$(nix_stub_install_count)" = "2" ]
}

@test "status reports three states across the lifecycle" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'

    run dodot status tools
    assert_output_contains "nix packages not installed"

    dodot up
    run dodot status tools
    assert_output_contains "nix packages installed"

    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep fd ]'
    run dodot status tools
    assert_output_contains "nix packages older version"
}

@test "up --no-provision skips packages.nix" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'

    dodot up --no-provision

    [ "$(nix_stub_install_count)" = "0" ]
    run dodot status tools
    assert_output_contains "nix packages not installed"
}

@test "attribute-set manifest installs via the wrapper expression" {
    # The wrapper expression collapses list / drv / attrset shapes
    # to a single list of derivations before installing, so every
    # manifest shape goes through the same `nix profile install
    # --expr` invocation. No planning-time gatekeeping by shape —
    # see the RunOnceCommand lifecycle-invariant note.
    create_pack_file "tools" "packages.nix" \
        '{ pkgs ? import <nixpkgs> {} }: { ripgrep = pkgs.ripgrep; fd = pkgs.fd; }'

    dodot up

    [ "$(nix_stub_install_count)" = "1" ]
    assert_sentinel_exists "tools" "nix" "packages.nix-*"
}

@test "broken-edit of previously-installed manifest surfaces as older version" {
    # Mirrors the install / homebrew lifecycle: a previously-run
    # file edited into something the underlying tool may dislike
    # is still reported as 'older version' by status — dodot does
    # NOT gatekeep planning on content. Any apply-time failure
    # would come from nix at --provision-rerun, not from dodot
    # refusing to plan.
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'
    dodot up
    [ "$(nix_stub_install_count)" = "1" ]

    # Edit the manifest into something a real nix would likely
    # reject (a non-derivation string at the top level). dodot
    # should still see it as 'older version', not fail planning.
    create_pack_file "tools" "packages.nix" '"hello"'

    run dodot status tools
    assert_output_contains "older version"

    # And `dodot up` (without --provision-rerun) does not re-run.
    dodot up
    [ "$(nix_stub_install_count)" = "1" ]
}

@test "a nix run names the manifest on the command line" {
    # The manifest reaches nix as the value of `--argstr manifest`,
    # not embedded in the wrapper expression — which is what lets
    # dodot name the file it ran without parsing Nix source.
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'

    dodot up

    [ "$(nix_stub_last_manifest)" = "$DOTFILES_ROOT/tools/packages.nix" ]
}

@test "a nix run writes a snapshot holding the manifest bytes" {
    # Regression: nix's command ends in `--extra-experimental-features
    # 'nix-command flakes'`, so snapshotting the last argument
    # snapshotted nothing at all and left `status --diff` unavailable
    # for packages.nix.
    local manifest='{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'
    create_pack_file "tools" "packages.nix" "$manifest"

    dodot up

    local snapshot
    snapshot="$(find "$XDG_DATA_HOME/dodot/packs/tools/nix" -maxdepth 1 -name 'packages.nix-*.snapshot')"
    [ -n "$snapshot" ] || {
        echo "no snapshot sibling next to the nix sentinel" >&2
        ls -la "$XDG_DATA_HOME/dodot/packs/tools/nix" >&2
        false
    }
    [ "$(cat "$snapshot")" = "$manifest" ]
}

@test "the progress header names the manifest, not the nix flags" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'

    run dodot up

    assert_output_contains "tools → nix → packages.nix"
    assert_output_not_contains "nix-command flakes"
}

@test "status --diff shows what changed in an edited packages.nix" {
    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep ]'
    dodot up

    create_pack_file "tools" "packages.nix" '{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep fd ]'

    run dodot status tools --diff

    # The snapshot is the previously-run manifest, so the diff has
    # both sides — and the row counts the change rather than
    # reporting `no diff data`.
    assert_output_contains "packages.nix"
    assert_output_contains "ripgrep fd"
    assert_output_not_contains "no diff data"
}
