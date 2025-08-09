#!/bin/bash
# Convenience script to run dodot live system tests in container

set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Running dodot live system tests..."
echo ""

# First build dodot in the container
echo "Building dodot..."
"$SCRIPT_DIR/run.sh" ./scripts/build

echo ""
echo "Running tests..."
# Run the test suite
"$SCRIPT_DIR/run.sh" /workspace/test-data/runner.sh