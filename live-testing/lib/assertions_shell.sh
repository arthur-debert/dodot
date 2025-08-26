#!/usr/bin/env bash
# Shell-specific assertion functions for dodot live system tests

# assert_shell_sourced() - Verify shell profile is sourced
# Args:
#   $1 - pack name
#   $2 - file path relative to pack
#
# Example: assert_shell_sourced "nvim" "alias.sh"
assert_shell_sourced() {
    local pack="$1"
    local file="$2"
    
    if [ -z "$pack" ] || [ -z "$file" ]; then
        echo "ERROR: assert_shell_sourced requires pack and file arguments" >&2
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

# assert_no_profile_for_pack() - Verify no shell profiles from pack are in init.sh
# Args:
#   $1 - pack name
#
# This checks that NO profiles from the specified pack were added to init.sh
assert_no_profile_for_pack() {
    local pack="$1"
    
    if [ -z "$pack" ]; then
        echo "ERROR: assert_no_profile_for_pack requires pack name" >&2
        return 1
    fi
    
    local init_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/shell/init.sh"
    
    # If init.sh doesn't exist, that's fine - no profiles were added
    if [ ! -f "$init_file" ]; then
        echo "PASS: No profiles for pack (init.sh doesn't exist): $pack"
        return 0
    fi
    
    # Check if init.sh contains any source commands for this pack
    local pack_refs=$(grep -c "# Source.*from $pack" "$init_file" 2>/dev/null || echo "0")
    if [ "$pack_refs" -gt 0 ]; then
        echo "FAIL: Found $pack_refs profile references for pack '$pack' in init.sh:" >&2
        grep "# Source.*from $pack" "$init_file" | sed 's/^/  /' >&2
        echo "  Full init.sh content:" >&2
        cat "$init_file" | sed 's/^/    /' >&2
        return 1
    fi
    
    # Also check for actual source commands pointing to the pack
    if grep -q "$DOTFILES_ROOT/$pack" "$init_file" 2>/dev/null; then
        echo "FAIL: Found source commands for pack '$pack' in init.sh:" >&2
        grep "$DOTFILES_ROOT/$pack" "$init_file" | sed 's/^/  /' >&2
        return 1
    fi
    
    echo "PASS: No profiles for pack in init.sh: $pack"
    return 0
}

# debug_shell_integration() - Print shell integration state
# Useful for debugging shell profile loading issues
debug_shell_integration() {
    echo "=== Shell Integration Debug ===" >&2
    echo "DODOT_SHELL_SOURCE_FLAG: ${DODOT_SHELL_SOURCE_FLAG:-<not set>}" >&2
    
    local init_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/shell/init.sh"
    if [ -f "$init_file" ]; then
        echo "init.sh contents:" >&2
        cat "$init_file" | sed 's/^/  /' >&2
    else
        echo "init.sh not found at: $init_file" >&2
    fi
    
    echo "=== End Shell Debug ===" >&2
}

# Export functions
export -f assert_shell_sourced
export -f assert_profile_in_init
export -f assert_path_added
export -f assert_no_profile_for_pack
export -f debug_shell_integration