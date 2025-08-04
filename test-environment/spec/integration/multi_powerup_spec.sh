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
      create_multi_powerup_pack "multitest"
      
      # Create files matching working test patterns
      echo "#!/bin/bash" > "$TEST_DOTFILES_ROOT/multitest/.bashrc"
      echo "alias g='git'" > "$TEST_DOTFILES_ROOT/multitest/aliases.sh"
      
      # Create pack.dodot.toml matching working format exactly
      cat > "$TEST_DOTFILES_ROOT/multitest/pack.dodot.toml" << 'EOF'
name = "multitest"

# Deploy .bashrc as symlink
[[matchers]]
triggers = [
    { type = "FileName", pattern = ".bashrc" }
]
actions = [
    { type = "symlink" }
]

# Source aliases.sh for shell profile
[[matchers]]
triggers = [
    { type = "FileName", pattern = "aliases.sh" }
]
actions = [
    { type = "shell_profile" }
]
EOF
      
      When call "$DODOT" deploy multitest
      The status should be success
      
      # Verify both powerups using composite function
      The result of function verify_pack_deployed "multitest" "symlink:.bashrc" "shell_profile:aliases.sh" should be successful
    End
    
    It 'handles all three deploy types in one pack'
      create_multi_powerup_pack "complete"
      
      # Symlink files
      echo "complete vimrc" > "$TEST_DOTFILES_ROOT/complete/.vimrc"
      
      # Shell profile scripts
      echo "source ~/.aliases" > "$TEST_DOTFILES_ROOT/complete/aliases.sh"
      
      # Shell add path directory
      mkdir -p "$TEST_DOTFILES_ROOT/complete/bin"
      echo '#!/bin/bash\necho "mytool"' > "$TEST_DOTFILES_ROOT/complete/bin/mytool"
      chmod +x "$TEST_DOTFILES_ROOT/complete/bin/mytool"
      
      # Create pack.dodot.toml
      cat > "$TEST_DOTFILES_ROOT/complete/pack.dodot.toml" << 'EOF'
name = "complete"

# Symlink
[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" }
]
actions = [
    { type = "symlink" }
]

# Shell profile
[[matchers]]
triggers = [
    { type = "FileName", pattern = "aliases.sh" }
]
actions = [
    { type = "shell_profile" }
]

# Shell add path
[[matchers]]
triggers = [
    { type = "Directory", pattern = "bin" }
]
actions = [
    { type = "shell_add_path" }
]
EOF
      
      When call "$DODOT" deploy complete
      The status should be success
      
      # Verify all three powerups using composite function
      The result of function verify_pack_deployed "complete" "symlink:.vimrc" "shell_profile:aliases.sh" "shell_add_path:bin" should be successful
    End
    
    It 'executes powerups in priority order'
      create_multi_powerup_pack "ordered"
      
      # Create different file types
      echo "bashrc content" > "$TEST_DOTFILES_ROOT/ordered/.bashrc"
      echo "aliases content" > "$TEST_DOTFILES_ROOT/ordered/aliases.sh"
      mkdir -p "$TEST_DOTFILES_ROOT/ordered/bin"
      echo '#!/bin/bash' > "$TEST_DOTFILES_ROOT/ordered/bin/tool"
      chmod +x "$TEST_DOTFILES_ROOT/ordered/bin/tool"
      
      # Pack config with explicit priorities
      cat > "$TEST_DOTFILES_ROOT/ordered/pack.dodot.toml" << 'EOF'
name = "ordered"

# High priority symlink
[[matchers]]
triggers = [
    { type = "FileName", pattern = ".bashrc" }
]
actions = [
    { type = "symlink" }
]
priority = 100

# Medium priority shell profile
[[matchers]]
triggers = [
    { type = "FileName", pattern = "aliases.sh" }
]
actions = [
    { type = "shell_profile" }
]
priority = 80

# Lower priority shell add path
[[matchers]]
triggers = [
    { type = "Directory", pattern = "bin" }
]
actions = [
    { type = "shell_add_path" }
]
priority = 70
EOF
      
      When call "$DODOT" deploy -v
      The status should be success
      
      # All should be deployed - verify using composite function
      The result of function verify_pack_deployed "ordered" "symlink:.bashrc" "shell_profile:aliases.sh" "shell_add_path:bin" should be successful
    End
    
    It 'maintains separate deployments for each powerup'
      create_multi_powerup_pack "separated"
      
      # Different powerup types with their own files
      echo "gitconfig content" > "$TEST_DOTFILES_ROOT/separated/.gitconfig"
      echo "export SEPARATED=yes" > "$TEST_DOTFILES_ROOT/separated/aliases.sh"
      
      # Pack config
      cat > "$TEST_DOTFILES_ROOT/separated/pack.dodot.toml" << 'EOF'
