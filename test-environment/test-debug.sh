#!/bin/bash
# Debug dodot operations

echo "=== Debug Dodot Operations ==="
echo

echo "1. Clean start - remove any existing files:"
rm -f ~/.vimrc ~/.zshrc ~/.gitconfig
echo "   Cleaned"
echo

echo "2. Deploy vim with maximum verbosity:"
dodot deploy vim -vvv
echo

echo "3. Check filesystem operations log:"
echo "   Looking for any created files in home:"
find $HOME -type f -o -type l -name ".*" -mmin -5 2>/dev/null | grep -v ".cache\|.bash\|.profile\|.zprofile\|.zshrc" | head -20
echo

echo "4. Check if operations are dry-run by default:"
dodot deploy ssh --dry-run > /tmp/dryrun.out 2>&1
dodot deploy ssh > /tmp/normal.out 2>&1
echo "   Dry run output lines: $(wc -l < /tmp/dryrun.out)"
echo "   Normal output lines: $(wc -l < /tmp/normal.out)"
diff /tmp/dryrun.out /tmp/normal.out || echo "   Outputs differ"
echo

echo "5. Try creating a simple symlink manually to verify filesystem:"
touch /tmp/test-file
ln -s /tmp/test-file ~/test-symlink
if [ -L ~/test-symlink ]; then
    echo "   ✓ Manual symlink creation works"
    rm ~/test-symlink
else
    echo "   ✗ Manual symlink creation failed"
fi