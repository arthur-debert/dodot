#!/usr/bin/env bats

# Suite 4: Single Power-up Edge Cases
# This suite tests edge cases for each power-up in isolation, focusing on
# error handling, boundary conditions, and unexpected inputs that might
# occur when power-ups are used individually.

# Load common test setup with debug support
source /workspace/test-data/lib/common.sh

# Setup before all tests
setup() {
    setup_with_debug
}

# Cleanup after each test
teardown() {
    teardown_with_debug
}

# Symlink edge cases
@test "symlink: handles missing pack gracefully" {
    # Try to deploy a pack that doesn't exist
    dodot_run deploy nonexistent-pack
    
    # Should fail gracefully with non-zero exit code
    [ "$status" -ne 0 ]
    
    # Should have helpful error message
    [[ "$output" == *"nonexistent-pack"* ]]
    
    # No symlinks should be created
    [ ! -L "$HOME/nonexistent-config" ]
    
    # Verify nothing was deployed
    assert_symlink_not_deployed "nonexistent-pack" "nonexistent-config" "$HOME/nonexistent-config"
}

@test "symlink: target already exists" {
    # Create a file that will conflict with the symlink target
    echo "existing content" > "$HOME/existing-config"
    
    # Verify the file exists and has our content
    [ -f "$HOME/existing-config" ]
    grep -q "existing content" "$HOME/existing-config"
    
    # Try to deploy a pack that wants to symlink to the same target
    dodot_run deploy conflict-pack
    
    # Should fail with non-zero exit code due to conflict
    [ "$status" -ne 0 ]
    
    # Should have error message about the conflict
    [[ "$output" == *"existing-config"* ]]
    
    # Original file should still exist and be unchanged
    [ -f "$HOME/existing-config" ]
    [ ! -L "$HOME/existing-config" ]  # Should NOT be a symlink
    grep -q "existing content" "$HOME/existing-config"
    
    # Verify no symlink was created by the assertion helper
    assert_symlink_not_deployed "conflict-pack" "existing-config" "$HOME/existing-config"
}

# Shell profile edge cases
@test "shell_profile: repeated deploy doesn't duplicate entries" {
    skip "Known bug from basic scenario - not implemented"
}

# Template edge cases
@test "template: missing variables handling" {
    skip "Not implemented"
}