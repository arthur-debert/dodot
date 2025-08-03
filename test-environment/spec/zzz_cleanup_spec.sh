#!/bin/zsh
# This spec runs last (alphabetically) to ensure final cleanup

Describe 'Test Environment Final Cleanup'
  It 'performs final cleanup'
    # Clean all test artifacts
    rm -rf /tmp/test-home 2>/dev/null || true
    rm -rf /tmp/test-dotfiles 2>/dev/null || true
    
    # Clean any dodot directories from root
    rm -rf /root/.local/share/dodot 2>/dev/null || true
    rm -rf /root/.config/dodot 2>/dev/null || true
    rm -rf /root/.cache/dodot 2>/dev/null || true
    rm -rf /root/.local/state/dodot 2>/dev/null || true
    
    # Clean test logs
    rm -f /tmp/brew-calls.log 2>/dev/null || true
    rm -f /tmp/*.log 2>/dev/null || true
    rm -f /tmp/*.marker 2>/dev/null || true
    rm -f /tmp/*.txt 2>/dev/null || true
    
    # Restore original environment variables
    if [ -n "${ORIGINAL_HOME:-}" ]; then
      export HOME="$ORIGINAL_HOME"
    fi
    if [ -n "${ORIGINAL_DOTFILES_ROOT:-}" ]; then
      export DOTFILES_ROOT="$ORIGINAL_DOTFILES_ROOT"
    fi
    
    # Unset all test variables
    unset TEST_HOME TEST_DOTFILES_ROOT
    unset DOTFILES_HOME DODOT_DATA_DIR DODOT_CONFIG_DIR DODOT_CACHE_DIR DODOT_DEBUG
    
    The status should be success
  End
End