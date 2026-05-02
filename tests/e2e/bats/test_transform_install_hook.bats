#!/usr/bin/env bats
# E2E tests for `dodot transform install-hook` (R4 of the template-magic track).
#
# Verifies the user-visible CLI behaviour of the pre-commit hook
# installer: idempotent + additive, errors out without `.git`, and
# the installed hook actually fires on `git commit` (refusing the
# commit when the template-source has unresolved markers).

setup() {
    load helpers/setup
    sandbox_setup
    # Initialise the dotfiles root as a git repo so the installer
    # accepts it. `dodot up` runs unaffected; the hook only matters
    # for `git commit`.
    git -C "$DOTFILES_ROOT" init -q
    git -C "$DOTFILES_ROOT" config user.email "test@example.com"
    git -C "$DOTFILES_ROOT" config user.name "Test"
    git -C "$DOTFILES_ROOT" config commit.gpgsign false
    # The hook runs in a subshell from `git commit` and doesn't
    # inherit the bats `dodot` shell function — it needs the binary
    # on PATH so the literal `dodot transform check --strict` line
    # in the hook resolves.
    export PATH="$(dirname "$DODOT_BIN"):$PATH"
}

teardown() {
    sandbox_teardown
}

@test "install-hook creates a new pre-commit hook" {
    [ ! -e "$DOTFILES_ROOT/.git/hooks/pre-commit" ]

    run dodot transform install-hook
    [ "$status" -eq 0 ]
    assert_output_contains "Installed pre-commit hook"

    [ -f "$DOTFILES_ROOT/.git/hooks/pre-commit" ]
    [ -x "$DOTFILES_ROOT/.git/hooks/pre-commit" ]
    assert_file_contains "$DOTFILES_ROOT/.git/hooks/pre-commit" \
        "dodot transform check --strict"
}

@test "install-hook is idempotent on re-run" {
    run dodot transform install-hook
    [ "$status" -eq 0 ]

    # Second run: same hook content, "already installed" message.
    body_before="$(cat "$DOTFILES_ROOT/.git/hooks/pre-commit")"
    run dodot transform install-hook
    [ "$status" -eq 0 ]
    assert_output_contains "already installed"
    [ "$(cat "$DOTFILES_ROOT/.git/hooks/pre-commit")" = "$body_before" ]
}

@test "install-hook appends to an existing pre-commit hook" {
    # User has their own hook (e.g. from another tool). install-hook
    # must preserve the existing content.
    cat > "$DOTFILES_ROOT/.git/hooks/pre-commit" <<'EOF'
#!/bin/sh
echo 'my custom hook'
exit 0
EOF
    chmod +x "$DOTFILES_ROOT/.git/hooks/pre-commit"

    run dodot transform install-hook
    [ "$status" -eq 0 ]
    assert_output_contains "Appended"

    # Both the user's content and our block are present.
    assert_file_contains "$DOTFILES_ROOT/.git/hooks/pre-commit" \
        "my custom hook"
    assert_file_contains "$DOTFILES_ROOT/.git/hooks/pre-commit" \
        "dodot transform check --strict"
}

@test "installed hook blocks commit when template source has unresolved markers" {
    # End-to-end: deploy a template, install the hook, write
    # dodot-conflict markers into the template source, attempt to
    # commit. The hook fires `dodot transform check --strict`, which
    # detects the markers and exits 1, so `git commit` aborts.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    run dodot transform install-hook
    [ "$status" -eq 0 ]

    # Stage and commit the clean state first so the next commit
    # has a parent (clarifies the failure mode below).
    git -C "$DOTFILES_ROOT" add -A
    run git -C "$DOTFILES_ROOT" commit -m "initial" --no-verify
    [ "$status" -eq 0 ]

    # Now write conflict markers into the template source.
    cat > "$DOTFILES_ROOT/app/config.toml.tmpl" <<'EOF'
name = {{ name }}
>>>>>> dodot-conflict (template)
host = "localhost"
====== dodot-conflict (deployed)
host = "production"
<<<<<< dodot-conflict
EOF

    # Attempt to commit — the hook should refuse.
    git -C "$DOTFILES_ROOT" add -A
    run git -C "$DOTFILES_ROOT" commit -m "dirty"
    [ "$status" -ne 0 ]
    # The hook's stdout/stderr should mention the marker scan or the
    # conflict line numbers.
    assert_output_contains "dodot-conflict"
}

@test "installed hook lets commit succeed when state is clean" {
    create_pack "app"
    create_pack_file "app" "greeting.tmpl" 'hello {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    run dodot transform install-hook
    [ "$status" -eq 0 ]

    git -C "$DOTFILES_ROOT" add -A
    run git -C "$DOTFILES_ROOT" commit -m "clean"
    [ "$status" -eq 0 ]
}
