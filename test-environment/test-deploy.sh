#!/bin/bash
# Test dodot deployment functionality

echo "=== Testing Dodot Deployment ==="
echo

echo "1. Current environment:"
echo "   DOTFILES_ROOT: $DOTFILES_ROOT"
echo "   PWD: $(pwd)"
echo "   User: $(whoami)"
echo

echo "2. List available packs:"
dodot list
echo

echo "3. Deploy vim pack:"
dodot deploy vim
echo

echo "4. Check if symlink was created:"
if [ -L "$HOME/.vimrc" ]; then
    echo "   ✓ ~/.vimrc is a symlink"
    echo "   Target: $(readlink $HOME/.vimrc)"
else
    echo "   ✗ ~/.vimrc is not a symlink"
    echo "   Checking if file exists:"
    ls -la ~/.vimrc 2>&1 || echo "   File does not exist"
fi
echo

echo "5. Check .local/share/dodot directory:"
if [ -d "$HOME/.local/share/dodot" ]; then
    echo "   Found .local/share/dodot:"
    find $HOME/.local/share/dodot -type l -o -type f | head -10
else
    echo "   No .local/share/dodot directory found"
fi
echo

echo "6. Test status command:"
dodot status vim
echo

echo "7. Test deploying multiple packs:"
dodot deploy zsh git
echo

echo "8. List all created symlinks:"
echo "   Symlinks in home directory:"
find $HOME -maxdepth 1 -type l -name ".*" 2>/dev/null | sort