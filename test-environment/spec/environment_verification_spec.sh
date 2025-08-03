#!/bin/zsh
# Test to verify our environment verification catches contamination

Describe 'Environment verification system'
  Describe 'clean environment verification'
    BeforeEach 'reset_test_environment'
    AfterEach 'cleanup_test_environment'
    
    It 'passes when environment is clean'
      When call verify_clean_environment
      The status should be success
      The error should include "Verification Complete: 0 errors found"
    End
  End

  Describe 'contamination detection'
    BeforeEach 'reset_test_environment'
    AfterEach 'cleanup_test_environment'
    
    It 'detects dodot data directory contamination'
      mkdir -p "$HOME/.local/share/dodot/state"
      touch "$HOME/.local/share/dodot/state/test.state"
      
      When call verify_clean_environment
      The status should be failure
      The error should include "Dodot directory exists when it shouldn't"
      The error should include ".local/share/dodot"
    End
    
    It 'detects dodot config directory contamination'
      mkdir -p "$HOME/.config/dodot"
      echo "test = true" > "$HOME/.config/dodot/config.toml"
      
      When call verify_clean_environment
      The status should be failure
      The error should include ".config/dodot"
    End
    
    It 'detects dodot cache directory contamination'
      mkdir -p "$HOME/.cache/dodot"
      
      When call verify_clean_environment
      The status should be failure
      The error should include ".cache/dodot"
    End
    
    It 'detects unexpected symlinks in HOME'
      ln -s /tmp/fake "$HOME/.fake-link"
      
      When call verify_clean_environment
      The status should be failure
      The error should include "Unexpected symlinks in HOME"
    End
    
    It 'detects incorrect HOME environment variable'
      export HOME="/tmp/wrong-home"
      
      When call verify_clean_environment
      The status should be failure
      The error should include "HOME is not set to TEST_HOME"
    End
    
    It 'detects incorrect DOTFILES_ROOT'
      export DOTFILES_ROOT="/tmp/wrong-dotfiles"
      
      When call verify_clean_environment
      The status should be failure
      The error should include "DOTFILES_ROOT is not set to TEST_DOTFILES_ROOT"
    End
    
    It 'detects dodot environment variables'
      export DODOT_DEBUG="true"
      export DODOT_DATA_DIR="/custom/data"
      
      When call verify_clean_environment
      The status should be failure
      The error should include "DODOT_DEBUG is set when it shouldn't be"
      The error should include "DODOT_DATA_DIR is set when it shouldn't be"
    End
    
    It 'detects missing template files'
      rm -f "$HOME/.zshrc"
      
      When call verify_clean_environment
      The status should be failure
      The error should include "Template .zshrc not found"
    End
    
    It 'detects multiple contaminations'
      # Create multiple issues
      mkdir -p "$HOME/.local/share/dodot"
      ln -s /tmp/fake "$HOME/.vimrc"
      export DODOT_DEBUG="true"
      
      When call verify_clean_environment
      The status should be failure
      The error should include "3 errors found"
    End
  End

  Describe 'reset recovery from contamination'
    It 'can recover from severe contamination'
      # Contaminate environment
      mkdir -p "$HOME/.local/share/dodot/state"
      mkdir -p "$HOME/.config/dodot"
      mkdir -p "$HOME/.cache/dodot"
      ln -s /tmp/fake "$HOME/.vimrc"
      ln -s /tmp/fake2 "$HOME/.gitconfig"
      export DODOT_DEBUG="true"
      export DODOT_DATA_DIR="/custom"
      export HOME="/wrong/home"
      
      # Reset should fix everything
      When call reset_test_environment
      The status should be success
      
      # Verify environment is clean after reset
      When call verify_clean_environment
      The status should be success
    End
  End

  Describe 'environment isolation between tests'
    It 'first test creates contamination'
      # This test intentionally contaminates
      mkdir -p "$HOME/.local/share/dodot/backups"
      touch "$HOME/.local/share/dodot/backups/test.bak"
      ln -s /tmp/test "$HOME/.test-symlink"
      
      When call test -d "$HOME/.local/share/dodot/backups"
      The status should be success
    End
    
    It 'second test starts clean despite previous contamination'
      # BeforeEach should have cleaned everything
      When call verify_clean_environment
      The status should be success
      
      # Verify specific contamination from previous test is gone
      When call test -d "$HOME/.local/share/dodot"
      The status should be failure
      
      When call test -L "$HOME/.test-symlink"
      The status should be failure
    End
  End
End