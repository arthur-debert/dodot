#!/bin/bash
set -e

# Usage: ./run-base.sh [script_path] [script_args...]
#
# This script runs commands in the base container (without AI tools)
# Used primarily for CI and test environments
#
# Examples:
#   ./run-base.sh                     # Interactive mode - drops into zsh shell
#   ./run-base.sh ./scripts/build     # Run build script and exit
#   ./run-base.sh ./scripts/test -v   # Run test script with args and exit
#   ./run-base.sh bash -c "go test"   # Run arbitrary command and exit

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Change to project root (two levels up from live-testing/scripts)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_ROOT"

# Export user ID and group ID (use existing values if set, otherwise use current user)
export USER_UID=${USER_UID:-$(id -u)}
export USER_GID=${USER_GID:-$(id -g)}

# Pass through git configuration if available
if git config --global user.name > /dev/null 2>&1; then
    export GIT_AUTHOR_NAME="$(git config --global user.name)"
    export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME"
fi

if git config --global user.email > /dev/null 2>&1; then
    export GIT_AUTHOR_EMAIL="$(git config --global user.email)"
    export GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
fi

# Check if a script path was provided
if [ $# -gt 0 ]; then
    # Script execution mode
    SCRIPT_PATH="$1"
    shift  # Remove first argument, pass the rest to the script
    
    echo "Starting dodot base container in script mode..." >&2
    echo "Executing: $SCRIPT_PATH $@" >&2
    echo "" >&2
    
    # Run the script and exit
    docker compose -f "$SCRIPT_DIR/../containers/docker-compose.yml" run --rm dodot-base /bin/bash -c "cd /workspace && $SCRIPT_PATH $*"
else
    # Interactive mode (default)
    echo "Starting dodot base container..."
    echo "You'll be dropped into the repository root at /workspace"
    echo ""
    
    # Run the container interactively
    docker compose -f "$SCRIPT_DIR/../containers/docker-compose.yml" run --rm dodot-base
fi