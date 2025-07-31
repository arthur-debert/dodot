#!/bin/bash
# Quick test to verify Docker environment is working
set -euo pipefail

echo "=== Dodot Integration Test Environment ==="
echo

echo "1. Checking environment..."
echo "   DOTFILES_ROOT: $DOTFILES_ROOT"
echo "   Shell: $(echo $SHELL)"
echo "   User: $(whoami)"
echo

echo "2. Checking Homebrew..."
if command -v brew &> /dev/null; then
    echo "   ✓ Homebrew installed: $(brew --version | head -1)"
else
    echo "   ✗ Homebrew not found!"
fi
echo

echo "3. Checking dodot..."
if command -v dodot &> /dev/null; then
    echo "   ✓ dodot installed: $(dodot --version)"
else
    echo "   ✗ dodot not found!"
    echo "   Note: Mount the binary with: -v ../bin/dodot:/usr/local/bin/dodot"
fi
echo

echo "4. Checking dotfiles..."
echo "   Packs available:"
if [ -d "$DOTFILES_ROOT" ]; then
    for pack in $DOTFILES_ROOT/*/pack.dodot.toml; do
        if [ -f "$pack" ]; then
            packname=$(basename $(dirname "$pack"))
            echo "   - $packname"
        fi
    done
else
    echo "   ✗ DOTFILES_ROOT not found!"
fi
echo

echo "Setup verification complete!"