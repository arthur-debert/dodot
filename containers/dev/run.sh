#!/bin/bash
set -e

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

echo "Starting dodot development container..."
echo "You'll be dropped into the repository root at /workspace"
echo ""

# Run the container using the compose file in containers/dev
docker-compose -f "$SCRIPT_DIR/docker-compose.yml" run --rm dodot-dev