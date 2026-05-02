#!/usr/bin/env bats
# E2E tests for R7: `dodot transform status`, `dodot git-show-alias`,
# `dodot git-install-alias`. The first is a passive view of the
# divergence cache; the latter two are the Tier 2 shell-side glue
# that wraps `git` in `dodot refresh --quiet`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

# ── transform status ────────────────────────────────────────────

@test "transform status on empty cache reports nothing-to-do" {
    run dodot transform status
    [ "$status" -eq 0 ]
    assert_output_contains "No template baselines"
}

@test "transform status reports synced after up" {
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    run dodot transform status
    [ "$status" -eq 0 ]
    assert_output_contains "1 synced"
    assert_output_contains "synced"
}

@test "transform status reports output_changed when deployed file diverges" {
    # Edit the rendered file post-up; status surfaces output_changed
    # without running the reverse-merge engine (purely informational).
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    cat > "$rendered" <<'EOF'
name = Alice
port = 9999
EOF

    run dodot transform status
    [ "$status" -eq 0 ]
    assert_output_contains "output_changed"
    assert_output_contains "diverged"
}

@test "transform status does not mutate sources" {
    # status is read-only; pin that with an explicit sha check
    # before/after.
    create_pack "app"
    create_pack_file "app" "config.toml.tmpl" 'name = {{ name }}
port = 5432'
    create_pack_config "app" '[preprocessor.template.vars]
name = "Alice"'

    run dodot up
    [ "$status" -eq 0 ]

    src="$DOTFILES_ROOT/app/config.toml.tmpl"
    before=$(shasum -a 256 "$src" | awk '{print $1}')

    rendered="$XDG_DATA_HOME/dodot/packs/app/preprocessed/config.toml"
    echo "name = Alice
port = 9999" > "$rendered"

    run dodot transform status
    [ "$status" -eq 0 ]

    after=$(shasum -a 256 "$src" | awk '{print $1}')
    [ "$before" = "$after" ]
}

# ── git-show-alias ──────────────────────────────────────────────

@test "git-show-alias prints the alias block for the requested shell" {
    run dodot git-show-alias --shell zsh
    [ "$status" -eq 0 ]
    assert_output_contains "alias git='dodot refresh --quiet && command git'"
    assert_output_contains "~/.zshrc"
}

@test "git-show-alias rejects unknown shells with a clear error" {
    run dodot git-show-alias --shell fish
    [ "$status" -ne 0 ]
    assert_output_contains "fish"
    assert_output_contains "bash"
}

@test "git-show-alias does not modify the rc file" {
    rc="$HOME/.zshrc"
    [ ! -f "$rc" ]
    run dodot git-show-alias --shell zsh
    [ "$status" -eq 0 ]
    [ ! -f "$rc" ]
}

# ── git-install-alias ───────────────────────────────────────────

@test "git-install-alias creates rc file when absent" {
    rc="$HOME/.zshrc"
    [ ! -f "$rc" ]

    run dodot git-install-alias --shell zsh
    [ "$status" -eq 0 ]
    assert_output_contains "Created"

    [ -f "$rc" ]
    assert_file_contains "$rc" "alias git='dodot refresh --quiet && command git'"
}

@test "git-install-alias appends to an existing rc file" {
    rc="$HOME/.bashrc"
    cat > "$rc" <<'EOF'
export PATH="/usr/local/bin:$PATH"
alias ll='ls -l'
EOF

    run dodot git-install-alias --shell bash
    [ "$status" -eq 0 ]
    assert_output_contains "Appended"

    # Existing content survived.
    assert_file_contains "$rc" "alias ll='ls -l'"
    assert_file_contains "$rc" "/usr/local/bin"
    # Our block landed.
    assert_file_contains "$rc" "alias git='dodot refresh --quiet && command git'"
}

@test "git-install-alias is idempotent" {
    run dodot git-install-alias --shell zsh
    [ "$status" -eq 0 ]
    run dodot git-install-alias --shell zsh
    [ "$status" -eq 0 ]
    assert_output_contains "already present"
}

@test "git-install-alias suggests source command for the right shell" {
    run dodot git-install-alias --shell zsh
    [ "$status" -eq 0 ]
    assert_output_contains "source ~/.zshrc"
}
