#!/bin/zsh
# Tests for Brewfile PowerUp functionality (run-once)
#
# NOTE: These tests are currently marked as Pending because the brewfile powerup
# implementation in dodot only creates sentinel files but doesn't actually
# execute brew bundle. The convertBrewActionWithContext function needs to be
# updated to include brew bundle execution operations.

Describe 'Brewfile PowerUp'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  # Helper to create a brewfile pack
  create_brewfile() {
    local pack_name="$1"
    local brewfile_content="$2"
    
    mkdir -p "$TEST_DOTFILES_ROOT/$pack_name"
    
    # Create Brewfile with provided content
    cat > "$TEST_DOTFILES_ROOT/$pack_name/Brewfile" << EOF
$brewfile_content
EOF
    
    # Create pack.dodot.toml
    cat > "$TEST_DOTFILES_ROOT/$pack_name/pack.dodot.toml" << 'EOF'
[[brewfile]]
trigger = { directory = ".", recursive = false }
file_name = "Brewfile"
EOF
  }
  
  
  Describe 'Basic Brewfile processing'
    It 'processes valid Brewfile using mock brew'
      # Create Brewfile with basic formulas
      create_brewfile "dev-tools" '
# Development tools
brew "git"
brew "vim"
brew "tmux"
'
      
      # Clear brew log
      rm -f /tmp/brew-calls.log
      
      When call "$DODOT" install
      The status should be success
      The output should include "Brewfile"
      
      # Use our verification function
      The result of function verify_brewfile_deployed "dev-tools" should be successful
    End
    
  End
  
  Describe 'Mock brew functionality'
    It 'can use brew-full for real brew access'
      # This test verifies the mock setup allows access to real brew if needed
      if command -v brew-full &> /dev/null; then
        When call brew-full --version
        The status should be success
        The output should include "Homebrew"
      else
        Skip "brew-full not available in this environment"
      fi
    End
  End
  
  Describe 'Idempotency'
    It 'installs packages on first run'
      create_brewfile "apps" 'brew "tree"\nbrew "jq"'
      
      rm -f /tmp/brew-calls.log
      
      When call "$DODOT" install
      The status should be success
      
      # Verify brewfile was processed
      The result of function verify_brewfile_deployed "apps" should be successful
    End
    
    It 'skips installation on second run with same Brewfile'
      create_brewfile "apps" 'brew "ripgrep"'
      
      # First run
      When call "$DODOT" install
      The status should be success
      
      # Clear log for second run
      rm -f /tmp/brew-calls.log
      
      # Second run - should skip
      When call "$DODOT" install
      The status should be success
      The output should include "Skipping Brewfile (already installed)"
      
      # Verify idempotent behavior using enhanced verification
      The result of function verify_brewfile_deployed "apps" "idempotent" should be successful
    End
    
    It 'reinstalls when Brewfile changes'
      create_brewfile "apps" 'brew "fd"'
      
      # First run
      When call "$DODOT" install
      The status should be success
      
      # Modify Brewfile
      create_brewfile "apps" 'brew "fd"\nbrew "bat"  # Added new tool'
      
      # Clear log
      rm -f /tmp/brew-calls.log
      
      # Second run - should reinstall due to checksum change
      When call "$DODOT" install
      The status should be success
      
      # Verify brewfile was processed again
      The result of function verify_brewfile_deployed "apps" should be successful
    End
  End
  
  Describe 'Error handling'
    It 'handles missing Brewfile'
      Pending "Would need file existence check in action generation"
      mkdir -p "$TEST_DOTFILES_ROOT/empty-pack"
      
      # Create pack.dodot.toml without creating Brewfile
      cat > "$TEST_DOTFILES_ROOT/empty-pack/pack.dodot.toml" << 'EOF'
[[brewfile]]
trigger = { directory = ".", recursive = false }
file_name = "Brewfile"
EOF
      
      When call "$DODOT" install
      The status should be failure
      The error should include "Brewfile not found"
    End
    
    It 'handles invalid Brewfile syntax'
      # Note: Our mock brew doesn't validate syntax, but real brew would
      create_brewfile "bad-syntax" '
# Invalid syntax
brewww "typo"
brew
'
      
      When call "$DODOT" install
      # With mock brew this succeeds, but logs the invalid lines
      The status should be success
      The file "/tmp/brew-calls.log" should include "brewww \"typo\""
    End
    
    It 'handles brew formula that does not exist'
      create_brewfile "bad-formula" 'brew "nonexistent-formula"'
      
      When call "$DODOT" install
      # Our mock brew simulates failure for this specific formula
      The status should be failure
      The error should include "No available formula"
    End
    
    It 'handles brew command failures'
      # Create a Brewfile that will cause mock brew to fail
      create_brewfile "failing" ''
      # Make Brewfile unreadable to trigger error
      chmod 000 "$TEST_DOTFILES_ROOT/failing/Brewfile"
      
      When call "$DODOT" install
      The status should be failure
      
      # Restore permissions for cleanup
      chmod 644 "$TEST_DOTFILES_ROOT/failing/Brewfile"
    End
  End
  
  Describe 'Brewfile formats'
    It 'handles basic brew formula lines'
      create_brewfile "formats" '
brew "wget"
brew "curl"
brew "httpie"
'
      
      rm -f /tmp/brew-calls.log
      
      When call "$DODOT" install
      The status should be success
      
      # Verify brewfile was processed
      The result of function verify_brewfile_deployed "formats" should be successful
    End
    
    It 'handles tap directives'
      create_brewfile "taps" '
tap "homebrew/cask-fonts"
brew "font-hack-nerd-font"
'
      
      rm -f /tmp/brew-calls.log
      
      When call "$DODOT" install
      The status should be success
      
      # Verify brewfile was processed correctly
      The result of function verify_brewfile_deployed "taps" should be successful
    End
    
    It 'handles cask installations'
      create_brewfile "casks" '
cask "visual-studio-code"
cask "docker"
'
      
      rm -f /tmp/brew-calls.log
      
      When call "$DODOT" install
      The status should be success
      
      # Verify brewfile was processed correctly
      The result of function verify_brewfile_deployed "casks" should be successful
    End
  End
End