#!/usr/bin/env bash
# dodot-init.sh - Shell integration script for dodot
# This script is sourced by the user's shell configuration to integrate dodot
# Its what $dodot snippet links to.

# When dodot deploys or install packs, it saves the links and state in a datat
# directory. For anything that is add to the shell, it needs to be sourced to
# make it available in the current shell session.
#
# This snippet will loop through the shell releated directories in the dodot
# data dir in source them.  That includes adding directories to the PATH, as
# per the bin handler.
#
# Determine DODOT_DATA_DIR
if [ -z "$DODOT_DATA_DIR" ]; then
    if [ -n "$XDG_DATA_HOME" ]; then
        DODOT_DATA_DIR="$XDG_DATA_HOME/dodot"
    else
        DODOT_DATA_DIR="$HOME/.local/share/dodot"
    fi
fi

# Ensure the data directory exists, else there is nothing to do
if [ ! -d "$DODOT_DATA_DIR" ]; then
    return 0
fi

# Load deployment metadata if available
# This file is created sagesy dodot during deployment and contains
# the DOTFILES_ROOT that was used at deployment time
if [ -f "$DODOT_DATA_DIR/deployment-metadata" ]; then
    # shellcheck disable=SC1091
    source "$DODOT_DATA_DIR/deployment-metadata"
fi

# Reset all tracking environment variables to prevent old values from sticking
export DODOT_SYMLINKS=""
export DODOT_SHELL_PROFILES=""
export DODOT_PATH_DIRS=""
export DODOT_SHELL_SOURCES=""
export DODOT_TEMPLATES=""
export DODOT_INSTALL_SCRIPTS=""
export DODOT_BREWFILES=""

# Define the deployed directory
DODOT_DEPLOYED_DIR="$DODOT_DATA_DIR/deployed"

