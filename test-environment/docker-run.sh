#!/bin/bash
# Helper script to run commands in the dodot test container

# Default to running bash if no command provided
COMMAND="${@:-/bin/bash}"

# Run the docker container with all necessary mounts
docker run --rm \
    -v "$(pwd)/../bin/dodot-linux:/usr/local/bin/dodot:ro" \
    -v "$(pwd)/sample-dotfiles:/dotfiles:rw" \
    -v "$(pwd)/test-deploy.sh:/test-deploy.sh:ro" \
    -v "$(pwd)/test-debug.sh:/test-debug.sh:ro" \
    -e DOTFILES_ROOT=/dotfiles \
    dodot-test \
    $COMMAND