#!/usr/bin/env bats
# E2E tests for the cross-pack ordering prefix grammar.
#
# Covers: directory-name lex order drives apply order; recognised
# `NNN-` / `NNN_` prefixes are stripped for display and CLI argument
# resolution (canonical form is the stripped name); both forms
# (`dodot up nvim` and `dodot up 010-nvim`) resolve to the same pack;
# scan-time collisions (logical-name + multi-prefix) are surfaced as
# errors with both offending paths in the message.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "lex order drives shell-source order across prefixed packs" {
    instrumented_shell "010-brew" "aliases.sh" 'export PROBE_ORDER="${PROBE_ORDER:+$PROBE_ORDER,}brew"'
    instrumented_shell "100-zsh" "aliases.sh" 'export PROBE_ORDER="${PROBE_ORDER:+$PROBE_ORDER,}zsh"'
    instrumented_shell "900-prompt" "aliases.sh" 'export PROBE_ORDER="${PROBE_ORDER:+$PROBE_ORDER,}prompt"'

    dodot up
    eval_init_sh

    # Source order matches lex order on the directory names, which —
    # with the zero-padded prefix — happens to match numeric order.
    [ "$PROBE_ORDER" = "brew,zsh,prompt" ]
}

@test "dodot list shows display names, not raw directory names" {
    create_pack_file "010-brew" "Brewfile" "brew 'rg'"
    create_pack_file "100-zsh" "zshrc" "echo zsh"
    create_pack_file "vim" "vimrc" "set nocompatible"

    run dodot list
    [ "$status" -eq 0 ]
    # Display names — no `010-` / `100-` prefixes leaked through.
    assert_output_contains "brew"
    assert_output_contains "zsh"
    assert_output_contains "vim"
    assert_output_not_contains "010-brew"
    assert_output_not_contains "100-zsh"
}

@test "dodot status shows display names for prefixed packs" {
    create_pack_file "010-brew" "Brewfile" "brew 'rg'"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "brew"
    # The raw directory form must not surface in the status table.
    assert_output_not_contains "010-brew"
}

@test "dodot up <display-name> finds the prefixed pack" {
    instrumented_shell "010-brew" "aliases.sh" 'alias brewup="brew upgrade"'

    run dodot up brew
    [ "$status" -eq 0 ]
    assert_output_contains "deployed"

    # The shell source landed in the datastore under the raw
    # directory key (010-brew), per the proposal's invariant — the
    # display layer is the only place the prefix is hidden.
    assert_exists "$XDG_DATA_HOME/dodot/packs/010-brew/shell"

    # Status confirms the canonical form is `brew`.
    run dodot status brew
    [ "$status" -eq 0 ]
    assert_output_contains "brew"
    assert_output_not_contains "010-brew"
}

@test "dodot up <raw-directory-name> still works as a fallback" {
    instrumented_shell "010-brew" "aliases.sh" 'alias brewup="brew upgrade"'

    # Raw directory form: muscle-memory / scripts using the on-disk name.
    run dodot up 010-brew
    [ "$status" -eq 0 ]
    assert_output_contains "deployed"
    assert_exists "$XDG_DATA_HOME/dodot/packs/010-brew/shell"
}

@test "dodot down resolves both display and raw forms" {
    instrumented_shell "010-brew" "aliases.sh" 'alias brewup="brew upgrade"'

    dodot up

    # down by display name
    run dodot down brew
    [ "$status" -eq 0 ]
    assert_output_contains "deactivated"
    assert_no_handler_state "010-brew" "shell"

    dodot up
    # down by raw directory name
    run dodot down 010-brew
    [ "$status" -eq 0 ]
    assert_output_contains "deactivated"
    assert_no_handler_state "010-brew" "shell"
}

@test "shell init script emits comments with the display name" {
    instrumented_shell "010-brew" "aliases.sh" 'alias brewup="brew upgrade"'

    dodot up

    local init_script="$XDG_DATA_HOME/dodot/shell/dodot-init.sh"
    assert_exists "$init_script"
    # Comment uses the display name, not the raw directory.
    assert_file_contains "$init_script" "# \[brew\]"
    if grep -q '# \[010-brew\]' "$init_script"; then
        echo "init script leaked raw prefix in pack comment:" >&2
        cat "$init_script" >&2
        return 1
    fi
}

@test "symlink target uses the display name, not the raw directory" {
    # `010-nvim/init.lua` should land at `~/.config/nvim/init.lua` —
    # which is where neovim actually reads its config, regardless of
    # what the dodot pack directory is named.
    create_pack_file "010-nvim" "init.lua" "-- nvim config"

    dodot up
    assert_exists "$XDG_CONFIG_HOME/nvim/init.lua"
    assert_not_exists "$XDG_CONFIG_HOME/010-nvim"
}

@test "symlink target follows display name when user invokes by raw form" {
    create_pack_file "010-nvim" "init.lua" "-- nvim config"

    dodot up 010-nvim
    # Same target — canonicalisation applies regardless of how the
    # user spelt the pack on the CLI.
    assert_exists "$XDG_CONFIG_HOME/nvim/init.lua"
    assert_not_exists "$XDG_CONFIG_HOME/010-nvim"
}

@test "logical-name collision is rejected with both paths in the message" {
    create_pack_file "nvim" "init.lua" "x"
    create_pack_file "010-nvim" "init.lua" "y"

    run dodot status
    # The CLI surfaces scan-time errors via stdout + Error: prefix;
    # the canonical signal is the message body, not exit code.
    assert_output_contains "collision"
    assert_output_contains "nvim"
    assert_output_contains "010-nvim"
}

@test "multi-prefix collision is rejected" {
    create_pack_file "010-nvim" "init.lua" "x"
    create_pack_file "020-nvim" "init.lua" "y"

    run dodot status
    assert_output_contains "collision"
    assert_output_contains "010-nvim"
    assert_output_contains "020-nvim"
}

@test "empty-stem prefix directory is rejected" {
    mkdir -p "$DOTFILES_ROOT/010-"
    # Add a placeholder so the directory survives as scan-eligible.
    touch "$DOTFILES_ROOT/010-/keep"

    run dodot status
    # The error mentions the empty-stem reason and the raw directory.
    assert_output_contains "010-"
    assert_output_contains "ordering prefix"
}

@test "same prefix with different stems is allowed" {
    create_pack_file "010-brew" "Brewfile" "brew 'rg'"
    create_pack_file "010-zsh" "zshrc" "echo zsh"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "brew"
    assert_output_contains "zsh"
}

@test "unprefixed packs interleave with prefixed ones in lex order" {
    # Lex: 010-brew < 020-zsh < nvim < starship
    instrumented_shell "010-brew" "aliases.sh" 'export PROBE_ORDER="${PROBE_ORDER:+$PROBE_ORDER,}brew"'
    instrumented_shell "020-zsh" "aliases.sh" 'export PROBE_ORDER="${PROBE_ORDER:+$PROBE_ORDER,}zsh"'
    instrumented_shell "nvim" "aliases.sh" 'export PROBE_ORDER="${PROBE_ORDER:+$PROBE_ORDER,}nvim"'
    instrumented_shell "starship" "aliases.sh" 'export PROBE_ORDER="${PROBE_ORDER:+$PROBE_ORDER,}starship"'

    dodot up
    eval_init_sh
    [ "$PROBE_ORDER" = "brew,zsh,nvim,starship" ]
}
