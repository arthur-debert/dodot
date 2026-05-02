#!/usr/bin/env bats
# E2E tests for the per-file routing prefixes parallel to `home.X`:
# `app.X`, `xdg.X`, `lib.X`. Each is the single-file counterpart to
# its `_app/`, `_xdg/`, `_lib/` directory cousin — same skip-pack-
# namespace semantics, top-level files only.
#
# Also covers the routing-override conflict: a file with both a
# filesystem-naming routing prefix and a `[symlink.targets]` entry
# refuses to deploy.

setup() {
    load helpers/setup
    sandbox_setup
    APP_SUPPORT="$HOME/Library/Application Support"
    mkdir -p "$APP_SUPPORT"
}

teardown() {
    sandbox_teardown
}

# Resolve the platform-appropriate app-support root for assertions.
expected_app_root() {
    if [[ "$(uname)" == "Darwin" ]]; then
        echo "$HOME/Library/Application Support"
    else
        echo "$XDG_CONFIG_HOME"
    fi
}

# ── app. file prefix ────────────────────────────────────────────

@test "app.X deploys raw under app_support_dir, no pack namespace" {
    create_pack_file "vscode" "app.global-settings.json" '{"x": 1}'

    dodot up

    local root
    root="$(expected_app_root)"
    assert_exists "$root/global-settings.json"
}

@test "status shows app. file deploy path with prefix stripped" {
    create_pack_file "vscode" "app.global-settings.json" '{"x": 1}'

    run dodot status
    [ "$status" -eq 0 ]
    # The mapping line shows source `app.global-settings.json` ➞ deploy
    # path under app_support; the deploy half drops the `app.` prefix.
    if [[ "$(uname)" == "Darwin" ]]; then
        assert_output_contains "Application Support/global-settings.json"
    else
        assert_output_contains ".config/global-settings.json"
    fi
}

# ── xdg. file prefix ────────────────────────────────────────────

@test "xdg.X deploys raw under XDG_CONFIG_HOME, no pack namespace" {
    create_pack_file "desktop" "xdg.mimeapps.list" "[Default Applications]"

    dodot up

    assert_exists "$XDG_CONFIG_HOME/mimeapps.list"
    assert_file_contains "$XDG_CONFIG_HOME/mimeapps.list" "Default Applications"
}

# ── lib. file prefix (macOS only) ───────────────────────────────

@test "lib.X deploys to ~/Library on macOS, warns and skips elsewhere" {
    create_pack_file "macapps" "lib.com.example.foo.plist" "<plist/>"

    if [[ "$(uname)" == "Darwin" ]]; then
        dodot up
        assert_exists "$HOME/Library/com.example.foo.plist"
    else
        run dodot up
        assert_output_contains "macOS-only path"
        assert_not_exists "$HOME/Library/com.example.foo.plist"
    fi
}

# ── Subdirectory files keep the prefix literal ──────────────────

@test "app./xdg./lib. prefixes only apply at top level" {
    # Nested files inside a top-level dir keep the prefix as part of
    # the literal name; the wholesale dir link routes them under XDG.
    create_pack_file "app" "subdir/app.config.json" '{"y": 2}'

    dodot up

    # The subdir is wholesale-linked under the pack's XDG dir; the
    # nested file's name retains the literal `app.` prefix.
    assert_exists "$HOME/.config/app/subdir/app.config.json"
}

# ── Routing override conflict ───────────────────────────────────

@test "filename with home. + [symlink.targets] entry errors out" {
    # Use a sandbox-rooted target so the assertion can verify the
    # conflicted file did not get deployed without depending on the
    # host filesystem (e.g. /etc/bashrc on macOS exists by default).
    local conflict_target="$SANDBOX/conflict-target-bashrc"
    create_pack_file "shell" "home.bashrc" "# bashrc"
    create_pack_config "shell" "$(printf '[symlink]\ntargets = { "home.bashrc" = "%s" }\n' "$conflict_target")"

    run dodot up
    # `up` collects per-pack errors and reports them in the output.
    # The symlink for the conflicted file must NOT have been deployed.
    assert_output_contains "routing override conflict"
    assert_output_contains "home.bashrc"
    assert_output_contains "$conflict_target"
    assert_not_exists "$HOME/.bashrc"
    assert_not_exists "$conflict_target"
}

@test "filename inside _xdg/ + [symlink.targets] entry errors out" {
    local conflict_target="$SANDBOX/conflict-target-ghostty.conf"
    create_pack_file "term" "_xdg/ghostty/config" "# ghostty"
    create_pack_config "term" "$(printf '[symlink]\ntargets = { "_xdg/ghostty/config" = "%s" }\n' "$conflict_target")"

    run dodot up
    assert_output_contains "routing override conflict"
    assert_output_contains "_xdg/ghostty/config"
    assert_not_exists "$XDG_CONFIG_HOME/ghostty/config"
    assert_not_exists "$conflict_target"
}

@test "[symlink.targets] for a plain (non-prefixed) file still works" {
    # The conflict only fires when both the filename AND the config
    # express a routing intent. A plain filename routed via
    # [symlink.targets] is the canonical use case for the override —
    # it must keep working.
    create_pack_file "etc" "mysterious.conf" "# m"
    create_pack_config "etc" '[symlink]\ntargets = { "mysterious.conf" = "/tmp/dodot-mysterious.conf" }'

    dodot up

    assert_exists "/tmp/dodot-mysterious.conf"
    assert_file_contains "/tmp/dodot-mysterious.conf" "# m"
    rm -f "/tmp/dodot-mysterious.conf"
}
