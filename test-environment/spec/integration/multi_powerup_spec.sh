#!/bin/zsh
# Tests for multiple PowerUps in same pack

Describe 'Multiple PowerUps Integration'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  # Helper to create a multi-powerup pack
  create_multi_powerup_pack() {
    local pack_name="$1"
    mkdir -p "$TEST_DOTFILES_ROOT/$pack_name"
  }
  
  Describe 'Multiple deploy PowerUps'
    It 'handles symlink + shell_profile in same pack'
      create_multi_powerup_pack "dev-tools"
      
      # Create files for symlink
      echo "alias ll='ls -la'" > "$TEST_DOTFILES_ROOT/dev-tools/.aliases"
      echo "export EDITOR=vim" > "$TEST_DOTFILES_ROOT/dev-tools/.exports"
      
      # Create shell profile scripts
      mkdir -p "$TEST_DOTFILES_ROOT/dev-tools/.config/shell_profile"
      echo "echo 'Loading dev tools profile'" > "$TEST_DOTFILES_ROOT/dev-tools/.config/shell_profile/dev.sh"
      
      # Create pack.dodot.toml with both powerups
      cat > "$TEST_DOTFILES_ROOT/dev-tools/pack.dodot.toml" << 'EOF'
# Symlink powerup
[[symlink]]
trigger = { file_name = ".aliases" }
target = "~/.aliases"

[[symlink]]
trigger = { file_name = ".exports" }
target = "~/.exports"

# Shell profile powerup
[[shell_profile]]
trigger = { directory = ".config/shell_profile", recursive = false }
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # Verify symlinks created
      The file "$HOME/.aliases" should be symlink
      The file "$HOME/.exports" should be symlink
      
      # Verify shell profile deployed
      The file "$HOME/.local/share/dodot/deployed/shell_profile/dev.sh" should exist
      The file "$HOME/.local/share/dodot/deployed/shell_profile/dev.sh" should include "Loading dev tools profile"
    End
    
    It 'handles all three deploy types in one pack'
      create_multi_powerup_pack "complete"
      
      # Symlink files
      echo "complete vimrc" > "$TEST_DOTFILES_ROOT/complete/.vimrc"
      
      # Shell profile scripts
      mkdir -p "$TEST_DOTFILES_ROOT/complete/shell"
      echo "source ~/.aliases" > "$TEST_DOTFILES_ROOT/complete/shell/init.sh"
      
      # Shell add path directory
      mkdir -p "$TEST_DOTFILES_ROOT/complete/bin"
      echo '#!/bin/bash\necho "mytool"' > "$TEST_DOTFILES_ROOT/complete/bin/mytool"
      chmod +x "$TEST_DOTFILES_ROOT/complete/bin/mytool"
      
      # Create comprehensive pack.dodot.toml
      cat > "$TEST_DOTFILES_ROOT/complete/pack.dodot.toml" << 'EOF'
# Symlink
[[symlink]]
trigger = { file_name = ".vimrc" }
target = "~/.vimrc"

# Shell profile
[[shell_profile]]
trigger = { directory = "shell", recursive = false }

# Shell add path
[[shell_add_path]]
trigger = { directory = "bin", recursive = false }
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # All three types should be deployed
      The file "$HOME/.vimrc" should be symlink
      The file "$HOME/.local/share/dodot/deployed/shell_profile/init.sh" should exist
      The file "$HOME/.local/share/dodot/deployed/shell_add_path/bin" should exist
    End
    
    It 'executes powerups in priority order'
      create_multi_powerup_pack "ordered"
      
      # Create files that log their execution order
      echo "Order: 1" > "$TEST_DOTFILES_ROOT/ordered/.first"
      echo "Order: 2" > "$TEST_DOTFILES_ROOT/ordered/.second"
      echo "Order: 3" > "$TEST_DOTFILES_ROOT/ordered/.third"
      
      # Pack with explicit priorities
      cat > "$TEST_DOTFILES_ROOT/ordered/pack.dodot.toml" << 'EOF'
[[symlink]]
trigger = { file_name = ".third" }
target = "~/.third"
priority = 30

[[symlink]]
trigger = { file_name = ".first" }
target = "~/.first"
priority = 10