name = "separated"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".gitconfig" }
]
actions = [
    { type = "symlink" }
]

[[matchers]]
triggers = [
    { type = "FileName", pattern = "aliases.sh" }
]
actions = [
    { type = "shell_profile" }
]
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # Each powerup maintains its own deployment structure
      The result of function verify_pack_deployed "separated" "symlink:.gitconfig" "shell_profile:aliases.sh" should be successful
      
      # They should not interfere with each other (edge case worth checking manually)
      The path "$HOME/.local/share/dodot/deployed/shell_profile/.gitconfig" should not exist
    End
  End
  
  Describe 'Multiple install PowerUps'
    It 'handles install_script + brewfile in same pack'
      create_multi_powerup_pack "installers"
      
      # Create install script
      cat > "$TEST_DOTFILES_ROOT/installers/install.sh" << 'EOF'
#!/bin/bash
echo "Running installer"
echo "Running installer" > /tmp/multi-install.log
EOF
      chmod +x "$TEST_DOTFILES_ROOT/installers/install.sh"
      
      # Create Brewfile
      cat > "$TEST_DOTFILES_ROOT/installers/Brewfile" << 'EOF'
brew "wget"
brew "tree"
EOF
      
      # Pack config
      cat > "$TEST_DOTFILES_ROOT/installers/pack.dodot.toml" << 'EOF'
name = "installers"

[[matchers]]
triggers = [
    { type = "FileName", pattern = "install.sh" }
]
actions = [
    { type = "install_script" }
]

[[matchers]]
triggers = [
    { type = "FileName", pattern = "Brewfile" }
]
actions = [
    { type = "brewfile" }
]
EOF
      
      When call "$DODOT" install
      The status should be success
      
      # Both should be deployed - verification functions check sentinel creation
      The result of function verify_install_script_deployed "installers" "install.sh" "/tmp/multi-install.log" should be successful
      The result of function verify_brewfile_deployed "installers" should be successful
    End
    
    It 'runs both installers on first deploy'
      create_multi_powerup_pack "first-run"
      
      # Create both install powerups
      echo "#!/bin/bash\necho 'First installer'" > "$TEST_DOTFILES_ROOT/first-run/install.sh"
      chmod +x "$TEST_DOTFILES_ROOT/first-run/install.sh"
      echo "brew 'tree'" > "$TEST_DOTFILES_ROOT/first-run/Brewfile"
      
      When call "$DODOT" install
      The status should be success
      
      # Check both ran
      The output should include "First installer"
      The output should include "brew bundle"
    End
    
    It 'tracks sentinel files separately'
      create_multi_powerup_pack "sentinel-test"
      
      # Create both install powerups
      echo "#!/bin/bash\necho 'Sentinel test'" > "$TEST_DOTFILES_ROOT/sentinel-test/install.sh"
      chmod +x "$TEST_DOTFILES_ROOT/sentinel-test/install.sh"
      echo "brew 'wget'" > "$TEST_DOTFILES_ROOT/sentinel-test/Brewfile"
      
      # First run
      "$DODOT" install >/dev/null 2>&1
      
      # Check sentinels exist
      The path "$HOME/.local/share/dodot/sentinels/install/sentinel-test" should be file
      The path "$HOME/.local/share/dodot/sentinels/brewfile/sentinel-test" should be file
    End
    
    It 'handles one installer failing'
      create_multi_powerup_pack "fail-test"
      
      # Create failing install script
      echo "#!/bin/bash\nexit 1" > "$TEST_DOTFILES_ROOT/fail-test/install.sh"
      chmod +x "$TEST_DOTFILES_ROOT/fail-test/install.sh"
      
      # Create working Brewfile
      echo "brew 'jq'" > "$TEST_DOTFILES_ROOT/fail-test/Brewfile"
      
      When call "$DODOT" install
      The status should be failure
      
      # Brewfile should still run even if install script fails
      The output should include "brew bundle"
    End
  End
  
  Describe 'Mixed deploy + install PowerUps'
    It 'handles symlink (deploy) + install_script in same pack'
      create_multi_powerup_pack "mixed"
      
      # Deploy files
      echo "mixed bash config" > "$TEST_DOTFILES_ROOT/mixed/.bashrc"
      
      # Install script
      cat > "$TEST_DOTFILES_ROOT/mixed/install.sh" << 'EOF'
#!/bin/bash
echo "Mixed pack installer"
EOF
      chmod +x "$TEST_DOTFILES_ROOT/mixed/install.sh"
      
      # Pack config
      cat > "$TEST_DOTFILES_ROOT/mixed/pack.dodot.toml" << 'EOF'
