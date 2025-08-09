#!/usr/bin/env bash
# Debug tooling for dodot live system tests
# Provides functions to dump system state for debugging test failures

# debug_state() - Dumps complete system state
# Shows dotfiles, home, dodot data directories and environment
debug_state() {
    echo "========================================="
    echo "DEBUG STATE DUMP"
    echo "========================================="
    echo ""
    
    # Environment variables
    echo "=== ENVIRONMENT ==="
    env | grep -E "(DODOT|HOME|DOTFILES)" | sort || true
    echo ""
    
    # Dotfiles root
    if [ -n "$DOTFILES_ROOT" ] && [ -d "$DOTFILES_ROOT" ]; then
        echo "=== DOTFILES ROOT: $DOTFILES_ROOT ==="
        if command -v tree >/dev/null 2>&1; then
            tree -a -L 3 "$DOTFILES_ROOT" 2>/dev/null || ls -la "$DOTFILES_ROOT"
        else
            ls -la "$DOTFILES_ROOT"
        fi
    else
        echo "=== DOTFILES ROOT: NOT SET OR MISSING ==="
    fi
    echo ""
    
    # Home directory (limited depth to avoid clutter)
    if [ -n "$HOME" ] && [ -d "$HOME" ]; then
        echo "=== HOME: $HOME ==="
        if command -v tree >/dev/null 2>&1; then
            tree -a -L 2 "$HOME" 2>/dev/null | head -50 || ls -la "$HOME"
        else
            ls -la "$HOME"
        fi
    else
        echo "=== HOME: NOT SET OR MISSING ==="
    fi
    echo ""
    
    # Dodot data directory
    local data_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}"
    if [ -d "$data_dir" ]; then
        echo "=== DODOT DATA DIR: $data_dir ==="
        if command -v tree >/dev/null 2>&1; then
            tree -a "$data_dir" 2>/dev/null || ls -la "$data_dir"
        else
            find "$data_dir" -type f -o -type l | sort || ls -la "$data_dir"
        fi
    else
        echo "=== DODOT DATA DIR: NOT FOUND ==="
    fi
    echo ""
    
    # Recent dodot logs
    local log_file="$HOME/.local/state/dodot/dodot.log"
    if [ -f "$log_file" ]; then
        echo "=== RECENT DODOT LOGS ==="
        tail -20 "$log_file" 2>/dev/null || echo "Could not read log file"
    else
        echo "=== DODOT LOGS: NOT FOUND ==="
    fi
    echo ""
    
    echo "========================================="
}

# debug_on_fail() - Automatically called on test failures
# Can be used in test teardown or trap
debug_on_fail() {
    local exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo ""
        echo "TEST FAILED WITH EXIT CODE: $exit_code"
        debug_state
    fi
    return $exit_code
}

# debug_symlinks() - Show all symlinks in home and their targets
debug_symlinks() {
    echo "=== SYMLINKS IN HOME ==="
    if [ -n "$HOME" ] && [ -d "$HOME" ]; then
        find "$HOME" -maxdepth 3 -type l -exec ls -la {} \; 2>/dev/null | \
            grep -v "/.cache/" | grep -v "/.local/share/dodot/deployed/" || true
    fi
    echo ""
    
    echo "=== DODOT DEPLOYED SYMLINKS ==="
    local deployed_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/deployed"
    if [ -d "$deployed_dir" ]; then
        find "$deployed_dir" -type l -exec ls -la {} \; 2>/dev/null || true
    fi
    echo ""
}

# debug_shell_integration() - Show shell integration status
debug_shell_integration() {
    echo "=== SHELL INTEGRATION STATUS ==="
    
    # Check if init script is sourced
    if [ -n "$DODOT_SHELL_SOURCE_FLAG" ]; then
        echo "DODOT_SHELL_SOURCE_FLAG=$DODOT_SHELL_SOURCE_FLAG"
    else
        echo "DODOT_SHELL_SOURCE_FLAG not set"
    fi
    
    # Check deployment metadata
    local metadata_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/deployment-metadata"
    if [ -f "$metadata_file" ]; then
        echo ""
        echo "Deployment metadata:"
        cat "$metadata_file"
    else
        echo "No deployment metadata found"
    fi
    
    # Check shell init files
    local shell_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/shell"
    if [ -d "$shell_dir" ]; then
        echo ""
        echo "Shell integration files:"
        ls -la "$shell_dir" 2>/dev/null || true
    fi
    echo ""
}

# Export functions for use in tests
export -f debug_state
export -f debug_on_fail
export -f debug_symlinks
export -f debug_shell_integration