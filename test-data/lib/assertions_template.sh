#!/usr/bin/env bash
# Template-related assertion functions for dodot live system tests

# assert_template_processed() - Verify template was processed
# Args:
#   $1 - pack name
#   $2 - template file (without .tmpl extension)
#   $3 - target path
#
# Example: assert_template_processed "config" "gitconfig" "$HOME/.gitconfig"
assert_template_processed() {
    local pack="$1"
    local template="$2"
    local target="$3"
    
    if [ -z "$pack" ] || [ -z "$template" ] || [ -z "$target" ]; then
        echo "ERROR: assert_template_processed requires pack, template, and target arguments" >&2
        return 1
    fi
    
    # Check target file exists
    if [ ! -f "$target" ]; then
        echo "FAIL: Processed template not found: $target" >&2
        return 1
    fi
    
    # Check it's not a symlink (templates create real files)
    if [ -L "$target" ]; then
        echo "FAIL: Target is a symlink, should be a regular file: $target" >&2
        return 1
    fi
    
    echo "PASS: Template processed: $pack/$template.tmpl -> $target"
    return 0
}

# assert_template_contains() - Verify processed template contains expected content
# Args:
#   $1 - target file path
#   $2 - expected content (can be partial)
assert_template_contains() {
    local target="$1"
    local expected="$2"
    
    if [ -z "$target" ] || [ -z "$expected" ]; then
        echo "ERROR: assert_template_contains requires target and expected content" >&2
        return 1
    fi
    
    if [ ! -f "$target" ]; then
        echo "FAIL: Target file not found: $target" >&2
        return 1
    fi
    
    if ! grep -q "$expected" "$target" 2>/dev/null; then
        echo "FAIL: Template output doesn't contain expected content" >&2
        echo "  Looking for: $expected" >&2
        echo "  In file: $target" >&2
        echo "  File content:" >&2
        cat "$target" | sed 's/^/    /' >&2
        return 1
    fi
    
    echo "PASS: Template contains: $expected"
    return 0
}

# assert_template_variable_expanded() - Verify template variable was expanded
# Args:
#   $1 - target file path
#   $2 - variable name (e.g., "HOME", "USER")
assert_template_variable_expanded() {
    local target="$1"
    local var_name="$2"
    
    if [ -z "$target" ] || [ -z "$var_name" ]; then
        echo "ERROR: assert_template_variable_expanded requires target and variable name" >&2
        return 1
    fi
    
    if [ ! -f "$target" ]; then
        echo "FAIL: Target file not found: $target" >&2
        return 1
    fi
    
    # Check that the template syntax is not present (was replaced)
    if grep -E "\{\{\.?$var_name\}\}|\$$var_name" "$target" >/dev/null 2>&1; then
        echo "FAIL: Template variable not expanded: $var_name" >&2
        echo "  Found unexpanded variable in: $target" >&2
        grep -n -E "\{\{\.?$var_name\}\}|\$$var_name" "$target" | sed 's/^/    /' >&2
        return 1
    fi
    
    echo "PASS: Template variable expanded: $var_name"
    return 0
}

# Export functions
export -f assert_template_processed
export -f assert_template_contains
export -f assert_template_variable_expanded