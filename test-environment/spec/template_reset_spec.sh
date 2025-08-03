#!/bin/zsh
# Test to verify template reset mechanism works correctly

Describe 'Template reset mechanism'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'

  It 'creates fresh home directory from template with .zshrc'
    When call test -f "$HOME/.zshrc"
    The status should be success
  End
  
  It 'preserves .zshrc content from template'
    When call cat "$HOME/.zshrc"
    The output should include "Loading test user .zshrc"
  End

  It 'creates fresh dotfiles root from template'
    When call test -d "$DOTFILES_ROOT/vim"
    The status should be success
  End
  
  It 'includes pack.dodot.toml in vim directory'
    When call test -f "$DOTFILES_ROOT/vim/pack.dodot.toml"
    The status should be success
  End

  It 'creates XDG config directory'
    When call test -d "$HOME/.config"
    The status should be success
  End
  
  It 'creates XDG local share directory'
    When call test -d "$HOME/.local/share"
    The status should be success
  End
  
  It 'creates XDG cache directory'
    When call test -d "$HOME/.cache"
    The status should be success
  End

  It 'preserves existing config files from template'
    When call test -f "$HOME/.config/existing-app/config.toml"
    The status should be success
  End
  
  It 'preserves config file content'
    When call grep -q "theme = \"light\"" "$HOME/.config/existing-app/config.toml"
    The status should be success
  End

  It 'starts with no dodot share directory'
    When call test -d "$HOME/.local/share/dodot"
    The status should be failure
  End
  
  It 'starts with no dodot cache directory'
    When call test -d "$HOME/.cache/dodot"
    The status should be failure
  End

  Describe 'test isolation'
    It 'can create a file in test environment'
      When call touch "$HOME/test-file"
      The status should be success
    End
    
    It 'file is removed after reset'
      # Reset happens in BeforeEach, so file from previous test should be gone
      When call test -f "$HOME/test-file"
      The status should be failure
    End
  End
End