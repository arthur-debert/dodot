# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This repository contains dodot, a stateless dotfiles manager written in Go.

**IMPORTANT**: Follow the developer guidelines in `docs/dev/` for all development work. Start with `00_getting-started.txxt`.

## Common Development Commands

### Build & Development
- `./scripts/build` - Builds the CLI binary with embedded version info to `./bin/dodot`
- `./scripts/test` - Runs tests with race detection
- `./scripts/test-with-cov` - Runs tests with detailed coverage report and HTML output
- `./scripts/lint` - Runs golangci-lint (auto-installs if needed)
- `./scripts/pre-commit install` - Sets up git hooks for code quality

### Release & Distribution
- `./scripts/release-new --patch|--minor|--major` - Creates a new semantic version release
- `goreleaser release --clean` - Builds multi-platform releases (triggered by version tags)

### Running the CLI
- `./bin/dodot --version` - Shows version info
- `./bin/dodot -v/-vv/-vvv` - Increases logging verbosity

## Development Guidelines

 - docs/dev/development.txxt
 - docs/dev/testing.txxt
