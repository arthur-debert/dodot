#!/usr/bin/env bash
# Neovim aliases

alias nv="nvim"
alias vi="nvim"

# For integration testing
export DODOT_INSTALL_FLAG="$DODOT_INSTALL_FLAG:${BASH_SOURCE#$DOTFILES_ROOT/}"