#!/usr/bin/env bats
# E2E tests for the MacOs paths feature: _app/, _lib/, force_app, app_aliases.
#
# These run on every host. Where macOS and Linux behave differently,
# tests assert the platform-appropriate outcome:
#
# - `_app/` and `app_aliases` route through `~/Library/Application Support`
#   on macOS; on Linux they collapse onto `$XDG_CONFIG_HOME` (the same
#   place `_xdg/` lands).
# - `_lib/` deploys to `$HOME/Library/<rest>` on macOS; on Linux it
#   produces a soft warning and skips with no symlink.
# - Adopting from `~/Library/Application Support/<X>/` is macOS-only —
#   on Linux that path doesn't match a recognized adopt source.

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
# Mirrors what XdgPather::app_support_dir produces.
expected_app_root() {
    if [[ "$(uname)" == "Darwin" ]]; then
        echo "$HOME/Library/Application Support"
    else
        echo "$XDG_CONFIG_HOME"
    fi
}

# ── _app/ prefix ────────────────────────────────────────────────

@test "_app/ deploys under app_support_dir without pack namespacing" {
    create_pack_file "vscode" "_app/Code/User/settings.json" '{"editor.fontSize": 14}'

    dodot up

    local root
    root="$(expected_app_root)"
    assert_exists "$root/Code/User/settings.json"
    assert_file_contains "$root/Code/User/settings.json" "fontSize"
}

@test "_app/ rule outranks the pack-name default" {
    # Pack literally named `Code` with `_app/Code/` subtree: priority 2c
    # routes through app_support, not the pack-name default at $XDG/Code.
    create_pack_file "Code" "_app/Code/marker.json" '{"x": 1}'

    dodot up

    local root
    root="$(expected_app_root)"
    assert_exists "$root/Code/marker.json"
    # On Linux the app_support root *is* $XDG, so the file at
    # $XDG_CONFIG_HOME/Code/marker.json is shared by the priority-2c
    # routing and the priority-6 default. There's nothing extra to
    # assert beyond "the file landed there"; the unit tests pin the
    # priority order itself.
}

# ── force_app ───────────────────────────────────────────────────

@test "default force_app routes Code/ first segment to app_support" {
    # `Code` ships in the default force_app list, so a top-level
    # `Code/` entry routes to <app_support_dir>/Code/... without any
    # `_app/` prefix in the pack tree.
    create_pack_file "macapps" "Code/User/settings.json" '{"theme": "Default"}'

    dodot up

    local root
    root="$(expected_app_root)"
    assert_exists "$root/Code/User/settings.json"
    assert_file_contains "$root/Code/User/settings.json" "theme"
}

@test "force_app is case sensitive" {
    # `code` (lowercase) is not in force_app, so a top-level `code/`
    # entry falls through to the default rule under the pack name.
    create_pack_file "macapps" "code/foo" "bar"

    dodot up

    # Default rule: $XDG_CONFIG_HOME/macapps/code/foo
    assert_exists "$XDG_CONFIG_HOME/macapps/code/foo"
    # And NOT under the app-support root with a lowercase folder.
    assert_not_exists "$(expected_app_root)/code/foo"
}

# ── app_aliases ─────────────────────────────────────────────────

@test "app_aliases reroutes the pack default to app_support_dir" {
    # Pack `vscode` aliased to `Code`: top-level entries deploy under
    # <app_support_dir>/Code/... without a `_app/` prefix.
    create_pack_file "vscode" "User/settings.json" '{"x": 2}'
    create_pack_config "vscode" '[symlink.app_aliases]\nvscode = "Code"\n'

    dodot up

    local root
    root="$(expected_app_root)"
    assert_exists "$root/Code/User/settings.json"
    assert_file_contains "$root/Code/User/settings.json" '"x": 2'
}

