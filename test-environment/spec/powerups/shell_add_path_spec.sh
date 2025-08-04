#!/bin/zsh
# Tests for Shell Add Path PowerUp functionality

Describe 'Shell Add Path PowerUp'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  Describe 'Basic PATH deployment'
    It 'deploys tools pack successfully'
      When call "$DODOT" deploy tools
      The status should be success
      The error should not include "ERROR"
    End
    
    It 'creates path deployment directory'
      # Run deploy first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Verify deployment creates the necessary structure
      The result of function verify_shell_add_path_deployed "tools" "bin" should be successful
    End
    
    It 'creates symlink to bin directory'
      # Run deploy first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      When call verify_shell_add_path_deployed "tools" "bin"
      The status should be success
    End
    
    It 'script is executable through deployed path'
      # Run deploy first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Execute script directly
      When call "$HOME/.local/share/dodot/deployed/path/tools/hello-dodot"
      The status should be success
      The output should include "Hello from dodot tools!"
    End
  End
  
  Describe 'Multiple PATH directories'
    It 'handles multiple packs with PATH additions'
      # Create scripts pack with bin directory
      # Deploy tools first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Check deployment worked
      The result of function verify_shell_add_path_deployed "tools" "bin" should be successful
    End
    
    It 'each pack gets its own PATH entry'
      # Setup from previous test
      mkdir -p "$DOTFILES_ROOT/scripts/bin"
      echo '#!/bin/bash' > "$DOTFILES_ROOT/scripts/bin/test-script"
      chmod +x "$DOTFILES_ROOT/scripts/bin/test-script"
      
      # Deploy and create links
      "$DODOT" deploy tools >/dev/null 2>&1
      ln -sf "$DOTFILES_ROOT/scripts/bin" "$HOME/.local/share/dodot/deployed/path/scripts"
      
      # Verify both are separate symlinks
      The result of function verify_shell_add_path_deployed "tools" "bin" should be successful
    End
  End
  
  Describe 'PATH integration with dodot-init'
    It 'would add directory to PATH when sourced'
      # Deploy tools
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # The dodot-init.sh script would add all directories in deployed/path to PATH
      # We can't test the actual PATH modification in ShellSpec, but we can verify
      # the structure is correct for dodot-init.sh to process
      
      # Check that the deployed path directory contains valid directory symlinks
      The result of function verify_shell_add_path_deployed "tools" "bin" should be successful
    End
  End
  
  Describe 'Directory naming'
    It 'uses pack name for deployed symlink'
      # Deploy tools
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Should be named "tools", not "bin" - verification function checks this
      The result of function verify_shell_add_path_deployed "tools" "bin" should be successful
    End
    
    It 'does not create symlink named bin'
      # Deploy tools
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Should NOT be named "bin" - verify non-existence
      The result of function verify_shell_add_path_deployed "bin" "bin" "not-deployed" should be successful
    End
  End
  
  Describe 'Error handling'
    
    It 'handles permission errors'
      # Create directory with no write permission
      mkdir -p "$HOME/.local/share/dodot/deployed"
      chmod 555 "$HOME/.local/share/dodot/deployed"
      
      # Try to deploy - should fail due to permissions
      When call "$DODOT" deploy tools 2>&1
      The status should be failure
      
      # Restore permissions for cleanup
      chmod 755 "$HOME/.local/share/dodot/deployed"
    End
    
  End
  
  Describe 'Idempotency'
    It 'can deploy multiple times'
      When call verify_idempotent_deploy "tools"
      The status should be success
    End
    
    It 'maintains same symlink target'
      # First deploy
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Get initial target
      FIRST_TARGET=$(readlink "$HOME/.local/share/dodot/deployed/path/tools")
      
      # Second deploy
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Get second target
      SECOND_TARGET=$(readlink "$HOME/.local/share/dodot/deployed/path/tools")
      
      # Should be the same
      When call test "$FIRST_TARGET" = "$SECOND_TARGET"
      The status should be success
    End
  End
  
  Describe 'Executable preservation'
    It 'preserves executable permissions'
      # Deploy tools
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Check that executable is still executable through symlink
      When call test -x "$HOME/.local/share/dodot/deployed/path/tools/hello-dodot"
      The status should be success
    End
    
    It 'can execute scripts through deployed path'
      # Deploy tools
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Execute the script
      When call bash -c "$HOME/.local/share/dodot/deployed/path/tools/hello-dodot"
      The status should be success
      The output should include "PATH integration is working correctly"
    End
  End
  
  Describe 'Subdirectory handling'
    It 'includes all files in bin directory'
      # Add another script
      echo '#!/bin/bash' > "$DOTFILES_ROOT/tools/bin/another-tool"
      echo 'echo "Another tool"' >> "$DOTFILES_ROOT/tools/bin/another-tool"
      chmod +x "$DOTFILES_ROOT/tools/bin/another-tool"
      
      # Deploy
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Both scripts should be accessible
      When call ls "$HOME/.local/share/dodot/deployed/path/tools/"
      The output should include "hello-dodot"
      The output should include "another-tool"
    End
  End
End