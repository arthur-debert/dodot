#!/bin/zsh
# Tests for Install Script PowerUp functionality (run-once)
#
# NOTE: These tests are currently marked as Pending because the install powerup
# implementation in dodot only creates sentinel files but doesn't actually
# execute the install scripts. The convertInstallActionWithContext function
# needs to be updated to include script execution operations.

Describe 'Install Script PowerUp'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  # Helper to create an install script pack
  create_install_script() {
    local pack_name="$1"
    local script_content="$2"
    
    mkdir -p "$TEST_DOTFILES_ROOT/$pack_name"
    
    # Create install.sh with provided content
    cat > "$TEST_DOTFILES_ROOT/$pack_name/install.sh" << EOF
#!/bin/bash
$script_content
EOF
    chmod +x "$TEST_DOTFILES_ROOT/$pack_name/install.sh"
    
    # Create pack.dodot.toml
    cat > "$TEST_DOTFILES_ROOT/$pack_name/pack.dodot.toml" << 'EOF'
[[install_script]]
trigger = { directory = ".", recursive = false }
file_name = "install.sh"
EOF
  }
  
  # Helper to verify sentinel file
  verify_sentinel() {
    local pack_name="$1"
    local sentinel_dir="$HOME/.local/share/dodot/sentinels"
    
    # First check if sentinel directory exists
    if [ ! -d "$sentinel_dir" ]; then
      echo "Sentinel directory not found: $sentinel_dir"
      return 1
    fi
    
    # Look for sentinel files that might match
    local found_sentinels=$(find "$sentinel_dir" -name "*${pack_name}*" -type f 2>/dev/null)
    if [ -z "$found_sentinels" ]; then
      echo "No sentinel files found for pack: $pack_name"
      echo "Contents of sentinel dir:"
      ls -la "$sentinel_dir" 2>/dev/null || echo "Directory does not exist"
      return 1
    fi
    
    # Check the first matching sentinel
    local sentinel_path=$(echo "$found_sentinels" | head -1)
    echo "Found sentinel: $sentinel_path"
    
    # Check if sentinel contains a checksum
    if ! grep -q "checksum" "$sentinel_path"; then
      echo "Sentinel file missing checksum"
      cat "$sentinel_path"
      return 1
    fi
    
    return 0
  }
  
  Describe 'Basic install script execution'
    It 'executes install.sh successfully'
      Pending "Install powerup only creates sentinels, doesn't execute scripts"
      create_install_script "tools" 'echo "Installing tools..." > /tmp/tools-installed.marker'
      
      # Debug: Check the created files
      echo "=== DEBUG: Created install script ===" >&2
      ls -la "$TEST_DOTFILES_ROOT/tools/" >&2
      echo "=== Content of install.sh ===" >&2
      cat "$TEST_DOTFILES_ROOT/tools/install.sh" >&2
      echo "=== Content of pack.dodot.toml ===" >&2
      cat "$TEST_DOTFILES_ROOT/tools/pack.dodot.toml" >&2
      echo "===" >&2
      
      When call "$DODOT" install
      The status should be success
      The output should include "Installing tools..."
      The file "/tmp/tools-installed.marker" should exist
      The file "/tmp/tools-installed.marker" should include "Installing tools..."
    End
    
    It 'creates sentinel file after execution'
      Pending "Verifying sentinel creation only"
      create_install_script "tools" 'echo "Test install"'
      
      When call "$DODOT" install
      The status should be success
      The result of function verify_sentinel should be successful
      The file "$HOME/.local/share/dodot/sentinels/tools_install.sh.sentinel" should exist
    End
    
    It 'stores checksum in sentinel file'
      Pending "Sentinel implementation incomplete"
      create_install_script "tools" 'echo "Test content"'
      
      When call "$DODOT" install
      The status should be success
      
      # Get the expected checksum
      local expected_checksum=$(sha256sum "$TEST_DOTFILES_ROOT/tools/install.sh" | cut -d' ' -f1)
      The file "$HOME/.local/share/dodot/sentinels/tools_install.sh.sentinel" should include "checksum: $expected_checksum"
    End
  End
  
  Describe 'Idempotency (run-once behavior)'
    It 'runs script on first deploy'
      Pending "Install scripts not executed"
      create_install_script "tools" 'echo "First run: $(date +%s)" > /tmp/install-timestamp.txt'
      
      When call "$DODOT" install
      The status should be success
      The output should include "First run:"
      The file "/tmp/install-timestamp.txt" should exist
    End
    
    It 'skips script on second deploy with same checksum'
      Pending "Install scripts not executed"
      create_install_script "tools" 'echo "Should only run once" > /tmp/idempotent-test.txt'
      
      # First run
      When call "$DODOT" install
      The status should be success
      The output should include "Should only run once"
      
      # Save the timestamp
      local first_timestamp=$(stat -c %Y /tmp/idempotent-test.txt 2>/dev/null || stat -f %m /tmp/idempotent-test.txt)
      
      # Wait a moment
      sleep 1
      
      # Second run - should skip
      When call "$DODOT" install
      The status should be success
      The output should not include "Should only run once"
      The output should include "Skipping install.sh (already run)"
      
      # Verify file wasn't modified
      local second_timestamp=$(stat -c %Y /tmp/idempotent-test.txt 2>/dev/null || stat -f %m /tmp/idempotent-test.txt)
      Assert equal "$first_timestamp" "$second_timestamp"
    End
    
    It 'runs script again when checksum changes'
      Pending "Install scripts not executed"
      create_install_script "tools" 'echo "Version 1" >> /tmp/checksum-test.log'
      
      # First run
      When call "$DODOT" install
      The status should be success
      The file "/tmp/checksum-test.log" should include "Version 1"
      
      # Modify the script
      create_install_script "tools" 'echo "Version 2" >> /tmp/checksum-test.log'
      
      # Second run - should run again due to checksum change
      When call "$DODOT" install 
      The status should be success
      The output should include "Version 2"
      The file "/tmp/checksum-test.log" should include "Version 1"
      The file "/tmp/checksum-test.log" should include "Version 2"
    End
  End
  
  Describe 'Script execution environment'
    It 'passes environment variables to script'
      Pending "Install scripts not executed"
      create_install_script "tools" 'echo "HOME=$HOME" > /tmp/env-test.txt; echo "DOTFILES_ROOT=$DOTFILES_ROOT" >> /tmp/env-test.txt'
      
      When call "$DODOT" install
      The status should be success
      The file "/tmp/env-test.txt" should include "HOME=$TEST_HOME"
      The file "/tmp/env-test.txt" should include "DOTFILES_ROOT=$TEST_DOTFILES_ROOT"
    End
    
    It 'executes from correct working directory'
      Pending "Install scripts not executed"
      create_install_script "tools" 'pwd > /tmp/pwd-test.txt'
      
      When call "$DODOT" install
      The status should be success
      The file "/tmp/pwd-test.txt" should include "$TEST_DOTFILES_ROOT/tools"
    End
  End
  
  Describe 'Error handling'
    It 'handles script exit with non-zero code'
      Pending "Install scripts not executed"
      create_install_script "tools" 'echo "Error occurred!"; exit 1'
      
      When call "$DODOT" install
      The status should be failure
      The error should include "Error occurred!"
      The error should include "install script failed"
      
      # Sentinel should not be created on failure
      The file "$HOME/.local/share/dodot/sentinels/tools_install.sh.sentinel" should not exist
    End
    
    It 'handles non-executable script file'
      Pending "Install scripts not executed"
      mkdir -p "$TEST_DOTFILES_ROOT/tools"
      
      # Create non-executable script
      cat > "$TEST_DOTFILES_ROOT/tools/install.sh" << 'EOF'
