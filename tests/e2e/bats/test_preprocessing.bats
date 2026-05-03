#!/usr/bin/env bats
# E2E tests for the preprocessing pipeline (template + unarchive).
#
# Intentionally narrow: the Rust suite already covers the matrix of
# preprocessor behaviours. These tests verify the *integration* — that
# `dodot up` wires preprocessing into the real binary, renders
# `.tmpl` files end-to-end, and exposes clean errors to the CLI.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "template file is rendered and deployed via symlink" {
    create_pack "app"
    create_pack_file "app" "vimrc.tmpl" 'set user={{ name }}
set os={{ dodot.os }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # The user-visible file has the `.tmpl` extension stripped; under
    # the post-#48 default it deploys to ~/.config/<pack>/<name>:
    # vimrc → ~/.config/app/vimrc.
    [ -L "$HOME/.config/app/vimrc" ]

    # Rendered content: the variable is substituted and dodot.os is a
    # non-empty string (whatever the host OS is).
    assert_file_contains "$HOME/.config/app/vimrc" "set user=Alice"
    assert_file_contains "$HOME/.config/app/vimrc" "set os="

    # The rendered file lives in the datastore under packs/<pack>/preprocessed/.
    assert_exists "$XDG_DATA_HOME/dodot/packs/app/preprocessed/vimrc"
}

@test "template can be disabled globally via root config" {
    create_root_config '[preprocessor]
enabled = false'
    create_pack "app"
    create_pack_file "app" "vimrc.tmpl" 'set user={{ name }}'

    run dodot up
    [ "$status" -eq 0 ]

    # With preprocessing disabled the `.tmpl` file is deployed verbatim,
    # extension preserved, no rendering.
    [ -L "$HOME/.config/app/vimrc.tmpl" ]
    assert_file_contains "$HOME/.config/app/vimrc.tmpl" '{{ name }}'
}

@test "undefined template variable surfaces with source path" {
    create_pack "app"
    create_pack_file "app" "bad.tmpl" 'value = "{{ nope }}"'

    # `dodot up` reports per-pack errors in its output but still exits 0
    # when at least the run itself completed. The important thing is the
    # error naming the template so the user can find it, plus no stray
    # file getting deployed.
    run dodot up
    assert_output_contains "bad.tmpl"
    assert_output_contains "template render failed"
    [ ! -e "$HOME/.config/app/bad" ]
}

@test "pack config overrides root config for template vars" {
    create_root_config '[preprocessor.template.vars]
name = "Root"'
    create_pack "app"
    create_pack_file "app" "greeting.tmpl" 'hello {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Pack"'

    run dodot up
    [ "$status" -eq 0 ]
    # Top-level file deploys to ~/.config/<pack>/<name> (post-#48 default).
    assert_file_contains "$HOME/.config/app/greeting" "hello Pack"
}

@test "template collision with regular file is rejected" {
    create_pack "app"
    create_pack_file "app" "config.toml" "regular"
    create_pack_file "app" "config.toml.tmpl" 'templated {{ 1 + 1 }}'

    run dodot up
    assert_output_contains "preprocessing collision"
    assert_output_contains "config.toml"
    # Neither file should have been deployed — the pack short-circuits
    # on the collision.
    [ ! -e "$HOME/.config/app/config.toml" ]
}

@test "template with env var and default filter renders fallback" {
    create_pack "app"
    create_pack_file "app" "settings.tmpl" 'editor={{ env.DODOT_MISSING_VAR | default("nano") }}'

    # Make sure the probe variable is truly unset for this process.
    unset DODOT_MISSING_VAR

    run dodot up
    [ "$status" -eq 0 ]
    assert_file_contains "$HOME/.config/app/settings" "editor=nano"
}

@test "dodot up preserves user edits to deployed template output (issue #110)" {
    # The §6.4 row-3 case: source unchanged, deployed file edited by
    # the user. `dodot up` must NOT silently overwrite the edit. The
    # render is skipped, the user's bytes stay on disk, and a warning
    # surfaces telling them how to reconcile.
    create_pack "app"
    create_pack_file "app" "cfg.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/cfg"
    assert_file_contains "$rendered" "name = Alice"

    # User edits the deployed file directly.
    echo "name = USER EDITED" > "$rendered"

    # Re-running up must not clobber the edit. Exit 0 (the run
    # succeeded; some files were just preserved).
    run dodot up
    [ "$status" -eq 0 ]
    assert_file_contains "$rendered" "name = USER EDITED"
    # And the warning should mention the resolution paths.
    assert_output_contains "preserved"
    assert_output_contains "transform check"
    assert_output_contains "force"
}

@test "dodot up --force overwrites divergent deployed file (issue #110 escape hatch)" {
    # The documented escape hatch: --force bypasses the divergence
    # guard so users who know they want the freshly-rendered output
    # (e.g. after rotating an env var the template references) can
    # still pick it up.
    create_pack "app"
    create_pack_file "app" "cfg.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/cfg"

    # User edit, then --force.
    echo "name = USER EDITED" > "$rendered"
    run dodot up --force
    [ "$status" -eq 0 ]
    # The render landed; the user's edit is gone.
    assert_file_contains "$rendered" "name = Alice"
}

@test "template with unresolved dodot-conflict markers is refused at deploy time" {
    # Once `dodot transform check` has rewritten a template to flag
    # ambiguous edits, the source carries dodot-conflict marker lines.
    # `dodot up` must refuse to render that template — the markers
    # would otherwise pass through MiniJinja verbatim and deploy as
    # garbage. The error names the source path so the user knows
    # exactly what to fix.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'first = "ok"
>>>>>> dodot-conflict (template)
host = "{{ env.DB_HOST }}"
====== dodot-conflict (deployed)
host = "production.db"
<<<<<< dodot-conflict
last = "ok"'

    run dodot up
    assert_output_contains "config.toml.tmpl"
    assert_output_contains "dodot-conflict"
    # No partial deployment: the rendered file must not exist.
    [ ! -e "$HOME/.config/app/config.toml" ]
}
