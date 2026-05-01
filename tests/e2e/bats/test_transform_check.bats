#!/usr/bin/env bats
# E2E tests for `dodot transform check` (R3 of the template-magic track).
#
# Verifies the user-visible CLI behaviour of reverse-merge: a deployed-
# side edit propagates back to the template source in the static-line
# case, surfaces a conflict block in the ambiguous case, and `--strict`
# blocks unresolved markers (the contract the pre-commit hook in R4
# will rely on). Mirrors the pattern in test_preprocessing.bats.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "transform check on clean state reports synced and exits 0" {
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    run dodot transform check
    [ "$status" -eq 0 ]
    assert_output_contains "clean"
}

@test "transform check propagates static-line edit back to template source" {
    # The auto-merge happy path: edit the rendered file, run check,
    # the static line lands in the .tmpl source while `{{ var }}`
    # is preserved verbatim.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # Modify the rendered file in the datastore (the symlink target).
    # Editing the user-side symlink lands here transparently — same
    # operation either way.
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    [ -f "$rendered" ]
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    run dodot transform check
    [ "$status" -eq 1 ]
    assert_output_contains "patched"
    assert_output_contains "config.toml.tmpl"

    # Source was rewritten: static line propagated, variable preserved.
    src="$DOTFILES_ROOT/app/config.toml.tmpl"
    assert_file_contains "$src" "port = 9999"
    assert_file_contains "$src" "name = {{ name }}"
}

@test "transform check leaves source alone for pure-data edit" {
    # Only the variable's value changed in the deployed file — the
    # template is correct as-is. Action: synced, no source mutation.
    create_pack "app"
    create_pack_file "app" "greeting.tmpl" 'hello {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/greeting.tmpl"
    original_src=$(cat "$src")

    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/greeting"
    echo "hello Bob" > "$rendered"

    run dodot transform check
    [ "$status" -eq 0 ]
    assert_output_contains "synced"

    # Source must be byte-identical.
    [ "$(cat "$src")" = "$original_src" ]
}

@test "transform check --dry-run reports patched but does not write" {
    # The action label still surfaces the would-be patch so the user
    # can preview, but the source file is untouched on disk.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/config.toml.tmpl"
    original_src=$(cat "$src")

    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    run dodot transform check --dry-run
    [ "$status" -eq 1 ]
    assert_output_contains "patched"

    # Source unchanged on disk.
    [ "$(cat "$src")" = "$original_src" ]
}

@test "transform check --strict reports unresolved dodot-conflict markers" {
    # Deploy normally, then write conflict markers into the source —
    # simulating a previous transform-check run that emitted a block.
    # --strict catches it; the pre-commit hook in R4 will use this.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/config.toml.tmpl"
    cat > "$src" <<'EOF'
name = {{ name }}
>>>>>> dodot-conflict (template)
host = "localhost"
====== dodot-conflict (deployed)
host = "production"
<<<<<< dodot-conflict
EOF

    # Lax mode: source change is just InputChanged; no marker scan.
    run dodot transform check
    # InputChanged is not a finding, so exit 0; but the source did
    # change so the next `up` will re-render. The strict-mode test
    # below is the one that should fail.
    [ "$status" -eq 0 ]
    assert_output_contains "input changed"

    # Strict mode: marker scan trips, exit 1.
    run dodot transform check --strict
    [ "$status" -eq 1 ]
    assert_output_contains "Unresolved dodot-conflict markers"
    assert_output_contains "config.toml.tmpl"
}

@test "transform check --strict on clean repo exits 0" {
    create_pack "app"
    create_pack_file "app" "greeting.tmpl" 'hello {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    run dodot transform check --strict
    [ "$status" -eq 0 ]
    assert_output_contains "clean"
}
