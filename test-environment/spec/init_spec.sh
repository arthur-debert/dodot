#!/bin/zsh
# This spec runs first (alphabetically) to ensure clean initial state

Describe 'Test Environment Initialization'
  It 'starts with a clean environment'
    # Clean any leftover dodot directories from previous runs
    # This handles the case where container state persists
    
    # Clean from root's home (container runs as root)
    When run rm -rf /root/.local/share/dodot /root/.config/dodot /root/.cache/dodot /root/.local/state/dodot
    The status should be success
    
    # Clean from any test directories
    When run rm -rf /tmp/test-home/.local/share/dodot /tmp/test-home/.config/dodot /tmp/test-home/.cache/dodot /tmp/test-home/.local/state/dodot
    The status should be success
    
    # Clean test logs
    When run rm -f /tmp/brew-calls.log
    The status should be success
  End
  
  It 'creates required directories'
    # Ensure test directories exist
    mkdir -p /tmp
    The path "/tmp" should be directory
  End
End