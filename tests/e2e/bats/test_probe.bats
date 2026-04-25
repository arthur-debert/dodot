#!/usr/bin/env bats
# E2E tests for `dodot probe` — deployment map and data-dir tree.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

# ── probe (summary) ─────────────────────────────────────────────

@test "probe lists available subcommands" {
    run dodot probe
    [ "$status" -eq 0 ]
    assert_output_contains "deployment-map"
    assert_output_contains "show-data-dir"
    assert_output_contains "data_dir"
}

@test "probe shows data_dir rooted under XDG_DATA_HOME" {
    run dodot probe
    [ "$status" -eq 0 ]
    assert_output_contains "$XDG_DATA_HOME/dodot"
}

# ── probe deployment-map ────────────────────────────────────────

@test "deployment-map on fresh install shows empty hint" {
    run dodot probe deployment-map
    [ "$status" -eq 0 ]
    assert_output_contains "nothing deployed"
}

@test "deployment-map lists entries after up" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    create_pack_file "vim" "home.vimrc" "set nocompatible"
    dodot up

    run dodot probe deployment-map
    [ "$status" -eq 0 ]
    assert_output_contains "vim"
    # shell handler (aliases.sh) and symlink handler (home.vimrc) should both show
    assert_output_contains "shell"
    assert_output_contains "symlink"
    assert_output_contains "aliases.sh"
}

@test "deployment-map TSV file is written alongside init script" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    assert_exists "$XDG_DATA_HOME/dodot/deployment-map.tsv"
    assert_file_contains "$XDG_DATA_HOME/dodot/deployment-map.tsv" "# dodot deployment map v1"
    assert_file_contains "$XDG_DATA_HOME/dodot/deployment-map.tsv" "vim.*shell.*symlink"
}

@test "deployment-map is refreshed on down" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up
    assert_file_contains "$XDG_DATA_HOME/dodot/deployment-map.tsv" "aliases.sh"

    dodot down

    # File still exists (header preserved), but no data rows.
    assert_exists "$XDG_DATA_HOME/dodot/deployment-map.tsv"
    assert_file_contains "$XDG_DATA_HOME/dodot/deployment-map.tsv" "# dodot deployment map v1"
    run grep -v '^#' "$XDG_DATA_HOME/dodot/deployment-map.tsv"
    # Only blank lines (if any) should remain.
    for line in $output; do
        [ -z "$(echo "$line" | tr -d '[:space:]')" ]
    done
}

@test "deployment-map points to the right TSV path" {
    run dodot probe deployment-map
    [ "$status" -eq 0 ]
    assert_output_contains "deployment-map.tsv"
}

# ── probe show-data-dir ─────────────────────────────────────────

@test "show-data-dir on fresh install renders the empty data_dir" {
    run dodot probe show-data-dir
    [ "$status" -eq 0 ]
    # Header is always there.
    assert_output_contains "Data directory"
    assert_output_contains "$XDG_DATA_HOME/dodot"
}

@test "show-data-dir displays packs tree after up" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    create_pack_bin "tools" "mytool" '#!/bin/sh
echo hi'
    dodot up

    run dodot probe show-data-dir
    [ "$status" -eq 0 ]
    assert_output_contains "packs"
    assert_output_contains "vim"
    assert_output_contains "shell"
    assert_output_contains "tools"
    assert_output_contains "path"
    # Tree should use box-drawing glyphs somewhere.
    [[ "$output" == *"├"* || "$output" == *"└"* ]]
}

@test "show-data-dir --depth 1 shows only immediate children" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    run dodot probe show-data-dir --depth 1
    [ "$status" -eq 0 ]
    # At depth 1 we see "packs" but not the vim pack inside.
    assert_output_contains "packs"
    assert_output_not_contains "vim"
}

@test "show-data-dir includes deployment-map.tsv after up" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    run dodot probe show-data-dir
    [ "$status" -eq 0 ]
    assert_output_contains "deployment-map.tsv"
}

# ── read-only safety ────────────────────────────────────────────

@test "probe never modifies the dotfiles root" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    # Snapshot: list of files + their contents hashes.
    local before
    before="$(cd "$DOTFILES_ROOT" && find . -type f | sort | xargs -I{} sh -c 'printf "%s %s\n" "{}" "$(cat "{}")"')"

    dodot probe
    dodot probe deployment-map
    dodot probe show-data-dir

    local after
    after="$(cd "$DOTFILES_ROOT" && find . -type f | sort | xargs -I{} sh -c 'printf "%s %s\n" "{}" "$(cat "{}")"')"

    [ "$before" = "$after" ]
}

# ── JSON output ─────────────────────────────────────────────────

@test "probe --output json emits kind-tagged JSON" {
    run dodot --output json probe
    [ "$status" -eq 0 ]
    assert_output_contains '"kind"'
    assert_output_contains '"summary"'
    assert_output_contains '"available"'
}

@test "probe deployment-map --output json emits entries array" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    run dodot --output json probe deployment-map
    [ "$status" -eq 0 ]
    assert_output_contains '"kind"'
    assert_output_contains '"deployment-map"'
    assert_output_contains '"entries"'
    assert_output_contains '"pack": "vim"'
}
