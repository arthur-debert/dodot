#!/bin/bash
# Login shell initialization script for dodot development container

# Set vi mode for shell
set -o vi

# Allow direnv
direnv allow

# Set up git user if environment variables are provided
if [ -n "$GIT_AUTHOR_NAME" ] && [ -n "$GIT_AUTHOR_EMAIL" ]; then
    git config --global user.name "$GIT_AUTHOR_NAME"
    git config --global user.email "$GIT_AUTHOR_EMAIL"
    echo "✓ Git user configured: $GIT_AUTHOR_NAME <$GIT_AUTHOR_EMAIL>"
fi

# Welcome message
echo "=================================================="
echo "Welcome to the dodot development container!"
echo "=================================================="
echo ""
echo "Go version: $(go version)"
echo "Working directory: $(pwd)"
echo ""
echo "Available commands:"
echo "  ./scripts/build       - Build dodot"
echo "  ./scripts/lint        - Run linting"
echo "  ./scripts/test        - Run tests"
echo "  ./scripts/pre-commit  - Run pre-commit checks"
echo "  goreleaser build      - Build with goreleaser"
echo ""
echo "=================================================="

# Note: This script is sourced, not executed, so no need to exec zsh
