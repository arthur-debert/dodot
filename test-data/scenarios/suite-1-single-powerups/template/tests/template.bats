#!/usr/bin/env bats
# Minimal test for template power-up - happy path only

# Load common test setup with debug support
source /workspace/test-data/lib/common.sh

# Setup before all tests
setup() {
    setup_with_debug
}

# Cleanup after each test

# Cleanup after each test
teardown() {
    teardown_with_debug
}

@test "template: YES - processed and variables expanded" {
    # TODO: Template variables are not being expanded - see GitHub issue #517
    # skip "Template variables not expanded - known bug #517"
    
    # Set test environment variables
    export USER="testuser"
    
    # Deploy the tools pack with template
    dodot_run deploy tools
    [ "$status" -eq 0 ]
    
    # Verify template was processed
    assert_template_processed "tools" "config" "$HOME/config"
    
    # Verify variable was expanded
    assert_template_contains "$HOME/config" "user = testuser"
    
    # Verify template syntax was replaced (not present in output)
    assert_template_variable_expanded "$HOME/config" "USER"
}

@test "template: NO - not processed (verify absence)" {
    # Create a pack with no .tmpl files
    mkdir -p "$DOTFILES_ROOT/vim"
    echo "set number" > "$DOTFILES_ROOT/vim/vimrc"
    
    # Deploy the vim pack (which has no templates)
    dodot_run deploy vim
    [ "$status" -eq 0 ]
    
    # Verify no template outputs were created
    # Since there are no templates, there should be no processed files
    # Check that our normal non-template file was symlinked instead
    [ -L "$HOME/vimrc" ] && [ -f "$HOME/vimrc" ]
}