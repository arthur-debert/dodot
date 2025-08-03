#!/bin/zsh
# This spec runs first (alphabetically) to ensure clean initial state

Describe 'Test Environment Initialization'
  It 'starts with a clean environment'
    # Clean any leftover dodot directories from previous runs
    # This handles the case where container state persists
    
    # Clean from root's home (container runs as root)
    rm -rf /root/.local/share/dodot 2>/dev/null || true
    rm -rf /root/.config/dodot 2>/dev/null || true
    rm -rf /root/.cache/dodot 2>/dev/null || true
    rm -rf /root/.local/state/dodot 2>/dev/null || true
    
    # Clean from any test directories
    rm -rf /tmp/test-home/.local/share/dodot 2>/dev/null || true
    rm -rf /tmp/test-home/.config/dodot 2>/dev/null || true
    rm -rf /tmp/test-home/.cache/dodot 2>/dev/null || true
    rm -rf /tmp/test-home/.local/state/dodot 2>/dev/null || true
    
    # Unset any dodot environment variables
    unset DOTFILES_HOME DODOT_DATA_DIR DODOT_CONFIG_DIR DODOT_CACHE_DIR DODOT_DEBUG
    
    # Clean brew log
    rm -f /tmp/brew-calls.log
    
    # Success
    The status should be success
  End
  
  It 'creates required directories'
    # Ensure test directories exist
    mkdir -p /tmp
    The path "/tmp" should be directory
  End
End