name = "mixed"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".bashrc" }
]
actions = [
    { type = "symlink" }
]

[[matchers]]
triggers = [
    { type = "FileName", pattern = "install.sh" }
]
actions = [
    { type = "install_script" }
]
EOF
      
      # First run deploy - should only run deploy powerups
      When call "$DODOT" deploy
      The status should be success
      The file "$HOME/.bashrc" should be symlink
      
      # Then run install - should only run install powerups
      When call "$DODOT" install
      The status should be success
      The path "$HOME/.local/share/dodot/sentinels/install" should be directory
    End
    
    It 'runs deploy powerups on every dodot deploy'
      create_multi_powerup_pack "deploy-always"
      
      echo "version 1" > "$TEST_DOTFILES_ROOT/deploy-always/.vimrc"
      
      # Pack config
      cat > "$TEST_DOTFILES_ROOT/deploy-always/pack.dodot.toml" << 'EOF'
name = "deploy-always"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" }
]
actions = [
    { type = "symlink" }
]
EOF
      
      # First deploy
      When call "$DODOT" deploy
      The status should be success
      The file "$HOME/.vimrc" should include "version 1"
      
      # Update source
      echo "version 2" > "$TEST_DOTFILES_ROOT/deploy-always/.vimrc"
      
      # Second deploy - should update
      When call "$DODOT" deploy
      The status should be success
      The file "$HOME/.vimrc" should include "version 2"
    End
    
    It 'runs install powerups only once'
      create_multi_powerup_pack "run-once"
      
      # Create install script that logs runs
      echo "#!/bin/bash\necho 'Install ran' >> /tmp/install-count.log" > "$TEST_DOTFILES_ROOT/run-once/install.sh"
      chmod +x "$TEST_DOTFILES_ROOT/run-once/install.sh"
      
      # Clear log
      rm -f /tmp/install-count.log
      
      # First run
      "$DODOT" install >/dev/null 2>&1
      
      # Second run
      "$DODOT" install >/dev/null 2>&1
      
      # Check it only ran once
      When call wc -l < /tmp/install-count.log
      The output should equal "1"
    End
    
    It 'handles deploy succeeding but install failing'
      create_multi_powerup_pack "partial-fail"
      
      # Deploy file (should succeed)
      echo "config content" > "$TEST_DOTFILES_ROOT/partial-fail/.vimrc"
      
      # Install script that fails
      echo "#!/bin/bash\necho 'About to fail'\nexit 1" > "$TEST_DOTFILES_ROOT/partial-fail/install.sh"
      chmod +x "$TEST_DOTFILES_ROOT/partial-fail/install.sh"
      
      # Run deploy first (should succeed)
      When call "$DODOT" deploy
      The status should be success
      The file "$HOME/.vimrc" should be symlink
      
      # Then run install (should fail)
      When call "$DODOT" install  
      The status should be failure
      The output should include "About to fail"
      
      # But deploy should still be intact
      The file "$HOME/.vimrc" should be symlink
    End
  End
  
  Describe 'Complex pack configurations'
    It 'handles pack with 5+ different powerups'
      create_multi_powerup_pack "kitchen-sink"
      
      # Symlink powerup files
      echo "vimrc" > "$TEST_DOTFILES_ROOT/kitchen-sink/.vimrc"
      echo "bashrc" > "$TEST_DOTFILES_ROOT/kitchen-sink/.bashrc"
      echo "gitconfig" > "$TEST_DOTFILES_ROOT/kitchen-sink/.gitconfig"
      
      # Shell profile powerup
      echo "shell setup" > "$TEST_DOTFILES_ROOT/kitchen-sink/aliases.sh"
      
      # Shell add path powerup
      mkdir -p "$TEST_DOTFILES_ROOT/kitchen-sink/bin"
      echo "#!/bin/bash" > "$TEST_DOTFILES_ROOT/kitchen-sink/bin/tool1"
      chmod +x "$TEST_DOTFILES_ROOT/kitchen-sink/bin/tool1"
      
      # Install script powerup
      echo "#!/bin/bash\necho 'installing'" > "$TEST_DOTFILES_ROOT/kitchen-sink/install.sh"
      chmod +x "$TEST_DOTFILES_ROOT/kitchen-sink/install.sh"
      
      # Brewfile powerup
      echo "brew 'jq'" > "$TEST_DOTFILES_ROOT/kitchen-sink/Brewfile"
      
      # Comprehensive pack config
      cat > "$TEST_DOTFILES_ROOT/kitchen-sink/pack.dodot.toml" << 'EOF'
name = "kitchen-sink"

