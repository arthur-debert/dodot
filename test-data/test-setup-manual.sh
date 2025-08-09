#!/usr/bin/env bash
# Manual test of setup and debug functions

# Source the libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/debug.sh

echo "Testing setup and debug functions..."
echo ""

# Ensure dodot is built
ensure_dodot_built

# Set up test environment
setup_test_env "/workspace/test-data/scenarios/basic"

# Show debug state
debug_state

# Run a simple dodot command
echo ""
echo "=== RUNNING: dodot list ==="
dodot list

# Clean up
clean_test_env

echo ""
echo "Test complete!"