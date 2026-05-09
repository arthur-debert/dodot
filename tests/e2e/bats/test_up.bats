#!/usr/bin/env bats
# E2E tests for `dodot up`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "up deploys symlink files" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"

    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "Packs deployed"

    # Double-link chain: source -> datastore -> ~/.vimrc
    assert_double_link "vim" "symlink" "home.vimrc" \
        "$DOTFILES_ROOT/vim/home.vimrc" \
        "$HOME/.vimrc"

    # Content should be readable through the symlink chain
    assert_file_contains "$HOME/.vimrc" "set nocompatible"
}

@test "up deploys multiple files in a pack" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"
    create_pack_file "vim" "home.gvimrc" "set guifont=Mono"

    dodot up
    assert_symlink "$HOME/.vimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.vimrc"
    assert_symlink "$HOME/.gvimrc" "$XDG_DATA_HOME/dodot/packs/vim/symlink/home.gvimrc"
}

@test "up deploys shell files" {
    create_pack_file "zsh" "aliases.sh" "alias ll='ls -la'"

    dodot up

    # Shell file should be staged in datastore
    assert_exists "$XDG_DATA_HOME/dodot/packs/zsh/shell/aliases.sh"

    # Shell init script should reference it
    assert_file_contains "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" "aliases.sh"
}

@test "up sources arbitrary-named shell files at pack root" {
    # Default `[mappings] shell` is `["*.sh", "*.bash", "*.zsh"]` —
    # any shell-extension file at the pack's root gets sourced, not
    # just a fixed allowlist of names. Mirrors the holman-style
    # convention used in popular dotfile repos.
    create_pack_file "shell" "path.sh" "export DODOT_TEST_PATH=1"
    create_pack_file "shell" "functions.zsh" "function dodot_fn() { :; }"
    create_pack_file "shell" "50_prompt.bash" "export DODOT_TEST_PROMPT=1"

    dodot up

    # All three should be staged under the shell handler's datastore dir.
    assert_exists "$XDG_DATA_HOME/dodot/packs/shell/shell/path.sh"
    assert_exists "$XDG_DATA_HOME/dodot/packs/shell/shell/functions.zsh"
    assert_exists "$XDG_DATA_HOME/dodot/packs/shell/shell/50_prompt.bash"

    # And the init script should source each one.
    assert_file_contains "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" "path.sh"
    assert_file_contains "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" "functions.zsh"
    assert_file_contains "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" "50_prompt.bash"
}

@test "up does not source shell files in pack subdirectories" {
    # Recursion safety: a window-manager helper script under
    # ~/.config/<wm>/scripts/foo.sh lives in the pack's subtree as a
    # placed-and-executed-by-another-tool file, NOT a file to source
    # into the interactive shell. The depth-1 scanner contract means
    # the wildcard `*.sh` shell rule never reaches into subdirs;
    # those files flow through the symlink handler verbatim.
    create_pack_file "hypr" "hyprland.conf" "# config"
    create_pack_file "hypr" "scripts/workspace-switch.sh" "#!/bin/sh
hyprctl dispatch workspace +1"
    create_pack_file "hypr" "scripts/launcher.sh" "#!/bin/sh
rofi -show drun"

    dodot up

    # The nested .sh files MUST NOT show up under the shell handler's
    # staging dir — a regression here would mean dodot is sourcing
    # window-manager scripts on every shell startup.
    assert_not_exists "$XDG_DATA_HOME/dodot/packs/hypr/shell/workspace-switch.sh"
    assert_not_exists "$XDG_DATA_HOME/dodot/packs/hypr/shell/launcher.sh"

    # And the init script must not reference them.
    if [[ -f "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" ]]; then
        run grep -E "(workspace-switch|launcher)\.sh" \
            "$XDG_DATA_HOME/dodot/shell/dodot-init.sh"
        [ "$status" -ne 0 ] || {
            echo "init script contains nested .sh script — recursion safety violated" >&2
            cat "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" >&2
            return 1
        }
    fi
}

@test "up routes install.sh to install handler even under shell wildcard default" {
    # install.sh sits at priority 20, above the priority-10 `*.sh`
    # shell wildcard, so the install hook runs once instead of being
    # silently sourced into every shell session.
    create_pack_script "tools" "install.sh" '#!/bin/sh
touch "$HOME/.dodot-install-marker"'

    dodot up

    # The install hook should have been invoked.
    assert_exists "$HOME/.dodot-install-marker"

    # install.sh should NOT have been staged for shell sourcing.
    assert_not_exists "$XDG_DATA_HOME/dodot/packs/tools/shell/install.sh"

    # init script must not source install.sh.
    if [[ -f "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" ]]; then
        run grep -F "install.sh" "$XDG_DATA_HOME/dodot/shell/dodot-init.sh"
        [ "$status" -ne 0 ] || {
            echo "init script sources install.sh — install hook would re-run on every shell" >&2
            return 1
        }
    fi
}

@test "up deploys path directories" {
    create_pack "tools"
    create_pack_bin "tools" "mytool" '#!/bin/sh\necho hello'

    dodot up

    # Path dir should be staged in datastore
    assert_exists "$XDG_DATA_HOME/dodot/packs/tools/path/bin"

    # Init script should add bin to PATH
    assert_file_contains "$XDG_DATA_HOME/dodot/shell/dodot-init.sh" "PATH="
}

@test "up --dry-run shows plan without changes" {
    create_pack_file "vim" "home.vimrc" "x"

    run dodot up --dry-run
    [ "$status" -eq 0 ]
    assert_output_contains "dry run"

    # Nothing should actually be deployed
    assert_not_exists "$HOME/.vimrc"
    assert_no_handler_state "vim" "symlink"
}

@test "up --no-provision skips install scripts" {
    create_pack_file "tools" "vimrc" "x"
    create_pack_script "tools" "install.sh" '#!/bin/sh\ntouch "$HOME/.tools-installed"'

    dodot up --no-provision

    # Install script should NOT have run
    assert_not_exists "$HOME/.tools-installed"
}

@test "up deploys selected packs only" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "git" "home.gitconfig" "x"

    dodot up vim

    # vim should be deployed
    assert_exists "$HOME/.vimrc"
    # git should NOT be deployed
    assert_not_exists "$HOME/.gitconfig"
}

@test "up is idempotent" {
    create_pack_file "vim" "home.vimrc" "set nocompatible"

    dodot up
    assert_exists "$HOME/.vimrc"

    # Second up should succeed without error
    run dodot up
    [ "$status" -eq 0 ]

    # Symlink should still work
    assert_file_contains "$HOME/.vimrc" "set nocompatible"
}

@test "up deploys multiple packs" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "git" "home.gitconfig" "x"

    dodot up
    assert_exists "$HOME/.vimrc"
    assert_exists "$HOME/.gitconfig"
}

@test "up skips ignored packs" {
    create_pack_file "vim" "home.vimrc" "x"
    create_pack_file "disabled" "file" "x"
    mark_ignored "disabled"

    dodot up

    assert_exists "$HOME/.vimrc"
    assert_not_exists "$HOME/.file"
}
