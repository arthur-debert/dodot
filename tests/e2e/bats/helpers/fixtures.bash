#!/usr/bin/env bash
# Fixture creation helpers — shell equivalent of TempEnvironment builder.
#
# All paths are relative to the current sandbox's DOTFILES_ROOT and HOME.

# Create a pack directory.
# Usage: create_pack "vim"
create_pack() {
    local pack="$1"
    mkdir -p "$DOTFILES_ROOT/$pack"
}

# Create a file inside a pack.
# Usage: create_pack_file "vim" "vimrc" "set nocompatible"
# Handles nested paths (creates parent dirs).
create_pack_file() {
    local pack="$1"
    local rel_path="$2"
    local contents="${3:-}"
    local full_path="$DOTFILES_ROOT/$pack/$rel_path"

    mkdir -p "$(dirname "$full_path")"
    printf '%b' "$contents" > "$full_path"
}

# Write a .dodot.toml config for a specific pack.
# Usage: create_pack_config "vim" '[pack]\nignore = ["*.bak"]'
create_pack_config() {
    local pack="$1"
    local toml="$2"

    printf '%b' "$toml" > "$DOTFILES_ROOT/$pack/.dodot.toml"
}

# Write a .dodot.toml at the dotfiles root.
# Usage: create_root_config '[symlink]\nstrip_dot_prefix = true'
create_root_config() {
    local toml="$1"
    printf '%b' "$toml" > "$DOTFILES_ROOT/.dodot.toml"
}

# Mark a pack as ignored by creating .dodotignore.
# Usage: mark_ignored "disabled-pack"
mark_ignored() {
    local pack="$1"
    mkdir -p "$DOTFILES_ROOT/$pack"
    touch "$DOTFILES_ROOT/$pack/.dodotignore"
}

# Create a file under the simulated HOME directory.
# Useful for testing adopt.
# Usage: create_home_file ".vimrc" "set nocompatible"
create_home_file() {
    local rel_path="$1"
    local contents="${2:-}"
    local full_path="$HOME/$rel_path"

    mkdir -p "$(dirname "$full_path")"
    printf '%b' "$contents" > "$full_path"
}

# Create an executable script inside a pack.
# Usage: create_pack_script "tools" "install.sh" '#!/bin/sh
# echo "installed" > "$HOME/.tools-installed"'
create_pack_script() {
    local pack="$1"
    local rel_path="$2"
    local contents="$3"

    create_pack_file "$pack" "$rel_path" "$contents"
    chmod +x "$DOTFILES_ROOT/$pack/$rel_path"
}

# Create a bin directory inside a pack with executable scripts.
# Usage: create_pack_bin "tools" "myscript" '#!/bin/sh
# echo hello'
create_pack_bin() {
    local pack="$1"
    local script_name="$2"
    local contents="$3"

    create_pack_file "$pack" "bin/$script_name" "$contents"
    chmod +x "$DOTFILES_ROOT/$pack/bin/$script_name"
}
