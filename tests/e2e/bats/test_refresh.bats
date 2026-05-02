#!/usr/bin/env bats
# E2E tests for `dodot refresh` (R5 of the template-magic track).
#
# `dodot refresh` copies deployed-side mtimes onto template sources
# whenever the deployed bytes have diverged from the cached baseline.
# Why: git's stat-cache skips re-reading working-tree files when their
# mtime is unchanged, so a deployed-side edit to a template won't
# surface in `git status` until the source mtime is bumped. Refresh
# is the bump.

setup() {
    load helpers/setup
    sandbox_setup
    git -C "$DOTFILES_ROOT" init -q
    git -C "$DOTFILES_ROOT" config user.email "test@example.com"
    git -C "$DOTFILES_ROOT" config user.name "Test"
}

# Portable mtime reader. Different stat invocations on macOS (BSD)
# vs Linux (GNU coreutils): `-c %Y` is GNU-only, `-f %m` is BSD-only,
# and `stat -f` on GNU coreutils silently switches to filesystem-info
# mode (returning the mount point) rather than failing — so a naive
# `stat -f %m || stat -c %Y` chain works on macOS but produces a
# non-numeric result on Linux that breaks `[ -gt ]` later. Detect
# the platform once and pick the right format.
mtime() {
    if [[ "$(uname)" == "Darwin" ]]; then
        stat -f %m "$1"
    else
        stat -c %Y "$1"
    fi
}

teardown() {
    sandbox_teardown
}

@test "refresh on clean state reports in-sync and exits 0" {
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    run dodot refresh
    [ "$status" -eq 0 ]
    assert_output_contains "in sync"
}

@test "refresh after editing deployed file touches the source mtime" {
    # The core scenario: edit the rendered file in the datastore,
    # then `dodot refresh` should bring the source mtime up to date.
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/cfg.toml.tmpl"
    src_mtime_before=$(mtime "$src")

    # Make sure the deployed mtime will be strictly later than the
    # current source mtime, even on fast filesystems.
    sleep 1
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/cfg.toml"
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    run dodot refresh
    [ "$status" -eq 0 ]
    assert_output_contains "Touched"

    src_mtime_after=$(mtime "$src")
    [ "$src_mtime_after" -gt "$src_mtime_before" ]
}

@test "refresh --list-paths prints divergent sources without writing mtimes" {
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/cfg.toml.tmpl"
    src_mtime_before=$(mtime "$src")

    sleep 1
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/cfg.toml"
    echo "name = Edited" > "$rendered"

    run dodot refresh --list-paths
    [ "$status" -eq 0 ]
    # Output is the absolute source path, suitable for `xargs touch`
    # or piping into a watcher integration.
    assert_output_contains "$src"

    # CRITICAL: --list-paths must NOT have written the mtime.
    src_mtime_after=$(mtime "$src")
    [ "$src_mtime_after" = "$src_mtime_before" ]
}

@test "refresh --quiet emits no output but still updates mtime" {
    # The Tier 2 shell alias depends on this: a no-op refresh on every
    # git invocation should be silent, but a real refresh must still
    # do its work.
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/cfg.toml.tmpl"
    sleep 1
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/cfg.toml"
    echo "name = Edited" > "$rendered"
    src_mtime_before=$(mtime "$src")

    run dodot refresh --quiet
    [ "$status" -eq 0 ]
    [ -z "$output" ]

    src_mtime_after=$(mtime "$src")
    [ "$src_mtime_after" -gt "$src_mtime_before" ]
}

@test "refresh on empty cache reports nothing-to-do" {
    # No `dodot up` ever ran in this sandbox, so the baseline cache is
    # empty. Refresh must not error out — just say there's nothing.
    run dodot refresh
    [ "$status" -eq 0 ]
    assert_output_contains "No template baselines"
}
