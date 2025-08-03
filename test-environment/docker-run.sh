#!/bin/bash
# Docker run script - Stable entry point
#
# IMPORTANT: This script should RARELY change. It sets up the proper mounts
# and runs the orchestrator. All logic is in the orchestrator and phase scripts.
#
# Key architecture decisions:
# 1. Mount entire repository (source code for building)
# 2. Mount test scripts directory
# 3. Mount sample dotfiles
# 4. Run orchestrator.sh which handles the three phases

set -euo pipefail

# Get the directory of this script
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

echo "Starting dodot Docker test environment..."
echo "Project root: $PROJECT_ROOT"
echo

# Run the container with all necessary mounts
# Note: In CI, the repository might be owned by a different user,
# so we run as root user to ensure permissions work
docker run --rm \
    --user root \
    -v "$PROJECT_ROOT:/dodot:rw" \
    -v "$SCRIPT_DIR:/test-environment:rw" \
    -v "$SCRIPT_DIR/scripts:/scripts:ro" \
    -v "$SCRIPT_DIR/orchestrator.sh:/orchestrator.sh:ro" \
    -v "$SCRIPT_DIR/sample-dotfiles:/dotfiles:rw" \
    -e DOTFILES_ROOT=/dotfiles \
    dodot-test \
    /bin/bash /orchestrator.sh