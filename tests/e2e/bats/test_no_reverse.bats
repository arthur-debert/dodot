#!/usr/bin/env bats
# E2E tests for `[preprocessor.template] no_reverse` (R9 of the
# template-magic track).
#
# `no_reverse` is the per-file opt-out that lets users skip
# burgertocow's reverse-merge for templates whose content is mostly
# dynamic (the heuristic degrades there and produces more conflict
# markers than usable diffs). With the opt-out:
#
#   - `dodot up` still renders the template normally.
#   - The baseline cache still tracks divergence.
#   - `dodot transform status` still reports the underlying state.
#   - `dodot transform check` short-circuits to Synced (no source
#     mutation, no findings, exit 0).
#   - The template clean filter falls through to "echo stdin" on the
#     slow path (fast-path hash equality still wins on clean state).

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "no_reverse opts a file out of transform check's reverse-merge" {
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"
[preprocessor.template]
no_reverse = ["config.toml.tmpl"]'

    run dodot up
    [ "$status" -eq 0 ]

    # Edit the rendered file (deployed-side change).
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    [ -f "$rendered" ]
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    src="$DOTFILES_ROOT/app/config.toml.tmpl"
    original_src=$(cat "$src")

    # Without no_reverse, this would patch the source and exit 1.
    # With no_reverse, exit 0 + source byte-identical.
    run dodot transform check
    [ "$status" -eq 0 ]

    new_src=$(cat "$src")
    [ "$new_src" = "$original_src" ]
}

@test "no_reverse glob pattern matches multiple files" {
    create_pack "app"
    create_pack_file "app" "alpha.gen.tmpl" 'a = {{ name }}'
    create_pack_file "app" "beta.gen.tmpl" 'b = {{ name }}'
    create_pack_file "app" "regular.tmpl" 'r = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"
[preprocessor.template]
no_reverse = ["*.gen.tmpl"]'

    run dodot up
    [ "$status" -eq 0 ]

    # Edit all three rendered files.
    for f in alpha.gen beta.gen regular; do
        rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/$f"
        [ -f "$rendered" ]
        echo "edited content here" > "$rendered"
    done

    run dodot transform check
    # `regular.tmpl` falls into reverse-merge (one finding); the two
    # `*.gen.tmpl` files are skipped. So exit code is 1 (the regular
    # finding) but the gen sources must be byte-identical.
    [ "$status" -eq 1 ]

    alpha_src=$(cat "$DOTFILES_ROOT/app/alpha.gen.tmpl")
    beta_src=$(cat "$DOTFILES_ROOT/app/beta.gen.tmpl")
    [ "$alpha_src" = "a = {{ name }}" ]
    [ "$beta_src" = "b = {{ name }}" ]
}

@test "no_reverse keeps transform status surfacing the divergence" {
    # The opt-out specifically scopes to the auto-merge step. Passive
    # `transform status` must still report the underlying state so
    # the user knows the deployed file diverged.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"
[preprocessor.template]
no_reverse = ["config.toml.tmpl"]'

    run dodot up
    [ "$status" -eq 0 ]

    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    echo "name = Alice EDITED" > "$rendered"

    run dodot transform status
    [ "$status" -eq 0 ]
    assert_output_contains "diverged"
    assert_output_contains "output_changed"
}
