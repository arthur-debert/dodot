#!/bin/bash

# Script to rename all test files to *_toremove_test.go

set -e

echo "Renaming test files to *_toremove_test.go..."

# Counter for renamed files
count=0

# Find all test files and rename them
while IFS= read -r file; do
    # Get the directory and filename
    dir=$(dirname "$file")
    basename=$(basename "$file")
    
    # Create new filename by replacing _test.go with _toremove_test.go
    new_name="${basename%_test.go}_toremove_test.go"
    new_path="$dir/$new_name"
    
    # Rename the file
    git mv "$file" "$new_path"
    echo "Renamed: $file -> $new_path"
    count=$((count + 1))
done < <(find pkg -name "*_test.go" -type f | grep -v "_toremove_test.go")

echo "Renamed $count test files"

# Also rename pkg/testutil to pkg/testutil_old
if [ -d "pkg/testutil" ]; then
    echo "Renaming pkg/testutil -> pkg/testutil_old"
    git mv pkg/testutil pkg/testutil_old
fi

echo "Done!"