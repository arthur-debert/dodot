#!/usr/bin/env bats
# E2E tests for the template clean filter (R6 of the template-magic
# track). Exercises the full git-side integration: install the
# filter, bind it via .gitattributes, edit a deployed template,
# verify `git diff` shows the template-space change.

setup() {
    load helpers/setup
    sandbox_setup

    # Initialise the dotfiles root as a git repo so install-filter
    # accepts it.
    git -C "$DOTFILES_ROOT" init -q
    git -C "$DOTFILES_ROOT" config user.email "test@example.com"
    git -C "$DOTFILES_ROOT" config user.name "Test"
    git -C "$DOTFILES_ROOT" config commit.gpgsign false

    # Filter subprocess (`git status`, `git diff`) runs in a fresh
    # shell that doesn't inherit the bats `dodot` function — the
    # registered clean command is bare `dodot template clean ...`,
    # so the binary must be on PATH.
    export PATH="$(dirname "$DODOT_BIN"):$PATH"
}

teardown() {
    sandbox_teardown
}

@test "install-filter writes the dodot-template filter to .git/config" {
    run dodot template install-filter
    [ "$status" -eq 0 ]
    assert_output_contains "Installed template clean filter"

    # Three keys present.
    [ "$(git -C "$DOTFILES_ROOT" config --get filter.dodot-template.clean)" = "dodot template clean --path %f" ]
    [ "$(git -C "$DOTFILES_ROOT" config --get filter.dodot-template.smudge)" = "cat" ]
    [ "$(git -C "$DOTFILES_ROOT" config --get filter.dodot-template.required)" = "true" ]
}

@test "install-filter is idempotent on re-run" {
    run dodot template install-filter
    [ "$status" -eq 0 ]
    run dodot template install-filter
    [ "$status" -eq 0 ]
    assert_output_contains "already installed"
}

@test "clean filter on clean state echoes stdin verbatim" {
    # No edit on either side. The filter's fast path hits and the
    # template content passes through unchanged.
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/cfg.toml.tmpl"
    expected="$(cat "$src")"

    out=$(dodot template clean --path "$src" < "$src")
    [ "$out" = "$expected" ]
}

@test "clean filter patches template when deployed file diverges" {
    # The core scenario: edit the rendered file, run the clean
    # filter on the source, observe the patched output (var
    # preserved, static line propagated).
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/cfg.toml"
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    src="$DOTFILES_ROOT/app/cfg.toml.tmpl"
    out=$(dodot template clean --path "$src" < "$src")
    [[ "$out" == *"port = 9999"* ]]
    [[ "$out" == *"name = {{ name }}"* ]]
}

@test "git status surfaces template-space diff after refresh + filter" {
    # End-to-end through git: install filter, bind in .gitattributes,
    # commit the clean state, edit deployed file, refresh source
    # mtime, run `git status` → source shows as modified, `git diff`
    # shows the template-space change.
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # Install + bind.
    run dodot template install-filter
    [ "$status" -eq 0 ]
    echo '*.tmpl filter=dodot-template' > "$DOTFILES_ROOT/.gitattributes"

    # Initial commit so we have a baseline tree to diff against.
    git -C "$DOTFILES_ROOT" add -A
    git -C "$DOTFILES_ROOT" commit -q -m "initial"

    # Edit the deployed file.
    sleep 1
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/cfg.toml"
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    # Without refresh, git's stat-cache won't even invoke the
    # filter — refresh is the bump.
    run dodot refresh --quiet
    [ "$status" -eq 0 ]

    # Now `git status` should see the source as modified.
    run git -C "$DOTFILES_ROOT" status --porcelain
    [[ "$output" == *"app/cfg.toml.tmpl"* ]]

    # And `git diff` shows the template-space change.
    run git -C "$DOTFILES_ROOT" diff app/cfg.toml.tmpl
    [[ "$output" == *"port = 9999"* ]]
}

@test "reinstalling the hook upgrades a stale R4-shape block" {
    # An older R4 hook (no `dodot refresh` line) should be detected
    # and upgraded to the new two-line form when the user re-runs
    # `dodot transform install-hook`.
    create_pack "app"
    create_pack_file "app" "cfg.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # Stage an old-style hook by hand: same guards, only the check
    # command (no refresh).
    cat > "$DOTFILES_ROOT/.git/hooks/pre-commit" <<'EOF'
#!/bin/sh
# >>> dodot transform check --strict (managed by `dodot transform install-hook`) >>>
# Old R4-style block.
dodot transform check --strict || exit 1
# <<< dodot transform check --strict <<<
EOF
    chmod +x "$DOTFILES_ROOT/.git/hooks/pre-commit"

    run dodot transform install-hook
    [ "$status" -eq 0 ]
    # New block is in place: refresh + check both run.
    assert_file_contains "$DOTFILES_ROOT/.git/hooks/pre-commit" "dodot refresh --quiet"
    assert_file_contains "$DOTFILES_ROOT/.git/hooks/pre-commit" "dodot transform check --strict"
}
