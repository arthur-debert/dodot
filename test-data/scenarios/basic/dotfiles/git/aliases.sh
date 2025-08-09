#!/usr/bin/env bash
# Git aliases

alias g="git"
alias gs="git status"
alias gc="git commit"

# For integration testing
export DODOT_INSTALL_FLAG="$DODOT_INSTALL_FLAG:${BASH_SOURCE#$DOTFILES_ROOT/}"