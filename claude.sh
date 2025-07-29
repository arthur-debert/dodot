#!/bin/bash

# claude.sh - Run claude with predefined allowed tools
# This script allows git, gh, find, grep, cat, touch, mkdir, go, and scripts/* commands

claude --allowedTools "Bash(git:*) Bash(gh:*) Bash(find:*) Bash(grep:*) Bash(cat:*) Bash(touch:*) Bash(mkdir:*) Bash(go:*) Bash(scripts/*) Edit" "$@"
