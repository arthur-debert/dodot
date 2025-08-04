#!/bin/zsh
# Tests for Symlink PowerUp functionality

Describe 'Symlink PowerUp'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  Describe 'Basic symlink creation'
    It 'deploys vim pack successfully'
      When call "$DODOT" deploy vim
      The status should be success
      The error should not include "ERROR"
    End
    
    It 'creates .vimrc symlink after deployment'
      # Run deploy first
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call verify_symlink_deployed "vim" ".vimrc"
      The status should be success
    End
    
    It 'creates symlink pointing to deployed directory'
      # Run deploy first
      "$DODOT" deploy vim >/dev/null 2>&1
      
      # This test is now redundant as verify_symlink_deployed checks this
      When call verify_symlink_deployed "vim" ".vimrc"
      The status should be success
    End
    
    It 'allows reading file content through symlink'
      # Create test content
      echo "\" Test vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      # Run deploy
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call cat "$HOME/.vimrc"
      The output should equal "\" Test vim config"
    End
    
    It 'creates directory symlinks'
      # Create a .vim directory with some content
      mkdir -p "$DOTFILES_ROOT/vim/.vim/colors"
      echo "colorscheme test" > "$DOTFILES_ROOT/vim/.vim/colors/test.vim"
      
      # Run deploy
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call verify_symlink_deployed "vim" ".vim"
      The status should be success
    End
    
    It 'allows accessing files through directory symlink'
      # Create a .vim directory with some content
      mkdir -p "$DOTFILES_ROOT/vim/.vim/colors"
      echo "colorscheme test" > "$DOTFILES_ROOT/vim/.vim/colors/test.vim"
      
      # Run deploy
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call cat "$HOME/.vim/colors/test.vim"
      The output should equal "colorscheme test"
    End
    
    It 'handles nested directory structure'
      # Create a nested config structure
      mkdir -p "$DOTFILES_ROOT/vim/.config/nvim"
      echo "set number" > "$DOTFILES_ROOT/vim/.config/nvim/init.vim"
      
      # Update the pack config to handle .config/nvim
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" },
    { type = "Directory", pattern = ".vim" },
    { type = "Directory", pattern = ".config/nvim" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      # Run deploy
      "$DODOT" deploy vim >/dev/null 2>&1
      
      # Verify parent directory was created
      When call test -d "$HOME/.config"
      The status should be success
    End
    
    It 'creates nested directory symlink'
      # Setup nested structure (from previous test)
      mkdir -p "$DOTFILES_ROOT/vim/.config/nvim"
      echo "set number" > "$DOTFILES_ROOT/vim/.config/nvim/init.vim"
      
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "Directory", pattern = ".config/nvim" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call test -L "$HOME/.config/nvim"
      The status should be success
    End
    
    It 'reads content through nested symlink'
      # Setup nested structure
      mkdir -p "$DOTFILES_ROOT/vim/.config/nvim"
      echo "set number" > "$DOTFILES_ROOT/vim/.config/nvim/init.vim"
      
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "Directory", pattern = ".config/nvim" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call cat "$HOME/.config/nvim/init.vim"
      The output should equal "set number"
    End
  End
  
  Describe 'Error handling'
    It 'fails when source file does not exist'
      # Create a config that references non-existent file
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" },
    { type = "FileName", pattern = ".nonexistent" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      When call "$DODOT" deploy vim
      The status should be failure
      The error should include "ERROR"
    End
    
    It 'fails with existing file at target'
      # Create a regular file where symlink would go
      echo "existing content" > "$HOME/.vimrc"
      
      # Create the source file
      echo "\" New vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      When call "$DODOT" deploy vim
      The status should be failure
      The error should include "ERROR"
    End
    
    It 'preserves existing file on failure'
      # Create existing file
      echo "existing content" > "$HOME/.vimrc"
      
      # Create source
      echo "\" New vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      # Try deploy (will fail)
      "$DODOT" deploy vim 2>/dev/null || true
      
      # Verify original file wasn't changed
      When call cat "$HOME/.vimrc"
      The output should equal "existing content"
    End
    
    It 'fails when target is a directory'
      # Create a directory where symlink would go
      mkdir -p "$HOME/.vimrc/subdir"
      
      # Create the source file
      echo "\" vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      When call "$DODOT" deploy vim
      The status should be failure
      The error should include "ERROR"
    End
    
    It 'preserves directory on failure'
      # Create directory
      mkdir -p "$HOME/.vimrc/subdir"
      
      # Create source
      echo "\" vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      # Try deploy (will fail)
      "$DODOT" deploy vim 2>/dev/null || true
      
      # Verify directory still exists
      When call test -d "$HOME/.vimrc"
      The status should be success
    End
    
    It 'handles permission errors'
      # Create source file
      echo "\" vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      # Make home directory read-only
      chmod 555 "$HOME"
      
      When call "$DODOT" deploy vim
      The status should be failure
      The error should include "ERROR"
    End
    
    It 'restores permissions after test'
      # This test ensures cleanup works after permission test
      chmod 755 "$HOME"
      
      When call test -w "$HOME"
      The status should be success
    End
  End
  
  Describe 'Idempotency'
    It 'first deploy succeeds'
      echo "\" vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      When call "$DODOT" deploy vim
      The status should be success
    End
    
    It 'second deploy also succeeds'
      echo "\" vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      # First deploy
      "$DODOT" deploy vim >/dev/null 2>&1
      
      # Second deploy should also succeed
      When call "$DODOT" deploy vim
      The status should be success
    End
    
    It 'symlink works after multiple deploys'
      echo "\" vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      # Deploy twice
      "$DODOT" deploy vim >/dev/null 2>&1
      "$DODOT" deploy vim >/dev/null 2>&1
      
      # Verify symlink still works
      When call verify_symlink_deployed "vim" ".vimrc"
      The status should be success
    End
    
    It 'verifies idempotent deployment'
      echo "\" vim config" > "$DOTFILES_ROOT/vim/.vimrc"
      
      When call verify_idempotent_deploy "vim"
      The status should be success
    End
  End
  
  Describe 'Multiple files'
    It 'creates all symlinks from pack'
      # Create multiple vim-related files
      echo "\" vimrc" > "$DOTFILES_ROOT/vim/.vimrc"
      mkdir -p "$DOTFILES_ROOT/vim/.vim/plugin"
      echo "\" plugin" > "$DOTFILES_ROOT/vim/.vim/plugin/test.vim"
      echo "\" gvimrc" > "$DOTFILES_ROOT/vim/.gvimrc"
      
      # Update config to include .gvimrc
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" },
    { type = "FileName", pattern = ".gvimrc" },
    { type = "Directory", pattern = ".vim" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      When call "$DODOT" deploy vim
      The status should be success
    End
    
    It 'verifies .vimrc symlink exists'
      # Setup from previous test
      echo "\" vimrc" > "$DOTFILES_ROOT/vim/.vimrc"
      echo "\" gvimrc" > "$DOTFILES_ROOT/vim/.gvimrc"
      mkdir -p "$DOTFILES_ROOT/vim/.vim/plugin"
      echo "\" plugin" > "$DOTFILES_ROOT/vim/.vim/plugin/test.vim"
      
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" },
    { type = "FileName", pattern = ".gvimrc" },
    { type = "Directory", pattern = ".vim" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call test -L "$HOME/.vimrc"
      The status should be success
    End
    
    It 'verifies .gvimrc symlink exists'
      # Setup files
      echo "\" vimrc" > "$DOTFILES_ROOT/vim/.vimrc"
      echo "\" gvimrc" > "$DOTFILES_ROOT/vim/.gvimrc"
      
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" },
    { type = "FileName", pattern = ".gvimrc" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call test -L "$HOME/.gvimrc"
      The status should be success
    End
    
    It 'verifies .vim directory symlink exists'
      # Setup files
      mkdir -p "$DOTFILES_ROOT/vim/.vim/plugin"
      echo "\" plugin" > "$DOTFILES_ROOT/vim/.vim/plugin/test.vim"
      
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "Directory", pattern = ".vim" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call test -L "$HOME/.vim"
      The status should be success
    End
    
    It 'accesses content through directory symlink'
      # Setup files
      mkdir -p "$DOTFILES_ROOT/vim/.vim/plugin"
      echo "\" plugin" > "$DOTFILES_ROOT/vim/.vim/plugin/test.vim"
      
      cat > "$DOTFILES_ROOT/vim/pack.dodot.toml" << 'EOF'
name = "vim"

[[matchers]]
triggers = [
    { type = "Directory", pattern = ".vim" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      "$DODOT" deploy vim >/dev/null 2>&1
      
      When call cat "$HOME/.vim/plugin/test.vim"
      The output should equal "\" plugin"
    End
  End
End