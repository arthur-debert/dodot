#!/bin/bash
# Mock dodot command for testing
# This simulates basic dodot behavior for integration tests

set -euo pipefail

# Log all calls for debugging
echo "[$(date -u +"%Y-%m-%d %H:%M:%S")] dodot $*" >> /tmp/dodot-calls.log

# Simple command parsing
CMD="${1:-}"
PACK="${2:-}"

case "$CMD" in
    "deploy")
        if [ -z "$PACK" ]; then
            echo "ERROR: Pack name required" >&2
            exit 1
        fi
        
        # Check if pack exists
        PACK_DIR="${DOTFILES_ROOT}/${PACK}"
        if [ ! -d "$PACK_DIR" ]; then
            echo "ERROR: Pack '$PACK' not found" >&2
            exit 1
        fi
        
        # Check for pack.dodot.toml
        if [ ! -f "$PACK_DIR/pack.dodot.toml" ]; then
            echo "ERROR: No pack.dodot.toml found in pack '$PACK'" >&2
            exit 1
        fi
        
        # Handle different packs
        if [ "$PACK" = "vim" ]; then
            # Simulate basic symlink creation for vim pack
            # Check for source files
            if [ -f "$PACK_DIR/.vimrc" ]; then
                # Check if target already exists
                if [ -e "$HOME/.vimrc" ] && [ ! -L "$HOME/.vimrc" ]; then
                    echo "ERROR: $HOME/.vimrc already exists and is not a symlink" >&2
                    exit 1
                fi
                
                # Check permissions
                if ! touch "$HOME/.test-write-permission" 2>/dev/null; then
                    echo "ERROR: Cannot write to $HOME" >&2
                    exit 1
                fi
                rm -f "$HOME/.test-write-permission"
                
                # Create deployed directory structure
                DEPLOYED_DIR="$HOME/.local/share/dodot/deployed/symlink"
                mkdir -p "$DEPLOYED_DIR"
                
                # Create symlink in deployed directory
                ln -sf "$PACK_DIR/.vimrc" "$DEPLOYED_DIR/.vimrc"
                
                # Create symlink in home
                ln -sf "$DEPLOYED_DIR/.vimrc" "$HOME/.vimrc"
            fi
            
            if [ -d "$PACK_DIR/.vim" ]; then
                # Check if target already exists
                if [ -e "$HOME/.vim" ] && [ ! -L "$HOME/.vim" ]; then
                    echo "ERROR: $HOME/.vim already exists and is not a symlink" >&2
                    exit 1
                fi
                
                # Create symlink in deployed directory
                ln -sf "$PACK_DIR/.vim" "$DEPLOYED_DIR/.vim"
                
                # Create symlink in home
                ln -sf "$DEPLOYED_DIR/.vim" "$HOME/.vim"
            fi
            
            # Handle .config/nvim if specified
            if [ -d "$PACK_DIR/.config/nvim" ]; then
                mkdir -p "$HOME/.config"
                if [ -e "$HOME/.config/nvim" ] && [ ! -L "$HOME/.config/nvim" ]; then
                    echo "ERROR: $HOME/.config/nvim already exists and is not a symlink" >&2
                    exit 1
                fi
                
                mkdir -p "$DEPLOYED_DIR/.config"
                ln -sf "$PACK_DIR/.config/nvim" "$DEPLOYED_DIR/.config/nvim"
                ln -sf "$DEPLOYED_DIR/.config/nvim" "$HOME/.config/nvim"
            fi
            
            # Handle .gvimrc if exists
            if [ -f "$PACK_DIR/.gvimrc" ]; then
                if [ -e "$HOME/.gvimrc" ] && [ ! -L "$HOME/.gvimrc" ]; then
                    echo "ERROR: $HOME/.gvimrc already exists and is not a symlink" >&2
                    exit 1
                fi
                
                ln -sf "$PACK_DIR/.gvimrc" "$DEPLOYED_DIR/.gvimrc"
                ln -sf "$DEPLOYED_DIR/.gvimrc" "$HOME/.gvimrc"
            fi
            
            # Check for non-existent files in pack.dodot.toml
            if grep -q '\.nonexistent' "$PACK_DIR/pack.dodot.toml"; then
                echo "ERROR: Source file .nonexistent not found" >&2
                exit 1
            fi
        elif [ "$PACK" = "bash" ]; then
            # Handle bash pack
            if [ -f "$PACK_DIR/.bashrc" ]; then
                # Check if target already exists
                if [ -e "$HOME/.bashrc" ] && [ ! -L "$HOME/.bashrc" ]; then
                    echo "ERROR: $HOME/.bashrc already exists and is not a symlink" >&2
                    exit 1
                fi
                
                # Check permissions
                if ! touch "$HOME/.test-write-permission" 2>/dev/null; then
                    echo "ERROR: Cannot write to $HOME" >&2
                    exit 1
                fi
                rm -f "$HOME/.test-write-permission"
                
                # Create deployed directory structure
                DEPLOYED_DIR="$HOME/.local/share/dodot/deployed/symlink"
                mkdir -p "$DEPLOYED_DIR"
                
                # Create symlink in deployed directory
                ln -sf "$PACK_DIR/.bashrc" "$DEPLOYED_DIR/.bashrc"
                
                # Create symlink in home
                ln -sf "$DEPLOYED_DIR/.bashrc" "$HOME/.bashrc"
            fi
            
            # Handle aliases.sh for shell_profile
            if [ -f "$PACK_DIR/aliases.sh" ]; then
                # Create shell_profile deployment directory
                SHELL_PROFILE_DIR="$HOME/.local/share/dodot/deployed/shell_profile"
                mkdir -p "$SHELL_PROFILE_DIR"
                
                # Create symlink with pack name
                ln -sf "$PACK_DIR/aliases.sh" "$SHELL_PROFILE_DIR/bash.sh"
            fi
        elif [ "$PACK" = "tools" ]; then
            # Handle tools pack with PATH
            if [ -d "$PACK_DIR/bin" ]; then
                # Create path deployment directory
                PATH_DIR="$HOME/.local/share/dodot/deployed/path"
                mkdir -p "$PATH_DIR"
                
                # Create symlink to bin directory
                ln -sf "$PACK_DIR/bin" "$PATH_DIR/tools"
            fi
        fi
        ;;
        
    "--version")
        echo "dodot mock version 0.0.1-test"
        ;;
        
    *)
        echo "ERROR: Unknown command: $CMD" >&2
        exit 1
        ;;
esac