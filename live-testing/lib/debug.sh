#!/usr/bin/env bash
# Debug tooling for dodot live system tests
# Provides functions to dump system state for debugging test failures

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# debug_state() - Dumps complete system state
# Shows dotfiles, home, dodot data directories and environment
debug_state() {
    echo ""
    echo -e "${RED}=========================================${NC}"
    echo -e "${RED}DEBUG STATE DUMP${NC}"
    echo -e "${RED}=========================================${NC}"
    echo ""
    
    # Timestamp and test context
    echo -e "${YELLOW}=== TEST CONTEXT ===${NC}"
    echo "Timestamp: $(date '+%Y-%m-%d %H:%M:%S')"
    echo "Test file: ${BATS_TEST_FILENAME:-unknown}"
    echo "Test name: ${BATS_TEST_DESCRIPTION:-unknown}"
    echo ""
    
    # Environment variables - all deploy related
    echo -e "${YELLOW}=== ENVIRONMENT VARIABLES ===${NC}"
    echo "Deploy state variables:"
    env | grep -E "^(DODOT|DOTFILES|HOME|PATH|SHELL)" | sort || true
    echo ""
    
    # Last dodot command executed
    if [ -n "$LAST_DODOT_COMMAND" ]; then
        echo -e "${YELLOW}=== LAST DODOT COMMAND ===${NC}"
        echo "Command: $LAST_DODOT_COMMAND"
        echo "Exit code: ${LAST_DODOT_EXIT_CODE:-unknown}"
        if [ -n "$LAST_DODOT_OUTPUT" ]; then
            echo "Output:"
            echo "$LAST_DODOT_OUTPUT" | head -20
        fi
        echo ""
    fi
    
    # Dotfiles root with symlink targets
    if [ -n "$DOTFILES_ROOT" ] && [ -d "$DOTFILES_ROOT" ]; then
        echo -e "${YELLOW}=== DOTFILES ROOT: $DOTFILES_ROOT ===${NC}"
        if command -v tree >/dev/null 2>&1; then
            # Show tree with symlink targets
            tree -a -L 3 -F --dirsfirst "$DOTFILES_ROOT" 2>/dev/null || ls -la "$DOTFILES_ROOT"
        else
            find "$DOTFILES_ROOT" -maxdepth 3 \( -type f -o -type l \) -exec ls -la {} \; 2>/dev/null
        fi
    else
        echo -e "${YELLOW}=== DOTFILES ROOT: NOT SET OR MISSING ===${NC}"
    fi
    echo ""
    
    # Home directory with symlink details
    if [ -n "$HOME" ] && [ -d "$HOME" ]; then
        echo -e "${YELLOW}=== HOME: $HOME ===${NC}"
        # Show files with symlink targets
        echo "Top-level files and symlinks:"
        ls -la "$HOME" | grep -v "^total" | head -20
        echo ""
        echo "Symlinks with targets:"
        find "$HOME" -maxdepth 2 -type l -exec sh -c 'echo -n "  "; ls -la "$1" | sed "s|$HOME|~|g"' _ {} \; 2>/dev/null | grep -v "/.cache/" || true
    else
        echo -e "${YELLOW}=== HOME: NOT SET OR MISSING ===${NC}"
    fi
    echo ""
    
    # Dodot data directory with full structure
    local data_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}"
    if [ -d "$data_dir" ]; then
        echo -e "${YELLOW}=== DODOT DATA DIR: $data_dir ===${NC}"
        if command -v tree >/dev/null 2>&1; then
            tree -a -F --dirsfirst "$data_dir" 2>/dev/null || ls -la "$data_dir"
        else
            find "$data_dir" \( -type f -o -type l -o -type d \) -exec ls -ld {} \; 2>/dev/null | sort
        fi
    else
        echo -e "${YELLOW}=== DODOT DATA DIR: NOT FOUND ===${NC}"
    fi
    echo ""
    
    # Shell sourcing files
    echo -e "${YELLOW}=== SHELL INTEGRATION FILES ===${NC}"
    local init_file="$HOME/.config/dodot/shell/init.sh"
    if [ -f "$init_file" ]; then
        echo "Contents of $init_file:"
        cat "$init_file" | sed 's/^/  /'
    else
        echo "No init.sh file found"
    fi
    echo ""
    
    # Recent dodot logs with more context
    local log_file="$HOME/.local/state/dodot/dodot.log"
    if [ -f "$log_file" ]; then
        echo -e "${YELLOW}=== RECENT DODOT LOGS ===${NC}"
        echo "Last 30 lines from $log_file:"
        tail -30 "$log_file" 2>/dev/null | sed 's/^/  /' || echo "Could not read log file"
    else
        echo -e "${YELLOW}=== DODOT LOGS: NOT FOUND ===${NC}"
    fi
    echo ""
    
    echo -e "${RED}=========================================${NC}"
}

# debug_on_fail() - Automatically called on test failures
# Can be used in test teardown or trap
debug_on_fail() {
    local exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo ""
        echo -e "${RED}TEST FAILED WITH EXIT CODE: $exit_code${NC}"
        debug_state
    fi
    return $exit_code
}

