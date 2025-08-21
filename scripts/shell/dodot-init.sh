#!/usr/bin/env bash
# dodot-init.sh - Shell integration script for dodot
# This script is sourced by the user's shell configuration to integrate dodot

# Determine DODOT_DATA_DIR
if [ -z "$DODOT_DATA_DIR" ]; then
    if [ -n "$XDG_DATA_HOME" ]; then
        DODOT_DATA_DIR="$XDG_DATA_HOME/dodot"
    else
        DODOT_DATA_DIR="$HOME/.local/share/dodot"
    fi
fi

# Ensure the data directory exists
if [ ! -d "$DODOT_DATA_DIR" ]; then
    return 0
fi

# Define the deployed directory
DODOT_DEPLOYED_DIR="$DODOT_DATA_DIR/deployed"

# 1. Source all shell profile scripts (aliases, environment variables, etc.)
if [ -d "$DODOT_DEPLOYED_DIR/shell_profile" ]; then
    for script in "$DODOT_DEPLOYED_DIR/shell_profile"/*.sh; do
        if [ -f "$script" ] && [ -r "$script" ]; then
            # Check if the symlink target exists
            if [ -e "$script" ]; then
                source "$script"
            fi
        fi
    done
fi

# 2. Add all directories to PATH
if [ -d "$DODOT_DEPLOYED_DIR/path" ]; then
    for dir in "$DODOT_DEPLOYED_DIR/path"/*; do
        if [ -d "$dir" ] && [ -r "$dir" ]; then
            # Check if the symlink target exists
            if [ -e "$dir" ]; then
                # Prepend to PATH to give precedence to dodot-managed bins
                export PATH="$dir:$PATH"
            fi
        fi
    done
fi

# 3. Source additional shell files
if [ -d "$DODOT_DEPLOYED_DIR/shell_source" ]; then
    for script in "$DODOT_DEPLOYED_DIR/shell_source"/*.sh; do
        if [ -f "$script" ] && [ -r "$script" ]; then
            # Check if the symlink target exists
            if [ -e "$script" ]; then
                source "$script"
            fi
        fi
    done
fi

# Export DODOT_DATA_DIR for potential use by dodot commands
export DODOT_DATA_DIR

# Helper function to check if a run-once handler needs to run
# Usage: dodot_should_run_once <type> <pack> <checksum>
# Returns: 0 if should run, 1 if already run with same checksum
dodot_should_run_once() {
    local type="$1"
    local pack="$2"
    local new_checksum="$3"
    
    if [ -z "$type" ] || [ -z "$pack" ] || [ -z "$new_checksum" ]; then
        return 0  # Run if missing arguments
    fi
    
    local sentinel_dir="$DODOT_DATA_DIR/$type"
    local sentinel_file="$sentinel_dir/$pack"
    
    # If sentinel doesn't exist, should run
    if [ ! -f "$sentinel_file" ]; then
        return 0
    fi
    
    # Check if checksum matches
    local existing_checksum=$(cat "$sentinel_file" 2>/dev/null | head -1)
    if [ "$existing_checksum" = "$new_checksum" ]; then
        return 1  # Already run with same checksum
    fi
    
    return 0  # Checksum changed, should run
}

# Add a helper function for debugging
dodot_status() {
    echo "dodot deployment status:"
    echo "DODOT_DATA_DIR: $DODOT_DATA_DIR"
    echo ""
    
    if [ -d "$DODOT_DEPLOYED_DIR" ]; then
        echo "Deployed items:"
        
        # Shell profiles
        if [ -d "$DODOT_DEPLOYED_DIR/shell_profile" ] && [ -n "$(ls -A "$DODOT_DEPLOYED_DIR/shell_profile" 2>/dev/null)" ]; then
            echo "  Shell profiles:"
            for item in "$DODOT_DEPLOYED_DIR/shell_profile"/*; do
                if [ -L "$item" ]; then
                    target=$(readlink "$item")
                    if [ -e "$item" ]; then
                        echo "    $(basename "$item") -> $target"
                    else
                        echo "    $(basename "$item") -> $target [broken]"
                    fi
                fi
            done
        fi
        
        # PATH directories
        if [ -d "$DODOT_DEPLOYED_DIR/path" ] && [ -n "$(ls -A "$DODOT_DEPLOYED_DIR/path" 2>/dev/null)" ]; then
            echo "  PATH additions:"
            for item in "$DODOT_DEPLOYED_DIR/path"/*; do
                if [ -L "$item" ]; then
                    target=$(readlink "$item")
                    if [ -e "$item" ]; then
                        echo "    $(basename "$item") -> $target"
                    else
                        echo "    $(basename "$item") -> $target [broken]"
                    fi
                fi
            done
        fi
        
        # Shell source files
        if [ -d "$DODOT_DEPLOYED_DIR/shell_source" ] && [ -n "$(ls -A "$DODOT_DEPLOYED_DIR/shell_source" 2>/dev/null)" ]; then
            echo "  Shell sources:"
            for item in "$DODOT_DEPLOYED_DIR/shell_source"/*; do
                if [ -L "$item" ]; then
                    target=$(readlink "$item")
                    if [ -e "$item" ]; then
                        echo "    $(basename "$item") -> $target"
                    else
                        echo "    $(basename "$item") -> $target [broken]"
                    fi
                fi
            done
        fi
        
        # Symlinks
        if [ -d "$DODOT_DEPLOYED_DIR/symlink" ] && [ -n "$(ls -A "$DODOT_DEPLOYED_DIR/symlink" 2>/dev/null)" ]; then
            echo "  Symlinked files:"
            for item in "$DODOT_DEPLOYED_DIR/symlink"/*; do
                if [ -L "$item" ]; then
                    target=$(readlink "$item")
                    if [ -e "$item" ]; then
                        echo "    $(basename "$item") -> $target"
                    else
                        echo "    $(basename "$item") -> $target [broken]"
                    fi
                fi
            done
        fi
        
        # Run-once handlers status
        echo ""
        echo "Run-once handlers:"
        
        # Brewfile installations
        if [ -d "$DODOT_DATA_DIR/brewfile" ] && [ -n "$(ls -A "$DODOT_DATA_DIR/brewfile" 2>/dev/null)" ]; then
            echo "  Brewfile installations (completed):"
            for sentinel in "$DODOT_DATA_DIR/brewfile"/*; do
                if [ -f "$sentinel" ]; then
                    pack=$(basename "$sentinel")
                    checksum=$(cat "$sentinel" 2>/dev/null | head -1)
                    echo "    $pack (checksum: ${checksum:0:16}...)"
                fi
            done
        else
            echo "  Brewfile installations: none"
        fi
        
        # Install scripts
        if [ -d "$DODOT_DATA_DIR/install" ] && [ -n "$(ls -A "$DODOT_DATA_DIR/install" 2>/dev/null)" ]; then
            echo "  Install scripts (completed):"
            for sentinel in "$DODOT_DATA_DIR/install"/*; do
                if [ -f "$sentinel" ]; then
                    pack=$(basename "$sentinel")
                    checksum=$(cat "$sentinel" 2>/dev/null | head -1)
                    echo "    $pack (checksum: ${checksum:0:16}...)"
                fi
            done
        else
            echo "  Install scripts: none"
        fi
    else
        echo "No deployments found."
    fi
}