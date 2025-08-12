#!/bin/bash
# Cleanup script to remove any stale template test files
# This should be run inside the container before tests

echo "Cleaning up stale template test files..."

# Remove template test directories if they exist
rm -rf /workspace/test-data/scenarios/suite-1-single-powerups/template
rm -rf /workspace/test-data/scenarios/suite-4-single-powerup-edge-cases/dotfiles/template-pack

# Remove any .tmpl files that shouldn't exist
find /workspace/test-data -name "*.tmpl" -type f -delete 2>/dev/null || true

# Remove template assertions library if it exists
rm -f /workspace/test-data/lib/assertions_template.sh

echo "Cleanup complete"