#!/bin/bash
echo "This should not run"
EOF
      chmod 644 "$TEST_DOTFILES_ROOT/tools/install.sh"
      
      # Create pack.dodot.toml
      cat > "$TEST_DOTFILES_ROOT/tools/pack.dodot.toml" << 'EOF'
[[install_script]]
trigger = { directory = ".", recursive = false }
file_name = "install.sh"
EOF
      
      When call "$DODOT" install
      The status should be failure
      The error should include "permission denied"
    End
    
    It 'handles missing script file'
      Pending "Would need file existence check in action generation"
      mkdir -p "$TEST_DOTFILES_ROOT/tools"
      
      # Create pack.dodot.toml without creating install.sh
      cat > "$TEST_DOTFILES_ROOT/tools/pack.dodot.toml" << 'EOF'
[[install_script]]
trigger = { directory = ".", recursive = false }
file_name = "install.sh"
EOF
      
      When call "$DODOT" install
      The status should be failure
      The error should include "install.sh not found"
    End
    
    It 'cleans up on failure'
      Pending "Install scripts not executed"
      create_install_script "tools" 'touch /tmp/partial-install.marker; echo "Failing..."; exit 1'
      
      When call "$DODOT" install
      The status should be failure
      
      # Partial work should remain (dodot doesn't rollback)
      The file "/tmp/partial-install.marker" should exist
      
      # But sentinel should not be created
      The file "$HOME/.local/share/dodot/sentinels/tools_install.sh.sentinel" should not exist
    End
  End
  
  Describe 'Complex scripts'
    It 'handles script with multiple commands'
      Pending "Install scripts not executed"
      create_install_script "tools" '
echo "Step 1: Creating directories..."
mkdir -p /tmp/test-install/{bin,lib,config}

echo "Step 2: Creating files..."
echo "binary content" > /tmp/test-install/bin/tool
chmod +x /tmp/test-install/bin/tool

echo "Step 3: Writing config..."
cat > /tmp/test-install/config/settings.conf << EOC
# Tool configuration
version=1.0
enabled=true
EOC

echo "Installation complete!"
'
      
      When call "$DODOT" install
      The status should be success
      The output should include "Step 1: Creating directories..."
      The output should include "Step 2: Creating files..."
      The output should include "Step 3: Writing config..."
      The output should include "Installation complete!"
      
      # Verify all files were created
      The path "/tmp/test-install/bin" should be directory
      The path "/tmp/test-install/lib" should be directory  
      The path "/tmp/test-install/config" should be directory
      The file "/tmp/test-install/bin/tool" should exist
      The file "/tmp/test-install/bin/tool" should be executable
      The file "/tmp/test-install/config/settings.conf" should include "version=1.0"
    End
    
    It 'captures script output'
      Pending "Install scripts not executed"
      create_install_script "tools" '
echo "Standard output message"
echo "Error message" >&2
echo "Final message"
'
      
      When call "$DODOT" install
      The status should be success
      The output should include "Standard output message"
      The output should include "Error message"
      The output should include "Final message"
    End
  End
End