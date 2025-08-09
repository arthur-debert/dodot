#!/usr/bin/env bash
# Shell-specific assertion functions for dodot live system tests

# assert_shell_profile_sourced() - Verify shell profile is sourced
# Args:
#   $1 - pack name
#   $2 - file path relative to pack
#
# Example: assert_shell_profile_sourced "nvim" "alias.sh"
assert_shell_profile_sourced() {
    local pack="$1"
    local file="$2"
    
    if [ -z "$pack" ] || [ -z "$file" ]; then
        echo "ERROR: assert_shell_profile_sourced requires pack and file arguments" >&2
        return 1
    fi
    
    # Check if DODOT_SHELL_SOURCE_FLAG contains the entry
    local expected_entry="$pack/$file"
    if [ -z "$DODOT_SHELL_SOURCE_FLAG" ]; then
        echo "FAIL: DODOT_SHELL_SOURCE_FLAG is not set" >&2
        debug_shell_integration
        return 1
    fi
    
    # Check if the flag contains our file
    if [[ ":$DODOT_SHELL_SOURCE_FLAG:" != *":$expected_entry:"* ]]; then
        echo "FAIL: Shell profile not sourced: $expected_entry" >&2
        echo "  DODOT_SHELL_SOURCE_FLAG=$DODOT_SHELL_SOURCE_FLAG" >&2
        debug_shell_integration
        return 1
    fi
    
    echo "PASS: Shell profile sourced: $pack/$file"
    return 0
}

# assert_profile_in_init() - Verify entry exists in init.sh
# Args:
#   $1 - pack name
#   $2 - file path relative to pack
assert_profile_in_init() {
    local pack="$1"
    local file="$2"
    
    if [ -z "$pack" ] || [ -z "$file" ]; then
        echo "ERROR: assert_profile_in_init requires pack and file arguments" >&2
        return 1
    fi
    
    local init_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/shell/init.sh"
    if [ ! -f "$init_file" ]; then
        echo "FAIL: init.sh not found: $init_file" >&2
        return 1
    fi
    
    # Check if init.sh contains a source line for this file
    local source_path="$DOTFILES_ROOT/$pack/$file"
    if ! grep -q "source \"$source_path\"" "$init_file" 2>/dev/null; then
        echo "FAIL: init.sh does not source: $pack/$file" >&2
        echo "  Looking for: source \"$source_path\"" >&2
        echo "  init.sh content:" >&2
        cat "$init_file" | sed 's/^/    /' >&2
        return 1
    fi
    
    echo "PASS: Profile in init.sh: $pack/$file"
    return 0
}

# assert_path_added() - Verify directory is added to PATH
# Args:
#   $1 - directory path
assert_path_added() {
    local dir="$1"
    
    if [ -z "$dir" ]; then
        echo "ERROR: assert_path_added requires directory argument" >&2
        return 1
    fi
    
    # Check if PATH contains the directory
    if [[ ":$PATH:" != *":$dir:"* ]]; then
        echo "FAIL: Directory not in PATH: $dir" >&2
        echo "  PATH=$PATH" >&2
        return 1
    fi
    
    echo "PASS: Directory in PATH: $dir"
    return 0
}

# Export functions
export -f assert_shell_profile_sourced
export -f assert_profile_in_init
export -f assert_path_added