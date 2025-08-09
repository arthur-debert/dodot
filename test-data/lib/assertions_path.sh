#!/usr/bin/env bash
# Path-related assertion functions for dodot live system tests

# assert_path_deployed() - Verify directory is deployed to path
# Args:
#   $1 - pack name
#   $2 - directory name relative to pack
#
# Example: assert_path_deployed "tools" "bin"
assert_path_deployed() {
    local pack="$1"
    local dir="$2"
    
    if [ -z "$pack" ] || [ -z "$dir" ]; then
        echo "ERROR: assert_path_deployed requires pack and directory arguments" >&2
        return 1
    fi
    
    # Check deployed symlink exists
    local deployed_link="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/deployed/path/$pack-$dir"
    if [ ! -L "$deployed_link" ]; then
        echo "FAIL: Path symlink not found: $deployed_link" >&2
        ls -la "$(dirname "$deployed_link")" 2>/dev/null || true
        return 1
    fi
    
    # Check it points to the right directory
    local expected_target="$DOTFILES_ROOT/$pack/$dir"
    local actual_target=$(readlink "$deployed_link")
    if [ "$actual_target" != "$expected_target" ]; then
        echo "FAIL: Path symlink points to wrong location" >&2
        echo "  Expected: $expected_target" >&2
        echo "  Actual: $actual_target" >&2
        return 1
    fi
    
    echo "PASS: Path deployed: $pack/$dir"
    return 0
}

# assert_path_in_shell_init() - Verify PATH addition in shell init
# Args:
#   $1 - directory path
assert_path_in_shell_init() {
    local dir_path="$1"
    
    if [ -z "$dir_path" ]; then
        echo "ERROR: assert_path_in_shell_init requires directory path" >&2
        return 1
    fi
    
    local init_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/shell/init.sh"
    if [ ! -f "$init_file" ]; then
        echo "FAIL: Shell init file not found: $init_file" >&2
        return 1
    fi
    
    # Check if init.sh contains PATH export for this directory
    if ! grep -q "export PATH=\"$dir_path:\$PATH\"" "$init_file" 2>/dev/null; then
        echo "FAIL: PATH addition not found in init.sh for: $dir_path" >&2
        echo "  init.sh content:" >&2
        cat "$init_file" | sed 's/^/    /' >&2
        return 1
    fi
    
    echo "PASS: PATH in init.sh: $dir_path"
    return 0
}

# assert_executable_available() - Verify an executable is available via deployed path
# Args:
#   $1 - executable name
#   $2 - pack/directory it should be in
assert_executable_available() {
    local exe_name="$1"
    local pack_dir="$2"
    
    if [ -z "$exe_name" ] || [ -z "$pack_dir" ]; then
        echo "ERROR: assert_executable_available requires executable name and pack/dir" >&2
        return 1
    fi
    
    local deployed_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/deployed/path/$pack_dir"
    local exe_path="$deployed_dir/$exe_name"
    
    if [ ! -x "$exe_path" ]; then
        echo "FAIL: Executable not found or not executable: $exe_path" >&2
        return 1
    fi
    
    echo "PASS: Executable available: $exe_name in $pack_dir"
    return 0
}

# Export functions
export -f assert_path_deployed
export -f assert_path_in_shell_init
export -f assert_executable_available