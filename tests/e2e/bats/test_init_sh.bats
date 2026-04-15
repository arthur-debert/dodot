#!/usr/bin/env bats
# E2E tests for `dodot init-sh`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "init-sh outputs valid shell script" {
    run dodot init-sh
    [ "$status" -eq 0 ]
    # Should have a shebang or be sourceable
    assert_output_contains "#!/bin/sh"
}

@test "init-sh includes shell file source lines after up" {
    create_pack_file "zsh" "aliases.sh" "alias ll='ls -la'"
    dodot up

    run dodot init-sh
    [ "$status" -eq 0 ]
    assert_output_contains "aliases.sh"
    # Should use `. "path"` or `source "path"` syntax
    assert_output_contains ". \""
}

@test "init-sh includes PATH additions after up" {
    create_pack "tools"
    create_pack_bin "tools" "mytool" '#!/bin/sh\necho hello'
    dodot up

    run dodot init-sh
    [ "$status" -eq 0 ]
    assert_output_contains "PATH="
    assert_output_contains "bin"
}

@test "init-sh is empty when no packs deployed" {
    create_pack_file "vim" "vimrc" "x"
    # Don't deploy

    run dodot init-sh
    [ "$status" -eq 0 ]
    # Should still be a valid script but have no source/PATH lines
    assert_output_contains "#!/bin/sh"
}

@test "init-sh reflects changes after down" {
    create_pack_file "zsh" "aliases.sh" "alias ll='ls -la'"
    dodot up

    run dodot init-sh
    assert_output_contains "aliases.sh"

    dodot down

    run dodot init-sh
    assert_output_not_contains "aliases.sh"
}
