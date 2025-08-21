#!/bin/bash
# Cleanup script to remove any stale template test files
# This should be run inside the container before tests

echo "Cleaning up stale template test files..."

# Remove template test directories if they exist
rm -rf /workspace/live-testing/scenarios/suite-1-single-handlers/template
rm -rf /workspace/live-testing/scenarios/suite-4-single-handler-edge-cases/dotfiles/template-pack

# Remove any .tmpl files that shouldn't exist
find /workspace/live-testing -name "*.tmpl" -type f -delete 2>/dev/null || true

# Remove template assertions library if it exists
rm -f /workspace/live-testing/lib/assertions_template.sh

echo "Cleanup complete"