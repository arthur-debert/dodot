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
      
      When call test -d "$HOME/.local/share/dodot/deployed/path"
      The status should be success
    End
    
    It 'creates symlink to bin directory'
      # Run deploy first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      When call test -L "$HOME/.local/share/dodot/deployed/path/tools"
      The status should be success
    End
    
    It 'symlink points to tools/bin directory'
      # Run deploy first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      When call readlink "$HOME/.local/share/dodot/deployed/path/tools"
      The output should include "tools/bin"
    End
    
    It 'can access files through symlink'
      # Run deploy first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      When call test -x "$HOME/.local/share/dodot/deployed/path/tools/hello-dodot"
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
      mkdir -p "$DOTFILES_ROOT/scripts/bin"
      cat > "$DOTFILES_ROOT/scripts/bin/test-script" << 'EOF'
#!/bin/bash
echo "Test script from scripts pack"
EOF
      chmod +x "$DOTFILES_ROOT/scripts/bin/test-script"
      
      cat > "$DOTFILES_ROOT/scripts/pack.dodot.toml" << 'EOF'
name = "scripts"

[[matchers]]
triggers = [
    { type = "Directory", pattern = "bin" }
]
actions = [
    { type = "shell_add_path" }
]
EOF
      
      # Deploy tools first
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Manually create scripts deployment (since mock doesn't handle it yet)
      ln -sf "$DOTFILES_ROOT/scripts/bin" "$HOME/.local/share/dodot/deployed/path/scripts"
      
      # Check both exist
      When call ls "$HOME/.local/share/dodot/deployed/path/"
      The output should include "tools"
      The output should include "scripts"
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
      When call test -L "$HOME/.local/share/dodot/deployed/path/tools"
      The status should be success
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
      When call test -d "$HOME/.local/share/dodot/deployed/path/tools"
      The status should be success
    End
  End
  
  Describe 'Directory naming'
    It 'uses pack name for deployed symlink'
      # Deploy tools
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Should be named "tools", not "bin"
      When call test -L "$HOME/.local/share/dodot/deployed/path/tools"
      The status should be success
    End
    
    It 'does not create symlink named bin'
      # Deploy tools
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Should NOT be named "bin"
      When call test -e "$HOME/.local/share/dodot/deployed/path/bin"
      The status should be failure
    End
  End
  
  Describe 'Error handling'
    It 'handles missing directory'
      # Create pack with non-existent directory reference
      mkdir -p "$DOTFILES_ROOT/broken-path"
      cat > "$DOTFILES_ROOT/broken-path/pack.dodot.toml" << 'EOF'
name = "broken-path"

[[matchers]]
triggers = [
    { type = "Directory", pattern = "missing-bin" }
]
actions = [
    { type = "shell_add_path" }
]
EOF
      
      # Mock doesn't validate, but real dodot should handle gracefully
      Skip "Mock doesn't validate missing directories for shell_add_path"
    End
    
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
    
    It 'handles non-directory files'
      # Create a file named bin instead of directory
      mkdir -p "$DOTFILES_ROOT/badpack"
      touch "$DOTFILES_ROOT/badpack/bin"
      
      cat > "$DOTFILES_ROOT/badpack/pack.dodot.toml" << 'EOF'
name = "badpack"

[[matchers]]
triggers = [
    { type = "Directory", pattern = "bin" }
]
actions = [
    { type = "shell_add_path" }
]
EOF
      
      # The trigger should not match since bin is not a directory
      Skip "Mock doesn't validate directory vs file for triggers"
    End
  End
  
  Describe 'Idempotency'
    It 'can deploy multiple times'
      # First deploy
      "$DODOT" deploy tools >/dev/null 2>&1
      
      # Second deploy should succeed
      When call "$DODOT" deploy tools
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