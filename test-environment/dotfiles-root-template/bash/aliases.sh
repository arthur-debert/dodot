#!/usr/bin/env sh
# Shell aliases for bash pack

# Git aliases
alias g='git'
alias gs='git status'
alias gc='git commit'
alias gp='git push'

# Directory navigation
alias ll='ls -la'
alias la='ls -A'
alias l='ls -CF'

# Safety aliases
alias rm='rm -i'
alias cp='cp -i'
alias mv='mv -i'

# Custom test alias for verification
alias dodot-test='echo "dodot bash aliases loaded successfully"'

# Marker for shell profile verification
BASH_PROFILE_LOADED=1