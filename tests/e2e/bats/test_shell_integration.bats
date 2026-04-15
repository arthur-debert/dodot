#!/usr/bin/env bats
# E2E tests for shell integration (eval "$(dodot init-sh)").
#
# These tests verify that sourcing dodot's init script in a
# real shell actually makes aliases and PATH changes take effect.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "sourcing init-sh makes shell aliases available" {
    create_pack_file "zsh" "aliases.sh" 'alias dodot_test_alias="echo it works"'
    dodot up

    # Source the init script in a subshell and check the alias
    result="$(bash -c "
        export HOME='$HOME'
        export XDG_DATA_HOME='$XDG_DATA_HOME'
        export DOTFILES_ROOT='$DOTFILES_ROOT'
        export XDG_CONFIG_HOME='$XDG_CONFIG_HOME'
        export XDG_CACHE_HOME='$XDG_CACHE_HOME'
        shopt -s expand_aliases
        eval \"\$($DODOT_BIN init-sh)\"
        dodot_test_alias
    " 2>&1)"

    [ "$result" = "it works" ]
}

@test "sourcing init-sh adds bin dirs to PATH" {
    create_pack "tools"
    create_pack_bin "tools" "dodot-test-tool" '#!/bin/sh
echo "tool output"'
    dodot up

    # Source init-sh and check PATH
    result="$(bash -c "
        export HOME='$HOME'
        export XDG_DATA_HOME='$XDG_DATA_HOME'
        export DOTFILES_ROOT='$DOTFILES_ROOT'
        export XDG_CONFIG_HOME='$XDG_CONFIG_HOME'
        export XDG_CACHE_HOME='$XDG_CACHE_HOME'
        eval \"\$($DODOT_BIN init-sh)\"
        dodot-test-tool
    " 2>&1)"

    [ "$result" = "tool output" ]
}

@test "shell integration handles multiple packs" {
    create_pack_file "shell1" "aliases.sh" 'alias test_a="echo a"'
    create_pack_file "shell2" "aliases.sh" 'alias test_b="echo b"'
    dodot up

    result="$(bash -c "
        export HOME='$HOME'
        export XDG_DATA_HOME='$XDG_DATA_HOME'
        export DOTFILES_ROOT='$DOTFILES_ROOT'
        export XDG_CONFIG_HOME='$XDG_CONFIG_HOME'
        export XDG_CACHE_HOME='$XDG_CACHE_HOME'
        shopt -s expand_aliases
        eval \"\$($DODOT_BIN init-sh)\"
        test_a
        test_b
    " 2>&1)"

    echo "$result" | grep -q "a"
    echo "$result" | grep -q "b"
}
