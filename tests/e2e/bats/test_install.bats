#!/usr/bin/env bats
# E2E tests for the install handler (real script execution).

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "install.sh runs and creates marker" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
echo "installed" > "$HOME/.tools-installed"'

    dodot up
    assert_exists "$HOME/.tools-installed"
    assert_file_contains "$HOME/.tools-installed" "installed"
}

@test "install.sh creates sentinel after run" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
echo done'

    dodot up
    assert_sentinel_exists "tools" "install" "install.sh-*"
}

@test "install.sh does not re-run on second up" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
echo "$(date +%s)" > "$HOME/.install-timestamp"'

    dodot up
    local first_ts
    first_ts="$(cat "$HOME/.install-timestamp")"

    # Small delay to ensure different timestamp
    sleep 1

    dodot up
    local second_ts
    second_ts="$(cat "$HOME/.install-timestamp")"

    # Should be the same — script didn't re-run
    [ "$first_ts" = "$second_ts" ]
}

@test "install.sh re-runs with --provision-rerun" {

    create_pack_script "tools" "install.sh" '#!/bin/sh
uuidgen > "$HOME/.install-timestamp"'

    dodot up
    local first_ts
    first_ts="$(cat "$HOME/.install-timestamp")"

    dodot up --provision-rerun
    local second_ts
    second_ts="$(cat "$HOME/.install-timestamp")"

    # Should be different — script re-ran
    [ "$first_ts" != "$second_ts" ]
}

@test "install handler shows installed/never run in status" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
echo done'

    run dodot status
    assert_output_contains "never run"

    dodot up
    run dodot status
    assert_output_contains "installed"
}

@test "up --no-provision skips install.sh" {
    create_pack_script "tools" "install.sh" '#!/bin/sh
touch "$HOME/.should-not-exist"'

    dodot up --no-provision

    assert_not_exists "$HOME/.should-not-exist"

    run dodot status
    assert_output_contains "never run"
}
