#!/usr/bin/env bats
# E2E tests for shell-init profiling (Phase 2):
# - Generated init script is bash/zsh-compatible and writes a profile file
# - `dodot probe shell-init` reads the latest profile back
# - Profiling can be disabled via config

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

# ── Profile file is written when a shell sources the init script ──

@test "bash sourcing dodot-init.sh writes a profile TSV" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    create_pack_bin "vim" "tool" '#!/bin/sh
echo hi'
    dodot up

    # Source the init script in a real bash subshell.
    bash -c ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\""
    [ "$?" -eq 0 ]

    # Exactly one profile file should have been written.
    local profiles_dir="$XDG_DATA_HOME/dodot/probes/shell-init"
    assert_dir_exists "$profiles_dir"
    local count
    count=$(find "$profiles_dir" -name 'profile-*.tsv' -type f | wc -l | tr -d ' ')
    [ "$count" = "1" ]
}

@test "profile contains preamble, one row per entry, and an end marker" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    create_pack_bin "vim" "tool" '#!/bin/sh
echo hi'
    dodot up

    bash -c ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\""

    local profile
    profile=$(find "$XDG_DATA_HOME/dodot/probes/shell-init" -name 'profile-*.tsv' -type f | head -1)
    [ -n "$profile" ]

    assert_file_contains "$profile" "# dodot shell-init profile v1"
    assert_file_contains "$profile" "# shell"
    assert_file_contains "$profile" "# start_t"
    assert_file_contains "$profile" "# end_t"
    # One PATH row + one source row.
    assert_file_contains "$profile" "^path	vim	path"
    assert_file_contains "$profile" "^source	vim	shell"
}

@test "concurrent shells get distinct profile files" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    # Three sub-shells in quick succession — distinct PIDs + RANDOM
    # should keep filenames unique even within the same EPOCHSECONDS.
    bash -c ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\""
    bash -c ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\""
    bash -c ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\""

    local count
    count=$(find "$XDG_DATA_HOME/dodot/probes/shell-init" -name 'profile-*.tsv' -type f | wc -l | tr -d ' ')
    [ "$count" = "3" ]
}

# ── `dodot probe shell-init` rendering ────────────────────────────

@test "probe shell-init renders the latest profile" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up
    bash -c ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\""

    run dodot probe shell-init
    [ "$status" -eq 0 ]
    assert_output_contains "Shell-init profile"
    assert_output_contains "vim"
    assert_output_contains "shell"
    assert_output_contains "aliases.sh"
    # Some duration label (µs / ms / s) should appear next to the row.
    [[ "$output" == *"µs"* || "$output" == *"ms"* ]]
}

@test "probe shell-init shows hint when no profile has been written" {
    # Profiling is on by default, but no shell has sourced the init
    # script yet — so the dir is empty / missing.
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    run dodot probe shell-init
    [ "$status" -eq 0 ]
    assert_output_contains "no profile yet"
}

@test "probe summary lists shell-init alongside the others" {
    run dodot probe
    [ "$status" -eq 0 ]
    assert_output_contains "shell-init"
    assert_output_contains "deployment-map"
    assert_output_contains "show-data-dir"
}

@test "probe shell-init --output json has the right shape" {
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up
    bash -c ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\""

    run dodot --output json probe shell-init
    [ "$status" -eq 0 ]
    assert_output_contains '"kind"'
    assert_output_contains '"shell-init"'
    assert_output_contains '"has_profile"'
    assert_output_contains '"groups"'
    assert_output_contains '"total_us"'
}

# ── Disabling profiling via config ────────────────────────────────

@test "init script omits profiling wrapper when disabled in config" {
    create_root_config '[profiling]\nenabled = false'
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    local script="$XDG_DATA_HOME/dodot/shell/dodot-init.sh"
    assert_exists "$script"
    # No profiling boilerplate.
    run grep -F "_dodot_prof" "$script"
    [ "$status" -ne 0 ]
    run grep -F "EPOCHREALTIME" "$script"
    [ "$status" -ne 0 ]

    # Sourcing it must not write a profile (no instrumentation present).
    bash -c ". $script"
    if [ -d "$XDG_DATA_HOME/dodot/probes/shell-init" ]; then
        local count
        count=$(find "$XDG_DATA_HOME/dodot/probes/shell-init" -name 'profile-*.tsv' -type f | wc -l | tr -d ' ')
        [ "$count" = "0" ]
    fi
}

@test "probe shell-init explains when profiling is disabled" {
    create_root_config '[profiling]\nenabled = false'
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    run dodot probe shell-init
    [ "$status" -eq 0 ]
    assert_output_contains "profiling is disabled"
}

# ── Rotation ──────────────────────────────────────────────────────

@test "dodot up rotates old profiles to keep_last_runs" {
    create_root_config '[profiling]\nkeep_last_runs = 2'
    create_pack_file "vim" "aliases.sh" "alias vi=vim"
    dodot up

    # Hand-create five fake profile files older than today's
    # rotation moment, then run `up` again to trigger pruning.
    local d="$XDG_DATA_HOME/dodot/probes/shell-init"
    mkdir -p "$d"
    for i in 1 2 3 4 5; do
        printf '# dodot shell-init profile v1\n' > "$d/profile-${i}-1-1.tsv"
    done

    dodot up

    local count
    count=$(find "$d" -name 'profile-*.tsv' -type f | wc -l | tr -d ' ')
    [ "$count" = "2" ]

    # The newest two (highest names) survive.
    assert_exists "$d/profile-4-1-1.tsv"
    assert_exists "$d/profile-5-1-1.tsv"
    assert_not_exists "$d/profile-1-1-1.tsv"
}
