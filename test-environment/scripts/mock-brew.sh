#!/bin/bash
# Mock brew command for testing Brewfile powerup
# Logs all calls to /tmp/brew-calls.log

BREW_LOG="/tmp/brew-calls.log"

# Create log entry with timestamp
echo "[$(date -u +"%Y-%m-%d %H:%M:%S")] brew $*" >> "$BREW_LOG"

# Handle different brew commands
case "$1" in
    "bundle")
        if [[ "$2" == "--file" ]] && [[ -n "$3" ]]; then
            echo "==> Parsing Brewfile: $3"
            if [[ ! -f "$3" ]]; then
                echo "Error: Brewfile not found: $3" >&2
                exit 1
            fi
            # Log the Brewfile contents
            echo "==> Installing from Brewfile..."
            while IFS= read -r line; do
                # Skip comments and empty lines
                [[ "$line" =~ ^#.*$ ]] || [[ -z "$line" ]] && continue
                echo "  - Processing: $line"
                echo "  [Brewfile] $line" >> "$BREW_LOG"
            done < "$3"
            echo "==> Brewfile installation complete"
        else
            echo "Usage: brew bundle --file <Brewfile>"
            exit 1
        fi
        ;;
    "install")
        shift
        for formula in "$@"; do
            echo "==> Installing $formula..."
            # Simulate some known failures
            if [[ "$formula" == "nonexistent-formula" ]]; then
                echo "Error: No available formula with the name \"$formula\"" >&2
                exit 1
            fi
            echo "  ✓ $formula installed successfully"
        done
        ;;
    "list")
        echo "git"
        echo "vim"
        echo "tmux"
        ;;
    "--version")
        echo "Homebrew 4.0.0 (mock)"
        echo "Homebrew/homebrew-core (git revision mock)"
        ;;
    *)
        echo "Mock brew: Command '$1' logged but not implemented"
        # Still log it but don't fail
        ;;
esac

exit 0