# Multiple symlinks
[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" },
    { type = "FileName", pattern = ".bashrc" },
    { type = "FileName", pattern = ".gitconfig" }
]
actions = [
    { type = "symlink" }
]

# Shell profile
[[matchers]]
triggers = [
    { type = "FileName", pattern = "aliases.sh" }
]
actions = [
    { type = "shell_profile" }
]

# Shell add path
[[matchers]]
triggers = [
    { type = "Directory", pattern = "bin" }
]
actions = [
    { type = "shell_add_path" }
]

# Install script
[[matchers]]
triggers = [
    { type = "FileName", pattern = "install.sh" }
]
actions = [
    { type = "install_script" }
]

# Brewfile
[[matchers]]
triggers = [
    { type = "FileName", pattern = "Brewfile" }
]
actions = [
    { type = "brewfile" }
]
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # Deploy powerups should succeed - verify all with composite function
      The result of function verify_symlink_deployed "mixed" ".vimrc" should be successful
      The result of function verify_symlink_deployed "mixed" ".bashrc" should be successful  
      The result of function verify_symlink_deployed "mixed" ".gitconfig" should be successful
      
      # Install powerups need separate install command
      When call "$DODOT" install
      The status should be success
    End
    
    It 'handles multiple file types with different patterns'
      create_multi_powerup_pack "patterns"
      
      # Various file types
      echo "vim config" > "$TEST_DOTFILES_ROOT/patterns/.vimrc"
      echo "bash config" > "$TEST_DOTFILES_ROOT/patterns/.bashrc"
      echo "git config" > "$TEST_DOTFILES_ROOT/patterns/.gitconfig"
      echo "alias x=exit" > "$TEST_DOTFILES_ROOT/patterns/aliases.sh"
      mkdir -p "$TEST_DOTFILES_ROOT/patterns/bin"
      echo "#!/bin/bash\necho hi" > "$TEST_DOTFILES_ROOT/patterns/bin/hello"
      chmod +x "$TEST_DOTFILES_ROOT/patterns/bin/hello"
      
      # Pack config with various patterns
      cat > "$TEST_DOTFILES_ROOT/patterns/pack.dodot.toml" << 'EOF'
name = "patterns"

# Dotfiles pattern
[[matchers]]
triggers = [
    { type = "FileName", pattern = ".*rc" },
    { type = "FileName", pattern = ".gitconfig" }
]
actions = [
    { type = "symlink" }
]

# Shell scripts pattern
[[matchers]]
triggers = [
    { type = "FileName", pattern = "*.sh" }
]
actions = [
    { type = "shell_profile" }
]

# Bin directory
[[matchers]]
triggers = [
    { type = "Directory", pattern = "bin" }
]
actions = [
    { type = "shell_add_path" }
]
EOF
      
      When call "$DODOT" deploy
      The status should be success
      
      # All should be deployed
      # Verify all powerups deployed correctly using composite verification
      The result of function verify_pack_deployed "patterns" "symlink:.vimrc" "symlink:.bashrc" "symlink:.gitconfig" "shell_profile:aliases.sh" "shell_add_path:bin" should be successful
    End
    
    It 'handles priority between different powerups'
      create_multi_powerup_pack "priority"
      
      # Create files that would be handled by different powerups
      mkdir -p "$TEST_DOTFILES_ROOT/priority/bin"
      echo "#!/bin/bash\necho test" > "$TEST_DOTFILES_ROOT/priority/bin/test"
      chmod +x "$TEST_DOTFILES_ROOT/priority/bin/test"
      
      # Create other files for clarity
      echo "vimrc" > "$TEST_DOTFILES_ROOT/priority/.vimrc"
      echo "aliases" > "$TEST_DOTFILES_ROOT/priority/aliases.sh"
      
      # Pack config with different priorities
      cat > "$TEST_DOTFILES_ROOT/priority/pack.dodot.toml" << 'EOF'
name = "priority"

# High priority
[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" }
]
actions = [
    { type = "symlink" }
]
priority = 100

# Medium priority
[[matchers]]
triggers = [
    { type = "Directory", pattern = "bin" }
]
actions = [
    { type = "shell_add_path" }
]
priority = 80

# Lower priority
[[matchers]]
triggers = [
    { type = "FileName", pattern = "*.sh" }
]
actions = [
    { type = "shell_profile" }
]
priority = 70
EOF
      
      When call "$DODOT" deploy -v
      The status should be success
      
      # Should deploy all files
      The output should include "bin"
      # Verify deployments using verification functions
      The result of function verify_pack_deployed "priority" "symlink:.vimrc" "shell_profile:aliases.sh" should be successful
    End
  End
End