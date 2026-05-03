#!/usr/bin/env bats
# E2E showcase tests for the full template-magic stack (R8 of the
# template-magic track). Exercises the user's actual workflow from
# install through day-to-day editing through commit, end to end.
#
# Per-PR bats files (test_transform_check, test_transform_install_hook,
# test_template_clean_filter, test_refresh, test_transform_status_and_alias)
# already cover their respective phases in isolation. THIS file covers
# the integration: every piece installed in the canonical order, the
# user editing a deployed template, the various git operations seeing
# the right thing, the commit cycle blocking on conflicts and
# succeeding when clean.
#
# When this file passes, the spec from `docs/proposals/magic.lex`
# §"User Experience" works end-to-end.

setup() {
    load helpers/setup
    sandbox_setup

    git -C "$DOTFILES_ROOT" init -q
    git -C "$DOTFILES_ROOT" config user.email "test@example.com"
    git -C "$DOTFILES_ROOT" config user.name "Test"
    git -C "$DOTFILES_ROOT" config commit.gpgsign false

    # Hook + clean filter both invoke `dodot ...` in a fresh shell
    # subprocess that doesn't inherit the bats `dodot` function. Put
    # the binary on PATH so bare `dodot` resolves there.
    export PATH="$(dirname "$DODOT_BIN"):$PATH"
}

teardown() {
    sandbox_teardown
}

# ── The install ladder ──────────────────────────────────────────

@test "install ladder: hook + filter + alias all install cleanly in order" {
    # The canonical install sequence as a user would run it after
    # their first `dodot up`. Each step is idempotent on its own;
    # this test pins that they don't interfere with each other.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # Step 1: pre-commit hook
    run dodot transform install-hook
    [ "$status" -eq 0 ]
    [ -x "$DOTFILES_ROOT/.git/hooks/pre-commit" ]
    assert_file_contains "$DOTFILES_ROOT/.git/hooks/pre-commit" "dodot refresh --quiet"
    assert_file_contains "$DOTFILES_ROOT/.git/hooks/pre-commit" "dodot transform check --strict"

    # Step 2: template clean filter
    run dodot template install-filter
    [ "$status" -eq 0 ]
    [ "$(git -C "$DOTFILES_ROOT" config --get filter.dodot-template.clean)" = "dodot template clean --path %f" ]
    [ "$(git -C "$DOTFILES_ROOT" config --get filter.dodot-template.smudge)" = "cat" ]

    # Step 3: shell alias
    run dodot git-install-alias --shell zsh
    [ "$status" -eq 0 ]
    assert_file_contains "$HOME/.zshrc" "alias git='dodot refresh --quiet && command git'"

    # Re-run the entire ladder; every step should report
    # already-installed.
    run dodot transform install-hook
    [ "$status" -eq 0 ]
    assert_output_contains "already"
    run dodot template install-filter
    [ "$status" -eq 0 ]
    assert_output_contains "already"
    run dodot git-install-alias --shell zsh
    [ "$status" -eq 0 ]
    assert_output_contains "already"
}

# ── The day-to-day workflow ─────────────────────────────────────

@test "workflow: edit deployed → git status sees template-space diff" {
    # The headline scenario: user edits a deployed config, runs
    # `git status`, sees the .tmpl source as modified — without
    # ever invoking dodot directly.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # Install just the filter (alias optional — we'll drive refresh
    # explicitly in this test).
    run dodot template install-filter
    [ "$status" -eq 0 ]
    echo '*.tmpl filter=dodot-template' > "$DOTFILES_ROOT/.gitattributes"

    # Initial commit so we have a baseline tree to diff against.
    git -C "$DOTFILES_ROOT" add -A
    git -C "$DOTFILES_ROOT" commit -q -m "initial"

    # The user edits the deployed file (or its symlink target).
    sleep 1
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    # Without refresh, git's stat-cache won't even invoke the
    # filter. Run refresh as the alias would.
    run dodot refresh --quiet
    [ "$status" -eq 0 ]

    # `git status` sees the source as modified.
    run git -C "$DOTFILES_ROOT" status --porcelain
    [[ "$output" == *"app/config.toml.tmpl"* ]]

    # `git diff` shows the template-space change: static line lands
    # in the .tmpl, variable preserved.
    run git -C "$DOTFILES_ROOT" diff app/config.toml.tmpl
    [[ "$output" == *"port = 9999"* ]]
}