# 1. Source all shell profile scripts (aliases, environment variables, etc.)
if [ -d "$DODOT_DEPLOYED_DIR/shell" ]; then
    for script in "$DODOT_DEPLOYED_DIR/shell"/*.sh; do
        if [ -f "$script" ] && [ -r "$script" ]; then
            # Check if the symlink target exists
            if [ -e "$script" ]; then
                # shellcheck disable=SC1090
                source "$script"

                # Track sourced scripts for debugging
                if [ -n "$DODOT_DEPLOYMENT_ROOT" ]; then
                    # Get the actual target of the symlink
                    target=$(readlink "$script")
                    # Strip the deployment-time dotfiles root to get relative path
                    relative_path="${target#$DODOT_DEPLOYMENT_ROOT/}"
                    # Only add if we successfully got a relative path
                    if [ "$relative_path" != "$target" ]; then
                        if [ -z "$DODOT_SHELL_PROFILES" ]; then
                            DODOT_SHELL_PROFILES="$relative_path"
                        else
                            DODOT_SHELL_PROFILES="$DODOT_SHELL_PROFILES:$relative_path"
                        fi
                    fi
                fi
            fi
        fi
    done
fi

# Export shell profiles if any were sourced
if [ -n "$DODOT_SHELL_PROFILES" ]; then
    export DODOT_SHELL_PROFILES
fi

# 2. Add all directories to PATH
if [ -d "$DODOT_DEPLOYED_DIR/path" ]; then
    for symlink in "$DODOT_DEPLOYED_DIR/path"/*; do
        if [ -L "$symlink" ] && [ -r "$symlink" ]; then
            # Check if the symlink target exists and is a directory
            if [ -e "$symlink" ] && [ -d "$symlink" ]; then
                # Prepend to PATH to give precedence to dodot-managed bins
                export PATH="$symlink:$PATH"
                
                # Track PATH additions for debugging
                if [ -n "$DODOT_DEPLOYMENT_ROOT" ]; then
                    # Get the actual target of the symlink
                    target=$(readlink "$symlink")
                    # Strip the deployment-time dotfiles root to get relative path
                    relative_path="${target#$DODOT_DEPLOYMENT_ROOT/}"
                    # Only add if we successfully got a relative path
                    if [ "$relative_path" != "$target" ]; then
                        if [ -z "$DODOT_PATH_DIRS" ]; then
                            DODOT_PATH_DIRS="$relative_path"
                        else
                            DODOT_PATH_DIRS="$DODOT_PATH_DIRS:$relative_path"
                        fi
                    fi
                fi
            fi
        fi
    done
fi

# Export PATH directories if any were added
if [ -n "$DODOT_PATH_DIRS" ]; then
    export DODOT_PATH_DIRS
fi

# 3. Source additional shell files
if [ -d "$DODOT_DEPLOYED_DIR/shell_source" ]; then
    for script in "$DODOT_DEPLOYED_DIR/shell_source"/*.sh; do
        if [ -f "$script" ] && [ -r "$script" ]; then
            # Check if the symlink target exists
            if [ -e "$script" ]; then
                # shellcheck disable=SC1090
                source "$script"
                
                # Track shell sources for debugging
                if [ -n "$DODOT_DEPLOYMENT_ROOT" ]; then
                    # Get the actual target of the symlink
                    target=$(readlink "$script")
                    # Strip the deployment-time dotfiles root to get relative path
                    relative_path="${target#$DODOT_DEPLOYMENT_ROOT/}"
                    # Only add if we successfully got a relative path
                    if [ "$relative_path" != "$target" ]; then
                        if [ -z "$DODOT_SHELL_SOURCES" ]; then
                            DODOT_SHELL_SOURCES="$relative_path"
                        else
                            DODOT_SHELL_SOURCES="$DODOT_SHELL_SOURCES:$relative_path"
                        fi
                    fi
                fi
            fi
        fi
    done
fi

# Export shell sources if any were sourced
if [ -n "$DODOT_SHELL_SOURCES" ]; then
    export DODOT_SHELL_SOURCES
fi

# 4. Track deployed symlinks (for debugging)
if [ -d "$DODOT_DEPLOYED_DIR/symlink" ] && [ -n "$(ls -A "$DODOT_DEPLOYED_DIR/symlink" 2>/dev/null)" ]; then
    # Process each file in the directory
    for symlink_file in "$DODOT_DEPLOYED_DIR/symlink"/.* "$DODOT_DEPLOYED_DIR/symlink"/*; do
        # Skip . and .. entries
        case "$(basename "$symlink_file")" in
            "." | ".." | "*" | ".*") continue ;;
        esac
        
        # Skip if file doesn't exist (glob didn't match)
        [ -e "$symlink_file" ] || continue
        
        
        if [ -L "$symlink_file" ]; then
            # Track symlinks for debugging
            if [ -n "$DODOT_DEPLOYMENT_ROOT" ]; then
                # Get the actual target of the symlink
                target=$(readlink "$symlink_file")
                # Strip the deployment-time dotfiles root to get relative path
                relative_path="${target#$DODOT_DEPLOYMENT_ROOT/}"
                
                
                # Only add if we successfully got a relative path
                if [ "$relative_path" != "$target" ]; then
                    if [ -z "$DODOT_SYMLINKS" ]; then
                        DODOT_SYMLINKS="$relative_path"
                    else
                        DODOT_SYMLINKS="$DODOT_SYMLINKS:$relative_path"
                    fi
                fi
            fi
        fi
    done
fi

# Export symlinks if any exist
if [ -n "$DODOT_SYMLINKS" ]; then
    export DODOT_SYMLINKS
fi

# 5. Track run-once handlers (provision scripts and brewfiles)
# Check provision script sentinels
if [ -d "$DODOT_DATA_DIR/provision/sentinels" ]; then
    for sentinel in "$DODOT_DATA_DIR/provision/sentinels"/*; do
        if [ -f "$sentinel" ]; then
            pack=$(basename "$sentinel")
            if [ -z "$DODOT_PROVISION_SCRIPTS" ]; then
                DODOT_PROVISION_SCRIPTS="$pack"
            else
                DODOT_PROVISION_SCRIPTS="$DODOT_PROVISION_SCRIPTS:$pack"
            fi
        fi
    done
fi

# Export provision scripts if any were completed
if [ -n "$DODOT_PROVISION_SCRIPTS" ]; then
    export DODOT_PROVISION_SCRIPTS
fi

# Check homebrew sentinels
if [ -d "$DODOT_DATA_DIR/homebrew" ]; then
    for sentinel in "$DODOT_DATA_DIR/homebrew"/*; do
        if [ -f "$sentinel" ]; then
            pack=$(basename "$sentinel")
            if [ -z "$DODOT_BREWFILES" ]; then
                DODOT_BREWFILES="$pack"
            else
                DODOT_BREWFILES="$DODOT_BREWFILES:$pack"
            fi
        fi
    done
fi

# Export brewfiles if any were completed
if [ -n "$DODOT_BREWFILES" ]; then
    export DODOT_BREWFILES
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
        return 0 # Run if missing arguments
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
        return 1 # Already run with same checksum
    fi

    return 0 # Checksum changed, should run
}

# Add a helper function for debugging
dodot_status() {
    echo "dodot deployment status:"
    echo ""
    echo "Data directory: $DODOT_DATA_DIR"
    echo "Deployment root: ${DODOT_DEPLOYMENT_ROOT:-[not set]}"
    echo ""
    echo "Environment variables (relative paths from dotfiles root):"
    echo "  DODOT_SYMLINKS: ${DODOT_SYMLINKS:-[none]}"
    echo "  DODOT_SHELL_PROFILES: ${DODOT_SHELL_PROFILES:-[none]}"
    echo "  DODOT_PATH_DIRS: ${DODOT_PATH_DIRS:-[none]}"
    echo "  DODOT_SHELL_SOURCES: ${DODOT_SHELL_SOURCES:-[none]}"
    echo "  DODOT_INSTALL_SCRIPTS: ${DODOT_INSTALL_SCRIPTS:-[none]}"
    echo "  DODOT_BREWFILES: ${DODOT_BREWFILES:-[none]}"
    echo ""

    if [ -d "$DODOT_DEPLOYED_DIR" ]; then
        echo "Deployed items:"

        # Shell profiles
        if [ -d "$DODOT_DEPLOYED_DIR/shell" ] && [ -n "$(ls -A "$DODOT_DEPLOYED_DIR/shell" 2>/dev/null)" ]; then
            echo "  Shell profiles:"
            for item in "$DODOT_DEPLOYED_DIR/shell"/*; do
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

# Helper function to display tracked items from environment variables
dodot_tracked() {
    echo "dodot tracked deployments (from environment variables):"
    echo ""
    
    if [ -n "$DODOT_SYMLINKS" ]; then
        echo "Symlinks:"
        echo "$DODOT_SYMLINKS" | tr ':' '\n' | sed 's/^/  /'
        echo ""
    fi
    
    if [ -n "$DODOT_SHELL_PROFILES" ]; then
        echo "Shell profiles:"
        echo "$DODOT_SHELL_PROFILES" | tr ':' '\n' | sed 's/^/  /'
        echo ""
    fi
    
    if [ -n "$DODOT_PATH_DIRS" ]; then
        echo "PATH additions:"
        echo "$DODOT_PATH_DIRS" | tr ':' '\n' | sed 's/^/  /'
        echo ""
    fi
    
    if [ -n "$DODOT_SHELL_SOURCES" ]; then
        echo "Shell sources:"
        echo "$DODOT_SHELL_SOURCES" | tr ':' '\n' | sed 's/^/  /'
        echo ""
    fi
    
    if [ -n "$DODOT_INSTALL_SCRIPTS" ]; then
        echo "Completed install scripts (packs):"
        echo "$DODOT_INSTALL_SCRIPTS" | tr ':' '\n' | sed 's/^/  /'
        echo ""
    fi
    
    if [ -n "$DODOT_BREWFILES" ]; then
        echo "Completed Brewfiles (packs):"
        echo "$DODOT_BREWFILES" | tr ':' '\n' | sed 's/^/  /'
        echo ""
    fi
    
    if [ -z "$DODOT_SYMLINKS" ] && [ -z "$DODOT_SHELL_PROFILES" ] && \
       [ -z "$DODOT_PATH_DIRS" ] && [ -z "$DODOT_SHELL_SOURCES" ] && \
       [ -z "$DODOT_INSTALL_SCRIPTS" ] && [ -z "$DODOT_BREWFILES" ]; then
        echo "No tracked deployments found."
    fi
}
