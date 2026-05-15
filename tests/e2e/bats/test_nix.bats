#!/usr/bin/env bats
# E2E tests for the nix handler against a stubbed `nix` binary.
#
# The stub (helpers/nix_stub.bash) covers the two `nix` subcommands
# the handler invokes — `nix eval --apply` for shape probing and
# `nix profile install --file` for installation. Real Nix is out of
# scope for the bats suite (no nix binary in CI; expensive setup);
# tier-0 unit tests in `crates/dodot-lib/src/handlers/nix.rs` cover
# the validator's own behavior and the run-once policy paths.

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

@test "attribute-set manifest is rejected with the v1 list-form workaround" {
    # `# stub-shape: set` makes the stub return "set" from `nix eval`
    # so the validator's per-shape rejection path fires.
    create_pack_file "tools" "packages.nix" '# stub-shape: set
{ pkgs ? import <nixpkgs> {} }: { ripgrep = pkgs.ripgrep; }'

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "attribute set"
    assert_output_contains "list form"

    [ "$(nix_stub_install_count)" = "0" ]
}

@test "unsupported manifest shape is rejected before install" {
    create_pack_file "tools" "packages.nix" '# stub-shape: unsupported
"hello"'

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "unsupported shape"

    [ "$(nix_stub_install_count)" = "0" ]
}
