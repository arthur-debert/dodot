# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-14

### Added

- Core dotfiles management with pack-based organization
- Symlink handler with smart path resolution (home dotfiles, XDG config, custom targets)
- Install handler with content-based checksums for idempotent script execution
- Homebrew handler for Brewfile-driven installs
- Shell handler for sourcing shell configuration files
- Path handler for adding directories to PATH
- Commands: `up`, `down`, `status`, `list`, `init`, `adopt`, `fill`, `genconfig`, `addignore`, `init-sh`
- Pack discovery with `.dodotignore` support
- Configuration system: defaults, root `.dodot.toml`, per-pack `.dodot.toml` overrides
- `dot.` prefix convention for top-level pack files
- Per-file custom symlink target overrides
- Double-link datastore architecture for state tracking
- Shell init script generation (`eval "$(dodot init-sh)"`)
- Dry-run mode for all deployment commands
- JSON output mode for scripting
- Themed terminal output via standout
