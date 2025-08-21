# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This repository contains dodot, a stateless dotfiles manager written in Go.

**IMPORTANT**: Follow the developer guidelines in `docs/dev/README.txt` for all development work.

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

**CRITICAL**: Read `docs/dev/README.txt` for comprehensive development standards. Key points:

1. **Documentation**: ALL docs must use txxt format, never Markdown
2. **Code Quality**: Pre-commit hooks are MANDATORY (scripts/lint and scripts/test)
3. **Logging**: Required for all new code (see pkg/logging/logging.go)
4. **Error Handling**: All errors must have codes and messages
5. **File System**: NO direct FS operations - only through synthfs
6. **Testing**: Use common test helpers in pkg/testutil
7. **Commits**: Use "Release X.Y.Z:" prefix when working on releases

## Architecture Overview

### Package Organization
- `cmd/dodot/` - CLI entry point (thin Cobra-based layer)
- `pkg/core/` - Core pipeline functions (GetPacks, GetFiringTriggers, GetActions, GetFsOps)
- `pkg/types/` - Type definitions and interfaces
- `pkg/triggers/` - Trigger implementations (FileNameTrigger, DirectoryTrigger, etc.)
- `pkg/handlers/` - Handler implementations (SymlinkHandler, ProfileHandler, etc.)
- `pkg/matchers/` - Matcher system connecting triggers to handlers
- `pkg/logging/` - Centralized zerolog-based logging with verbosity control
- `pkg/errors/` - Error types with codes for stable testing
- `pkg/registry/` - Generic registry for extensibility

### dodot Core Concepts
dodot is a stateless dotfiles manager built around four key components:

1. **Triggers** - Pattern-matching engines that scan files and directories within packs
2. **Matchers** - Configuration objects that bind triggers to power-ups
3. **Power-ups** - Action generators that process matched files (symlink, brew, shell_profile, etc.)
4. **Actions** - High-level descriptions of operations to be performed

### Key Design Principles
- **No state management** - deployments are idempotent
- **Heavy use of symlinks** - for live editing of configs
- **Files under source control ARE the deployed files** - no copying
- **Organized in "packs"** - directories under DOTFILES_ROOT
- **File system safety** - all operations go through synthfs
- **Type safety** - leverage Go's type system throughout

### Build System
- GoReleaser handles multi-platform builds (Linux/macOS/Windows, amd64/arm64)
- Version info embedded via ldflags: `-X main.version={{.Version}} -X main.commit={{.Commit}}`
- Homebrew formula generation for macOS distribution
- Debian package generation for Linux distribution

### Key Dependencies
- `github.com/spf13/cobra` - CLI framework
- `github.com/rs/zerolog` - Structured logging
- `github.com/knadh/koanf` - Configuration management
- `github.com/arthur-debert/go-synthfs` - Safe filesystem operations
- Go 1.23+ required

### Testing Approach
- Use standard Go testing with race detection enabled
- Coverage reports uploaded to Codecov
- Run single tests: `go test -v -run TestName ./pkg/...`
- Table-driven tests preferred for comprehensive coverage
- Minimal E2E tests (most testing should use mocks)

### CI/CD Pipeline
- GitHub Actions workflow on push/PR: build, test, coverage
- Release workflow triggered by version tags (v*)
- Automated multi-platform builds and GitHub releases via GoReleaser

## Important Context

- **Documentation Format**: ALL project docs use `.txxt` format (see docs/dev/txxt-primer.txxt)
- **Design Docs**: See `docs/design/` for architecture decisions:
  - `simpler.txxt` - Overall design philosophy
  - `execution.txxt` - Implementation roadmap and phases
- **Error Codes**: All errors must have codes (not just messages) for stable testing
- **Logging Levels**: Default WARN, use -v (INFO), -vv (DEBUG), -vvv (TRACE)
- **Temp Files**: Use PROJECT_ROOT/tmp/ for all test artifacts