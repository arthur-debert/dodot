# Test zshrc for dodot integration testing
export EDITOR=vim
export PATH="$HOME/.local/bin:$PATH"

# Test prompt
PROMPT='%F{green}%n@%m%f:%F{blue}%~%f$ '

# Enable command history
HISTFILE=~/.zsh_history
HISTSIZE=1000
SAVEHIST=1000

# Test aliases
alias ll='ls -la'
alias gs='git status'

# Marker for testing
echo "Test .zshrc loaded successfully"