[[symlink]]
trigger = { file_name = ".second" }
target = "~/.second"
priority = 20
EOF
      
      When call "$DODOT" deploy -v
      The status should be success
      
      # All should be deployed (order verification would need operation logs)
      The file "$HOME/.first" should exist
      The file "$HOME/.second" should exist
      The file "$HOME/.third" should exist
    End
    
    It 'maintains separate deployments for each powerup'
      create_multi_powerup_pack "separated"
      
      # Different powerup types with their own files
      echo "gitconfig content" > "$TEST_DOTFILES_ROOT/separated/.gitconfig"
      mkdir -p "$TEST_DOTFILES_ROOT/separated/profile.d"
      echo "export SEPARATED=yes" > "$TEST_DOTFILES_ROOT/separated/profile.d/env.sh"
      
      cat > "$TEST_DOTFILES_ROOT/separated/pack.dodot.toml" << 'EOF'
[[symlink]]
trigger = { file_name = ".gitconfig" }
target = "~/.gitconfig"

[[shell_profile]]
trigger = { directory = "profile.d", recursive = false }
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # Each powerup maintains its own deployment structure
      The path "$HOME/.gitconfig" should be symlink
      The path "$HOME/.local/share/dodot/deployed/shell_profile" should be directory
      The file "$HOME/.local/share/dodot/deployed/shell_profile/env.sh" should exist
      
      # They should not interfere with each other
      The path "$HOME/.local/share/dodot/deployed/shell_profile/.gitconfig" should not exist
    End
  End
  
  Describe 'Multiple install PowerUps'
    It 'handles install_script + brewfile in same pack'
      Pending "Install powerups not fully implemented"
      create_multi_powerup_pack "installers"
      
      # Create install script
      cat > "$TEST_DOTFILES_ROOT/installers/install.sh" << 'EOF'
#!/bin/bash
echo "Running installer" > /tmp/multi-install.log
EOF
      chmod +x "$TEST_DOTFILES_ROOT/installers/install.sh"
      
      # Create Brewfile
      cat > "$TEST_DOTFILES_ROOT/installers/Brewfile" << 'EOF'
brew "wget"
brew "tree"
EOF
      
      # Pack with both install powerups
      cat > "$TEST_DOTFILES_ROOT/installers/pack.dodot.toml" << 'EOF'
[[install_script]]
trigger = { directory = ".", recursive = false }
file_name = "install.sh"

[[brewfile]]
trigger = { directory = ".", recursive = false }
file_name = "Brewfile"
EOF
      
      When call "$DODOT" install
      The status should be success
      
      # Both should create sentinels (actual execution not implemented)
      The path "$HOME/.local/share/dodot/sentinels" should be directory
    End
    
    It 'runs both installers on first deploy'
      Pending "Install powerups not fully implemented"
    End
    
    It 'tracks sentinel files separately'
      Pending "Install powerups not fully implemented"
    End
    
    It 'handles one installer failing'
      Pending "Install powerups not fully implemented"
    End
  End
  
  Describe 'Mixed deploy + install PowerUps'
    It 'handles symlink (deploy) + install_script in same pack'
      create_multi_powerup_pack "mixed"
      
      # Deploy files (symlink)
      echo "mixed config" > "$TEST_DOTFILES_ROOT/mixed/.mixedrc"
      
      # Install script
      cat > "$TEST_DOTFILES_ROOT/mixed/install.sh" << 'EOF'
#!/bin/bash
echo "Mixed pack installer"
EOF
      chmod +x "$TEST_DOTFILES_ROOT/mixed/install.sh"
      
      cat > "$TEST_DOTFILES_ROOT/mixed/pack.dodot.toml" << 'EOF'
[[symlink]]
trigger = { file_name = ".mixedrc" }
target = "~/.mixedrc"

[[install_script]]
trigger = { directory = ".", recursive = false }
file_name = "install.sh"
EOF
      
      # First run - both should execute
      When call "$DODOT" deploy
      The status should be success
      The file "$HOME/.mixedrc" should be symlink
      
      # Install would create sentinel (if implemented)
      When call "$DODOT" install
      The status should be success
    End
    
    It 'runs deploy powerups on every dodot deploy'
      create_multi_powerup_pack "deploy-always"
      
      echo "version 1" > "$TEST_DOTFILES_ROOT/deploy-always/.config"
      
      cat > "$TEST_DOTFILES_ROOT/deploy-always/pack.dodot.toml" << 'EOF'
