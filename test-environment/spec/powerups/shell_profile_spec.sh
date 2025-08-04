#!/bin/zsh
# Tests for Shell Profile PowerUp functionality

Describe 'Shell Profile PowerUp'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  Describe 'Basic shell profile deployment'
    It 'deploys bash pack successfully'
      When call "$DODOT" deploy bash
      The status should be success
      The error should not include "ERROR"
    End
    
    It 'creates shell profile deployment structure'
      # Run deploy first
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Verification function checks directory structure is created correctly
      The result of function verify_shell_profile_deployed "bash" "aliases.sh" should be successful
    End
    
    It 'creates bash.sh symlink in shell_profile directory'
      # Run deploy first
      "$DODOT" deploy bash >/dev/null 2>&1
      
      When call verify_shell_profile_deployed "bash" "aliases.sh"
      The status should be success
    End
    
    It 'can read aliases through symlink'
      # Run deploy first
      "$DODOT" deploy bash >/dev/null 2>&1
      
      When call grep "alias g='git'" "$HOME/.local/share/dodot/deployed/shell_profile/bash.sh"
      The status should be success
      The output should include "alias g='git'"
    End
    
    It 'verifies shell profile is executable through source'
      # Run deploy first
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Directly source the deployed file and check alias
      When call bash -c "source $HOME/.local/share/dodot/deployed/shell_profile/bash.sh && alias g"
      The status should be success
      The output should include "alias g='git'"
    End
  End
  
  
  Describe 'Shell profile with custom names'
    It 'uses pack name for deployed file'
      # The deployed file should be named after the pack, not the source file
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Should be bash.sh, not aliases.sh  
      When call verify_shell_profile_deployed "bash" "aliases.sh"
      The status should be success
    End
    
    It 'does not create aliases.sh in deployed directory'
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Should NOT be named aliases.sh - verify using not-deployed mode
      # Note: We're checking a different filename that shouldn't exist
      The result of function verify_shell_profile_deployed "aliases" "aliases.sh" "not-deployed" should be successful
    End
  End
  
  Describe 'Error handling'
    
    It 'handles permission errors on deployment directory'
      # Create directory with no write permission
      mkdir -p "$HOME/.local/share/dodot/deployed"
      chmod 555 "$HOME/.local/share/dodot/deployed"
      
      # Try to deploy - should fail due to permissions
      When call "$DODOT" deploy bash 2>&1
      The status should be failure
      
      # Restore permissions for cleanup
      chmod 755 "$HOME/.local/share/dodot/deployed"
    End
  End
  
  Describe 'Idempotency'
    It 'can deploy multiple times successfully'
      When call verify_idempotent_deploy "bash"
      The status should be success
    End
    
    It 'maintains same symlink on repeated deploys'
      # First deploy
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Get initial link target
      FIRST_TARGET=$(readlink "$HOME/.local/share/dodot/deployed/shell_profile/bash.sh")
      
      # Second deploy
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Get second link target
      SECOND_TARGET=$(readlink "$HOME/.local/share/dodot/deployed/shell_profile/bash.sh")
      
      # Should be the same
      When call test "$FIRST_TARGET" = "$SECOND_TARGET"
      The status should be success
    End
  End
  
  Describe 'Integration with other powerups'
    It 'works alongside symlink powerup'
      # Deploy bash pack which has both symlink and shell_profile
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Check symlink was created using appropriate verification
      The result of function verify_symlink_deployed "bash" ".bashrc" should be successful
    End
    
    It 'creates shell_profile link alongside symlink'
      # Deploy bash pack
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Check shell_profile was created using verification function
      The result of function verify_shell_profile_deployed "bash" "aliases.sh" should be successful
    End
  End
  
  Describe 'Shell compatibility'
    It 'creates POSIX-compatible scripts'
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Check shebang is POSIX sh
      When call head -1 "$DOTFILES_ROOT/bash/aliases.sh"
      The output should include "#!/usr/bin/env sh"
    End
    
    It 'sources correctly in bash'
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Test sourcing in bash
      When call bash -c "source $HOME/.local/share/dodot/deployed/shell_profile/bash.sh && alias g"
      The status should be success
      The output should include "alias g='git'"
    End
    
    It 'sources correctly in zsh'
      "$DODOT" deploy bash >/dev/null 2>&1
      
      # Test sourcing in zsh - zsh alias output format is different
      When call zsh -c "source $HOME/.local/share/dodot/deployed/shell_profile/bash.sh && alias g"
      The status should be success
      The output should include "g=git"
    End
  End
End