@test "app_aliases lose to explicit _xdg/ prefix" {
    # Aliases only modify the default rule. A `_xdg/` entry in an
    # aliased pack still routes raw under XDG_CONFIG_HOME.
    create_pack_file "vscode" "_xdg/Code/User/foo" "explicit"
    create_pack_config "vscode" '[symlink.app_aliases]\nvscode = "Code"\n'

    dodot up

    assert_exists "$XDG_CONFIG_HOME/Code/User/foo"
    assert_file_contains "$XDG_CONFIG_HOME/Code/User/foo" "explicit"
}

# ── _lib/ prefix ────────────────────────────────────────────────

@test "_lib/ deploys to ~/Library on macOS, warns and skips on Linux" {
    create_pack_file "macapps" "_lib/LaunchAgents/com.example.foo.plist" \
        "<?xml version=\"1.0\"?><plist></plist>"

    run dodot up
    [ "$status" -eq 0 ]

    if [[ "$(uname)" == "Darwin" ]]; then
        assert_exists "$HOME/Library/LaunchAgents/com.example.foo.plist"
    else
        # Soft warning on stderr (or stdout, BATS captures both via run);
        # the file is *not* deployed. _lib/ is macOS-only.
        assert_output_contains "macOS-only path"
        assert_not_exists "$HOME/Library/LaunchAgents/com.example.foo.plist"
    fi
}

# ── Adopt from AppSupport (macOS only) ──────────────────────────

@test "adopt from ~/Library/Application Support/<X>/ round-trips via _app/" {
    if [[ "$(uname)" != "Darwin" ]]; then
        skip "AppSupport adopt is macOS-only (Linux app_support_dir collapses to XDG)"
    fi

    mkdir -p "$APP_SUPPORT/Code/User"
    echo '{"editor.fontSize": 14}' > "$APP_SUPPORT/Code/User/settings.json"

    run dodot adopt "$APP_SUPPORT/Code/User/settings.json"
    [ "$status" -eq 0 ]

    # File copied into pack at `Code/_app/Code/User/settings.json`
    # (pack name `Code`, in-pack prefix `_app/Code/...`).
    assert_exists "$DOTFILES_ROOT/Code/_app/Code/User/settings.json"
    assert_file_contains "$DOTFILES_ROOT/Code/_app/Code/User/settings.json" "fontSize"

    # Original is now a symlink — the round-trip property: `dodot up`
    # would land it back at the same place.
    [ -L "$APP_SUPPORT/Code/User/settings.json" ] || \
        { echo "expected symlink at $APP_SUPPORT/Code/User/settings.json" >&2; return 1; }
}

@test "adopt of AppSupport source emits app_aliases tip (macOS only)" {
    if [[ "$(uname)" != "Darwin" ]]; then
        skip "AppSupport adopt is macOS-only"
    fi

    mkdir -p "$APP_SUPPORT/Cursor/User"
    echo '{}' > "$APP_SUPPORT/Cursor/User/settings.json"

    run dodot adopt "$APP_SUPPORT/Cursor/User/settings.json"
    [ "$status" -eq 0 ]

    # The capitalization heuristic identifies `Cursor` as a GUI-app
    # folder name and suggests the `app_aliases` ergonomic.
    assert_output_contains "app_aliases"
    assert_output_contains "Cursor"
}

# ── Sandboxed Containers refusal (cross-platform) ──────────────

@test "adopt refuses ~/Library/Containers/ paths" {
    # The refusal is platform-agnostic so no one accidentally adopts a
    # container's plist on a CI Linux box. We synthesize the path
    # under the sandbox HOME rather than touching real ~/Library.
    mkdir -p "$HOME/Library/Containers/com.example.app/Data/Library/Preferences"
    echo "x" > "$HOME/Library/Containers/com.example.app/Data/Library/Preferences/foo.plist"

    run dodot adopt "$HOME/Library/Containers/com.example.app/Data/Library/Preferences/foo.plist"
    [ "$status" -ne 0 ]
    assert_output_contains "sandboxed"
}
