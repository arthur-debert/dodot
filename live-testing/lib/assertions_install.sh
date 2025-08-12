#!/usr/bin/env bash
# Install-related assertion functions for dodot live system tests

# assert_install_script_executed() - Verify install script was executed
# Args:
#   $1 - pack name
#
# Example: assert_install_script_executed "tools"
assert_install_script_executed() {
    local pack="$1"
    
    if [ -z "$pack" ]; then
        echo "ERROR: assert_install_script_executed requires pack argument" >&2
        return 1
    fi
    
    # Check sentinel file exists
    local sentinel_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/install/sentinels/$pack"
    if [ ! -f "$sentinel_file" ]; then
        echo "FAIL: Install sentinel not found for pack: $pack" >&2
        echo "  Expected: $sentinel_file" >&2
        ls -la "$(dirname "$sentinel_file")" 2>/dev/null || true
        return 1
    fi
    
    # Check install script was copied
    local install_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/install/$pack"
    local script_file="$install_dir/install.sh"
    if [ ! -f "$script_file" ]; then
        echo "FAIL: Install script not copied for pack: $pack" >&2
        echo "  Expected: $script_file" >&2
        return 1
    fi
    
    echo "PASS: Install script executed: $pack"
    return 0
}

# assert_install_script_not_executed() - Verify install script was not executed
# Args:
#   $1 - pack name
assert_install_script_not_executed() {
    local pack="$1"
    
    if [ -z "$pack" ]; then
        echo "ERROR: assert_install_script_not_executed requires pack argument" >&2
        return 1
    fi
    
    local sentinel_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/install/sentinels/$pack"
    if [ -f "$sentinel_file" ]; then
        echo "FAIL: Install sentinel exists when it shouldn't: $pack" >&2
        echo "  Found: $sentinel_file" >&2
        return 1
    fi
    
    echo "PASS: Install script not executed: $pack"
    return 0
}

# assert_install_artifact_exists() - Verify a file/directory created by install script
# Args:
#   $1 - path to expected artifact
assert_install_artifact_exists() {
    local artifact="$1"
    
    if [ -z "$artifact" ]; then
        echo "ERROR: assert_install_artifact_exists requires artifact path" >&2
        return 1
    fi
    
    if [ ! -e "$artifact" ]; then
        echo "FAIL: Install artifact not found: $artifact" >&2
        return 1
    fi
    
    echo "PASS: Install artifact exists: $artifact"
    return 0
}

# Export functions
export -f assert_install_script_executed
export -f assert_install_script_not_executed
export -f assert_install_artifact_exists