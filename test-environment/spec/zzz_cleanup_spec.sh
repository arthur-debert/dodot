#!/bin/zsh
# This spec runs last (alphabetically) to ensure final cleanup

Describe 'Test Environment Final Cleanup'
  It 'performs final cleanup'
    # Clean all test artifacts
    When run rm -rf /tmp/test-home /tmp/test-dotfiles
    The status should be success
    
    # Clean any dodot directories from root
    When run rm -rf /root/.local/share/dodot /root/.config/dodot /root/.cache/dodot /root/.local/state/dodot
    The status should be success
    
    # Clean test logs and markers
    When run sh -c "rm -f /tmp/brew-calls.log /tmp/*.log /tmp/*.marker /tmp/*.txt 2>/dev/null || true"
    The status should be success
  End
End