#!/usr/bin/env bash
# Assertion functions for dodot live system tests

# Source debug functions for better error output
source "$(dirname "${BASH_SOURCE[0]}")/debug.sh"

# assert_symlink_deployed() - Verify symlink deployment
# Args:
#   $1 - pack name
#   $2 - file path relative to pack
#   $3 - expected target path in home
#
# Example: assert_symlink_deployed "git" "gitconfig" "$HOME/.gitconfig"
assert_symlink_deployed() {
    local pack="$1"
    local file="$2"
    local expected_target="$3"
    
    if [ -z "$pack" ] || [ -z "$file" ] || [ -z "$expected_target" ]; then
        echo "ERROR: assert_symlink_deployed requires pack, file, and target arguments" >&2
        return 1
    fi
    
    # Check intermediate symlink exists
    local intermediate="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/deployed/symlink/$(basename "$expected_target")"
    if [ ! -L "$intermediate" ]; then
        echo "FAIL: Intermediate symlink not found: $intermediate" >&2
        debug_symlinks
        return 1
    fi
    
    # Check intermediate points to source
    local source_file="$DOTFILES_ROOT/$pack/$file"
    local intermediate_target=$(readlink "$intermediate")
    if [ "$intermediate_target" != "$source_file" ]; then
        echo "FAIL: Intermediate symlink points to wrong source" >&2
        echo "  Expected: $source_file" >&2
        echo "  Actual: $intermediate_target" >&2
        debug_symlinks
        return 1
    fi
    
    # Check target symlink exists
    if [ ! -L "$expected_target" ]; then
        echo "FAIL: Target symlink not found: $expected_target" >&2
        debug_symlinks
        return 1
    fi
    
    # Check target points to intermediate
    local target_dest=$(readlink "$expected_target")
    if [ "$target_dest" != "$intermediate" ]; then
        echo "FAIL: Target symlink points to wrong location" >&2
        echo "  Expected: $intermediate" >&2
        echo "  Actual: $target_dest" >&2
        debug_symlinks
        return 1
    fi
    
    echo "PASS: Symlink deployed: $pack/$file -> $expected_target"
    return 0
}

# assert_symlink_not_deployed() - Verify symlink is not deployed
# Args:
#   $1 - expected target path
assert_symlink_not_deployed() {
    local expected_target="$1"
    
    if [ -z "$expected_target" ]; then
        echo "ERROR: assert_symlink_not_deployed requires target argument" >&2
        return 1
    fi
    
    if [ -L "$expected_target" ] || [ -e "$expected_target" ]; then
        echo "FAIL: Target exists when it shouldn't: $expected_target" >&2
        ls -la "$expected_target" 2>/dev/null || true
        return 1
    fi
    
    echo "PASS: Symlink not deployed: $expected_target"
    return 0
}

# assert_file_exists() - Basic file existence check
# Args:
#   $1 - file path
assert_file_exists() {
    local file="$1"
    
    if [ ! -f "$file" ]; then
        echo "FAIL: File not found: $file" >&2
        return 1
    fi
    
    echo "PASS: File exists: $file"
    return 0
}

# assert_dir_exists() - Directory existence check
# Args:
#   $1 - directory path
assert_dir_exists() {
    local dir="$1"
    
    if [ ! -d "$dir" ]; then
        echo "FAIL: Directory not found: $dir" >&2
        return 1
    fi
    
    echo "PASS: Directory exists: $dir"
    return 0
}

# assert_env_set() - Environment variable check
# Args:
#   $1 - variable name
#   $2 - expected value (optional)
assert_env_set() {
    local var_name="$1"
    local expected_value="$2"
    
    local actual_value="${!var_name}"
    
    if [ -z "$actual_value" ]; then
        echo "FAIL: Environment variable not set: $var_name" >&2
        return 1
    fi
    
    if [ -n "$expected_value" ] && [ "$actual_value" != "$expected_value" ]; then
        echo "FAIL: Environment variable has wrong value: $var_name" >&2
        echo "  Expected: $expected_value" >&2
        echo "  Actual: $actual_value" >&2
        return 1
    fi
    
    echo "PASS: Environment variable set: $var_name=$actual_value"
    return 0
}

# assert_file_not_exists() - Verify file does not exist
# Args:
#   $1 - file path
assert_file_not_exists() {
    local file="$1"
    
    if [ -f "$file" ]; then
        echo "FAIL: File exists but should not: $file" >&2
        return 1
    fi
    
    echo "PASS: File does not exist: $file"
    return 0
}

# assert_dir_not_exists() - Verify directory does not exist
# Args:
#   $1 - directory path
assert_dir_not_exists() {
    local dir="$1"
    
    if [ -d "$dir" ]; then
        echo "FAIL: Directory exists but should not: $dir" >&2
        return 1
    fi
    
    echo "PASS: Directory does not exist: $dir"
    return 0
}

# assert_file_executable() - Verify file exists and is executable
# Args:
#   $1 - file path
assert_file_executable() {
    local file="$1"
    
    if [ ! -f "$file" ]; then
        echo "FAIL: File not found: $file" >&2
        return 1
    fi
    
    if [ ! -x "$file" ]; then
        echo "FAIL: File exists but is not executable: $file" >&2
        return 1
    fi
    
    echo "PASS: File is executable: $file"
    return 0
}

# Export all assertion functions
export -f assert_symlink_deployed
export -f assert_symlink_not_deployed
export -f assert_file_exists
export -f assert_file_not_exists
export -f assert_dir_exists
export -f assert_dir_not_exists
export -f assert_env_set
export -f assert_file_executable