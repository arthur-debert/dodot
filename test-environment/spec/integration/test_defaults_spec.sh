#!/bin/zsh
# Test if default matchers work without pack.dodot.toml

Describe 'Default Matchers Test'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  It 'symlinks .bashrc without pack.dodot.toml'
    mkdir -p "$TEST_DOTFILES_ROOT/defaults"
    echo "test bashrc" > "$TEST_DOTFILES_ROOT/defaults/.bashrc"
    
    # NO pack.dodot.toml!
    
    When call "$DODOT" deploy defaults
    The status should be success
    
    # Should create symlink using default catch-all matcher
    The result of function verify_symlink_deployed "defaults" ".bashrc" should be successful
  End
  
  It 'handles aliases.sh for shell_profile without pack.dodot.toml'
    mkdir -p "$TEST_DOTFILES_ROOT/profile"
    echo "alias x=exit" > "$TEST_DOTFILES_ROOT/profile/aliases.sh"
    echo "PROFILE_PROFILE_LOADED=1" >> "$TEST_DOTFILES_ROOT/profile/aliases.sh"
    
    # NO pack.dodot.toml!
    
    When call "$DODOT" deploy profile
    The status should be success
    
    # Should deploy to shell_profile using default matcher
    The result of function verify_shell_profile_deployed "profile" "aliases.sh" should be successful
  End
End