# run_dodot() - Helper function to run dodot commands and capture output for debugging
# Usage: run_dodot deploy mypack
# This wrapper captures command details for debug output
run_dodot() {
    local cmd="dodot $*"
    export LAST_DODOT_COMMAND="$cmd"
    
    # Run command and capture output
    local output
    local exit_code
    
    output=$($cmd 2>&1)
    exit_code=$?
    
    export LAST_DODOT_EXIT_CODE="$exit_code"
    export LAST_DODOT_OUTPUT="$output"
    
    # Print output as normal
    if [ -n "$output" ]; then
        echo "$output"
    fi
    
    return $exit_code
}

# debug_symlinks() - Show all symlinks in home and their targets
debug_symlinks() {
    echo -e "${YELLOW}=== SYMLINKS IN HOME ===${NC}"
    if [ -n "$HOME" ] && [ -d "$HOME" ]; then
        # Show symlinks with their targets in a readable format
        find "$HOME" -maxdepth 3 -type l 2>/dev/null | \
            grep -v "/.cache/" | grep -v "/.local/share/dodot/deployed/" | \
            while read -r link; do
                local target=$(readlink "$link")
                local rel_link=${link#$HOME/}
                echo "  ~/$rel_link -> $target"
            done || true
    else
        echo "  No HOME directory set"
    fi
    echo ""
    
    echo -e "${YELLOW}=== DODOT DEPLOYED SYMLINKS ===${NC}"
    local deployed_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/deployed"
    if [ -d "$deployed_dir" ]; then
        # Show deployed symlinks organized by power-up
        for powerup_dir in "$deployed_dir"/*; do
            if [ -d "$powerup_dir" ]; then
                local powerup=$(basename "$powerup_dir")
                echo "  $powerup:"
                find "$powerup_dir" -type l -o -type f 2>/dev/null | while read -r item; do
                    if [ -L "$item" ]; then
                        local target=$(readlink "$item")
                        echo "    $(basename "$item") -> $target"
                    else
                        echo "    $(basename "$item") (file)"
                    fi
                done
            fi
        done
    else
        echo "  No deployed directory found"
    fi
    echo ""
}

# debug_shell_integration() - Show shell integration status
debug_shell_integration() {
    echo -e "${YELLOW}=== SHELL INTEGRATION STATUS ===${NC}"
    
    # Check if init script is sourced
    if [ -n "$DODOT_SHELL_SOURCE_FLAG" ]; then
        echo "DODOT_SHELL_SOURCE_FLAG=$DODOT_SHELL_SOURCE_FLAG"
    else
        echo "DODOT_SHELL_SOURCE_FLAG not set"
    fi
    
    # Show init.sh contents with line numbers
    local init_file="$HOME/.config/dodot/shell/init.sh"
    if [ -f "$init_file" ]; then
        echo ""
        echo "Contents of $init_file:"
        cat -n "$init_file" | sed 's/^/  /'
        
        # Show which files are being sourced
        echo ""
        echo "Files being sourced:"
        grep -E "^source|^\." "$init_file" 2>/dev/null | while read -r line; do
            local file=$(echo "$line" | sed -E 's/^(source|\.) +//; s/"//g')
            if [ -f "$file" ]; then
                echo "  ✓ $file (exists)"
            else
                echo "  ✗ $file (missing)"
            fi
        done
    else
        echo "No init.sh file found at $init_file"
    fi
    
    # Check deployment metadata
    local metadata_file="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/deployment-metadata"
    if [ -f "$metadata_file" ]; then
        echo ""
        echo "Deployment metadata:"
        cat "$metadata_file" | sed 's/^/  /'
    else
        echo "No deployment metadata found"
    fi
    
    # Check shell integration directory
    local shell_dir="${DODOT_DATA_DIR:-$HOME/.local/share/dodot}/shell"
    if [ -d "$shell_dir" ]; then
        echo ""
        echo "Shell integration directory contents:"
        ls -la "$shell_dir" 2>/dev/null | sed 's/^/  /' || true
    fi
    echo ""
}

# debug_timing() - Show execution timing for performance debugging
debug_timing() {
    echo -e "${YELLOW}=== EXECUTION TIMING ===${NC}"
    if [ -n "$BATS_TEST_START_TIME" ]; then
        local end_time=$(date +%s)
        local duration=$((end_time - BATS_TEST_START_TIME))
        echo "Test duration: ${duration}s"
    fi
    
    if [ -n "$LAST_DODOT_COMMAND" ] && [ -n "$DODOT_COMMAND_START_TIME" ]; then
        local cmd_end_time=$(date +%s)
        local cmd_duration=$((cmd_end_time - DODOT_COMMAND_START_TIME))
        echo "Last dodot command duration: ${cmd_duration}s"
    fi
    echo ""
}

# Export functions for use in tests
export -f debug_state
export -f debug_on_fail
export -f debug_symlinks
export -f debug_shell_integration
export -f debug_timing
export -f run_dodot