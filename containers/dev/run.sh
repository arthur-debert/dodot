#!/bin/bash
set -e

# Usage: ./run.sh [script_path] [script_args...]
#
# Examples:
#   ./run.sh                     # Interactive mode - drops into zsh shell
#   ./run.sh ./scripts/build     # Run build script and exit
#   ./run.sh ./scripts/test -v   # Run test script with args and exit
#   ./run.sh bash -c "go test"   # Run arbitrary command and exit

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Change to project root (two levels up from containers/dev)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_ROOT"

# Export user ID and group ID
export USER_UID=$(id -u)
export USER_GID=$(id -g)

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
    
    echo "Starting dodot development container in script mode..."
    echo "Executing: $SCRIPT_PATH $@"
    echo ""
    
    # Run the script and exit, ensuring direnv is loaded first
    docker-compose -f "$SCRIPT_DIR/docker-compose.yml" run --rm dodot-dev /bin/bash -c "cd /workspace && direnv allow && eval \"\$(direnv export bash)\" && $SCRIPT_PATH $*"
else
    # Interactive mode (default)
    echo "Starting dodot development container..."
    echo "You'll be dropped into the repository root at /workspace"
    echo ""
    
    # Run the container interactively
    docker-compose -f "$SCRIPT_DIR/docker-compose.yml" run --rm dodot-dev
fi