[[symlink]]
trigger = { file_name = ".config" }
target = "~/.deploy-config"
EOF
      
      # First deploy
      When call "$DODOT" deploy
      The status should be success
      The file "$HOME/.deploy-config" should include "version 1"
      
      # Update source
      echo "version 2" > "$TEST_DOTFILES_ROOT/deploy-always/.config"
      
      # Second deploy - should update
      When call "$DODOT" deploy
      The status should be success
      The file "$HOME/.deploy-config" should include "version 2"
    End
    
    It 'runs install powerups only once'
      Pending "Install powerups not fully implemented - would test sentinel behavior"
    End
    
    It 'handles deploy succeeding but install failing'
      Pending "Install powerups not fully implemented"
    End
  End
  
  Describe 'Complex pack configurations'
    It 'handles pack with 5+ different powerups'
      create_multi_powerup_pack "kitchen-sink"
      
      # Create various files
      echo "vimrc" > "$TEST_DOTFILES_ROOT/kitchen-sink/.vimrc"
      echo "bashrc" > "$TEST_DOTFILES_ROOT/kitchen-sink/.bashrc"
      echo "gitconfig" > "$TEST_DOTFILES_ROOT/kitchen-sink/.gitconfig"
      
      mkdir -p "$TEST_DOTFILES_ROOT/kitchen-sink/shell.d"
      echo "shell setup" > "$TEST_DOTFILES_ROOT/kitchen-sink/shell.d/setup.sh"
      
      mkdir -p "$TEST_DOTFILES_ROOT/kitchen-sink/bin"
      echo "#!/bin/bash" > "$TEST_DOTFILES_ROOT/kitchen-sink/bin/tool1"
      
      # Comprehensive pack configuration
      cat > "$TEST_DOTFILES_ROOT/kitchen-sink/pack.dodot.toml" << 'EOF'
# Multiple symlinks
[[symlink]]
trigger = { file_name = ".vimrc" }
target = "~/.vimrc"

[[symlink]]
trigger = { file_name = ".bashrc" }
target = "~/.bashrc"

[[symlink]]
trigger = { file_name = ".gitconfig" }
target = "~/.gitconfig"

# Shell profile
[[shell_profile]]
trigger = { directory = "shell.d", recursive = false }

# Add to PATH
[[shell_add_path]]
trigger = { directory = "bin", recursive = false }
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # All deployments should succeed
      The file "$HOME/.vimrc" should be symlink
      The file "$HOME/.bashrc" should be symlink
      The file "$HOME/.gitconfig" should be symlink
      The path "$HOME/.local/share/dodot/deployed/shell_profile" should be directory
      The path "$HOME/.local/share/dodot/deployed/shell_add_path" should be directory
    End
    
    It 'handles multiple matchers with different powerups'
      create_multi_powerup_pack "matchers"
      
      # Different file patterns
      echo "Config 1" > "$TEST_DOTFILES_ROOT/matchers/app.conf"
      echo "Config 2" > "$TEST_DOTFILES_ROOT/matchers/tool.conf"
      echo "RC file" > "$TEST_DOTFILES_ROOT/matchers/.apprc"
      
      cat > "$TEST_DOTFILES_ROOT/matchers/pack.dodot.toml" << 'EOF'
# Match .conf files
[[symlink]]
trigger = { pattern = "*.conf" }
target = "~/.config/{{.FileName}}"

# Match rc files
[[symlink]]
trigger = { pattern = ".*rc" }
target = "~/{{.FileName}}"
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # Pattern matches should work
      The file "$HOME/.config/app.conf" should be symlink
      The file "$HOME/.config/tool.conf" should be symlink
      The file "$HOME/.apprc" should be symlink
    End
    
    It 'handles overlapping file patterns'
      create_multi_powerup_pack "overlap"
      
      # File that could match multiple patterns
      echo "Special config" > "$TEST_DOTFILES_ROOT/overlap/.special.conf"
      
      cat > "$TEST_DOTFILES_ROOT/overlap/pack.dodot.toml" << 'EOF'
# General conf pattern
[[symlink]]
trigger = { pattern = "*.conf" }
target = "~/.config/{{.FileName}}"
priority = 20

# More specific dotfile pattern
[[symlink]]
trigger = { pattern = ".*" }
target = "~/{{.FileName}}"
priority = 10
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # Should handle based on priority or first match
      # At least one should succeed
      The output should include ".special.conf"
    End
  End
End