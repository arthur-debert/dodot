#!/bin/bash
# Login shell initialization script for dodot development container

PATH="$PATH:/workspace/bin:/workspace/scripts"
# Set vi mode for shell
set -o vi

# Allow direnv silently
(cd /workspace && direnv allow) >/dev/null 2>&1

# Set up git user if environment variables are provided
if [ -n "$GIT_AUTHOR_NAME" ] && [ -n "$GIT_AUTHOR_EMAIL" ]; then
    git config --global user.name "$GIT_AUTHOR_NAME"
    git config --global user.email "$GIT_AUTHOR_EMAIL"
fi

# Configure git to use GitHub CLI for authentication
if command -v gh &>/dev/null && [ -n "$GITHUB_TOKEN" -o -n "$GH_TOKEN" ]; then
    git config --global credential.helper "!gh auth git-credential"
fi

# Ensure Homebrew is in PATH and configured
if [ -d "/home/linuxbrew/.linuxbrew" ]; then
    eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"
fi

# Check if dodot binary exists, build if not
if [ ! -f "/workspace/bin/dodot" ]; then
    echo "🔨 dodot binary not found, building..."
    cd /workspace && ./scripts/build
    echo ""
fi

# Welcome message
echo "=================================================="
echo "dodot welcomes yall"
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