@test "workflow: commit refused on unresolved markers, accepted when clean" {
    # The pre-commit hook (R4 + R6 upgrade) blocks commits when
    # any template source carries unresolved dodot-conflict markers,
    # and lets clean states through.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    run dodot transform install-hook
    [ "$status" -eq 0 ]

    # Initial commit (clean state, hook should let it through).
    git -C "$DOTFILES_ROOT" add -A
    run git -C "$DOTFILES_ROOT" commit -m "initial"
    [ "$status" -eq 0 ]

    # Inject a marker block into the source — simulating a previous
    # `dodot transform check` run that detected an ambiguous edit.
    cat > "$DOTFILES_ROOT/app/config.toml.tmpl" <<'EOF'
name = {{ name }}
>>>>>> dodot-conflict (template)
host = "localhost"
====== dodot-conflict (deployed)
host = "production"
<<<<<< dodot-conflict
EOF

    # Hook fires `dodot transform check --strict` → finds markers →
    # exits 1 → commit blocked.
    git -C "$DOTFILES_ROOT" add -A
    run git -C "$DOTFILES_ROOT" commit -m "should fail"
    [ "$status" -ne 0 ]
    assert_output_contains "dodot-conflict"

    # Resolve manually (pick the deployed value) and retry.
    cat > "$DOTFILES_ROOT/app/config.toml.tmpl" <<'EOF'
name = {{ name }}
host = "production"
EOF
    git -C "$DOTFILES_ROOT" add -A
    run git -C "$DOTFILES_ROOT" commit -m "resolved"
    [ "$status" -eq 0 ]
}

@test "workflow: transform status reflects each state across the lifecycle" {
    # Pin the read-only `transform status` output as the user
    # progresses through the editing lifecycle: clean → output
    # changed → re-up → clean again.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # 1. Clean post-up.
    run dodot transform status
    assert_output_contains "1 synced"

    # 2. Edit deployed.
    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    echo "name = Alice" > "$rendered"  # pure-data-equivalent rewrite
    sleep 1
    echo "name = Alice EDITED" > "$rendered"

    run dodot transform status
    assert_output_contains "diverged"
    assert_output_contains "output_changed"

    # 3. Plain `dodot up` preserves the user's edit (issue #110): the
    #    divergence guard refuses to overwrite divergent deployed
    #    files, so `transform status` continues to report diverged.
    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "preserved"
    run dodot transform status
    assert_output_contains "diverged"

    # 4. `dodot up --force` is the documented escape hatch — re-renders,
    #    returns to clean.
    run dodot up --force
    [ "$status" -eq 0 ]
    run dodot transform status
    assert_output_contains "1 synced"
    assert_output_contains "0 diverged"
}

# ── The reinstall path ──────────────────────────────────────────

@test "reinstall ladder: hook block upgrades when re-running install-hook" {
    # Models the post-R4 → post-R6 user upgrade path: someone who
    # installed the hook in R4 (single-line check, no refresh) re-
    # runs install-hook after upgrading dodot, gets the new
    # two-line block automatically.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    # Stage an old R4-shape block.
    cat > "$DOTFILES_ROOT/.git/hooks/pre-commit" <<'EOF'
#!/bin/sh
echo 'user pre-commit step'
# >>> dodot transform check --strict (managed by `dodot transform install-hook`) >>>
# Old R4-style block.
dodot transform check --strict || exit 1
# <<< dodot transform check --strict <<<
echo 'user post-block step'
EOF
    chmod +x "$DOTFILES_ROOT/.git/hooks/pre-commit"

    run dodot transform install-hook
    [ "$status" -eq 0 ]

    body="$(cat "$DOTFILES_ROOT/.git/hooks/pre-commit")"
    # Both new commands.
    [[ "$body" == *"dodot refresh --quiet"* ]]
    [[ "$body" == *"dodot transform check --strict"* ]]
    # User content (before AND after) survived.
    [[ "$body" == *"user pre-commit step"* ]]
    [[ "$body" == *"user post-block step"* ]]
    # Single managed block.
    [ "$(echo "$body" | grep -c 'dodot transform install-hook' || true)" = "1" ]
}

# ── Tier 2 alias (the "ambient git status" experience) ──────────

@test "alias: install + simulated source produces wrapped git in shell" {
    # bats can't actually `source ~/.zshrc` (we're running in bash),
    # but we can verify the alias install lands the right line and
    # check that running `git` through a shell that sources the rc
    # picks up the wrap. Use a dummy alias body so we don't actually
    # invoke dodot; this test focuses on the install + sourcing
    # mechanism.
    run dodot git-install-alias --shell bash
    [ "$status" -eq 0 ]

    # Verify the alias landed.
    assert_file_contains "$HOME/.bashrc" "alias git='dodot refresh --quiet && command git'"

    # Source the rc in a sub-bash and verify the alias is defined.
    output="$(bash -c "source $HOME/.bashrc && alias git" 2>&1 || true)"
    [[ "$output" == *"dodot refresh --quiet && command git"* ]]
}
