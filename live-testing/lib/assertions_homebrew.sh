#!/usr/bin/env bash
# Homebrew-related assertion functions for dodot live system tests

# assert_brewfile_processed() - Verify Brewfile was processed
# Args:
#   $1 - pack name
#
# Example: assert_brewfile_processed "tools"
assert_brewfile_processed() {
    local pack="$1"
    
    if [ -z "$pack" ]; then
        echo "ERROR: assert_brewfile_processed requires pack argument" >&2
        return 1
    fi
    
    # Check sentinel file exists
    local sentinel_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/homebrew/$pack"
    if [ ! -f "$sentinel_file" ]; then
        echo "FAIL: Brewfile sentinel not found for pack: $pack" >&2
        echo "  Expected: $sentinel_file" >&2
        ls -la "$(dirname "$sentinel_file")" 2>/dev/null || true
        return 1
    fi
    
    echo "PASS: Brewfile processed: $pack"
    return 0
}

# assert_brewfile_not_processed() - Verify Brewfile was not processed
# Args:
#   $1 - pack name
assert_brewfile_not_processed() {
    local pack="$1"
    
    if [ -z "$pack" ]; then
        echo "ERROR: assert_brewfile_not_processed requires pack argument" >&2
        return 1
    fi
    
    local sentinel_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/homebrew/$pack"
    if [ -f "$sentinel_file" ]; then
        echo "FAIL: Brewfile sentinel exists when it shouldn't: $pack" >&2
        echo "  Found: $sentinel_file" >&2
        return 1
    fi
    
    echo "PASS: Brewfile not processed: $pack"
    return 0
}

# assert_brew_package_installed() - Verify a brew package is installed
# Args:
#   $1 - package name
#
# Note: This requires brew to be available in the test environment
assert_brew_package_installed() {
    local package="$1"
    
    if [ -z "$package" ]; then
        echo "ERROR: assert_brew_package_installed requires package name" >&2
        return 1
    fi
    
    if ! command -v brew >/dev/null 2>&1; then
        echo "SKIP: Homebrew not available in test environment" >&2
        return 0
    fi
    
    if ! brew list "$package" >/dev/null 2>&1; then
        echo "FAIL: Brew package not installed: $package" >&2
        return 1
    fi
    
    echo "PASS: Brew package installed: $package"
    return 0
}

# Export functions
export -f assert_brewfile_processed
export -f assert_brewfile_not_processed
export -f assert_brew_package_installed