#!/bin/zsh
# Test composite verification function

Describe 'Composite Verification'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  It 'verifies multiple powerups with verify_pack_deployed'
    # Deploy bash pack which has both symlink and shell_profile
    "$DODOT" deploy bash >/dev/null 2>&1
    
    When call verify_pack_deployed "bash" "symlink:.bashrc" "shell_profile:aliases.sh"
    The status should be success
  End
  
  It 'verifies single powerup with verify_pack_deployed'
    # Deploy vim pack which only has symlink
    "$DODOT" deploy vim >/dev/null 2>&1
    
    When call verify_pack_deployed "vim" "symlink:.vimrc"
    The status should be success
  End
  
  It 'fails when any powerup verification fails'
    # Deploy vim but check for non-existent file
    "$DODOT" deploy vim >/dev/null 2>&1
    
    When call verify_pack_deployed "vim" "symlink:.vimrc" "symlink:.nonexistent"
    The status should be failure
  End
  
  It 'verifies complex pack with multiple powerup types'
    # Create a test pack with multiple powerups
    mkdir -p "$DOTFILES_ROOT/testpack/bin"
    echo "#!/bin/sh" > "$DOTFILES_ROOT/testpack/bin/test-cmd"
    chmod +x "$DOTFILES_ROOT/testpack/bin/test-cmd"
    echo "alias test='echo test'" > "$DOTFILES_ROOT/testpack/aliases.sh"
    echo "TESTPACK_PROFILE_LOADED=1" >> "$DOTFILES_ROOT/testpack/aliases.sh"
    echo "test config" > "$DOTFILES_ROOT/testpack/.testrc"
    
    "$DODOT" deploy testpack >/dev/null 2>&1
    
    When call verify_pack_deployed "testpack" \
      "symlink:.testrc" \
      "shell_profile:aliases.sh" \
      "shell_add_path:bin"
    The status should be success
  End
End