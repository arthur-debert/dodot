#!/usr/bin/env bats
# Test template power-up functionality

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_template.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "template: processes .tmpl files" {
    # Deploy config pack with template
    run dodot deploy config
    [ "$status" -eq 0 ]
    
    # Verify template was processed to target location
    assert_template_processed "config" "test-config" "$HOME/test-config"
    
    # Verify it's a real file, not a symlink
    [ -f "$HOME/test-config" ]
    [ ! -L "$HOME/test-config" ]
}

@test "template: expands environment variables" {
    # Set a test user
    export USER="testuser"
    
    # Deploy config pack
    run dodot deploy config
    [ "$status" -eq 0 ]
    
    # Verify variables were expanded
    assert_template_processed "config" "test-config" "$HOME/test-config"
    assert_template_contains "$HOME/test-config" "user = testuser"
    assert_template_contains "$HOME/test-config" "home = $HOME"
    
    # Verify template syntax was replaced
    assert_template_variable_expanded "$HOME/test-config" "USER"
    assert_template_variable_expanded "$HOME/test-config" "HOME"
}

@test "template: updates file on repeated deploy" {
    # First deploy
    export USER="user1"
    run dodot deploy config
    [ "$status" -eq 0 ]
    assert_template_contains "$HOME/test-config" "user = user1"
    
    # Change variable and redeploy
    export USER="user2"
    run dodot deploy config
    [ "$status" -eq 0 ]
    
    # File should be updated
    assert_template_contains "$HOME/test-config" "user = user2"
    ! grep -q "user = user1" "$HOME/test-config"
}