# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.14.0] - 2026-04-22

### Added
- **Cross-pack conflict detection** (#29): `dodot up` now collects intents from all packs before executing any, detects when multiple packs produce symlinks targeting the same resolved path, and halts with a clear error listing conflicting packs, handlers, and source files — no partial deployment occurs
- `dodot status` surfaces potential cross-pack conflicts as warnings, even for packs that aren't deployed yet
- Symlink target collisions detected across all resolution layers: `[symlink.targets]`, `_home/` prefix, `dot.` convention, `force_home`, XDG defaults
- PATH executable shadowing detected: two packs with `bin/` directories containing same-named files are flagged (one would shadow the other in `$PATH`)
- **Auto-executable permissions**: `dodot up` now automatically adds `+x` to files in path-handler directories (`bin/`), matching user intent that these files should be runnable. Controlled by `[path] auto_chmod_exec` (default: `true`). Already-executable files are left untouched; permission failures are reported as warnings, not hard errors.
- New `CrossPackConflict` error variant with structured conflict data
- `dodot status` now lists directories skipped via `.dodotignore` under an "Ignored Packs" heading, so users aren't baffled when a pack-shaped directory doesn't appear in the main listing

### Changed
- **`mappings.shell` default now includes `env.sh`** alongside `aliases.sh`, `profile.sh`, `login.sh`. Files named `env.sh` in any pack are now claimed by the shell handler (sourced at shell init) instead of falling through to the symlink handler (which previously dropped them at `~/.env.sh` and collided across packs)
- `dodot up` now uses a two-phase execution model: collect all intents first, then execute — replacing the previous sequential per-pack execution
- `--force` does not override cross-pack conflicts (it only applies to pre-existing non-dodot files); cross-pack conflicts require a configuration fix
- Orchestration pipeline split into `collect_pack_intents()` and `execute_intents()` for composability
- **Scanner is top-level only** (#37): rules match pack depth-1 entries only; nested files are the responsibility of the handler that owns the containing top-level entry. Fixes two long-standing issues:
  - A pack's `bin/tool` was being **both** staged via the path handler *and* symlinked individually via the catchall — now only the `bin/` directory is claimed (by path). One shell command, one status line.
  - A nested `foo/install.sh` used to trigger the install handler because matching was on basename-only; now only a top-level `install.sh` does.
- **Symlink handler links top-level directories wholesale**: `warp/themes/` now becomes a single symlink `~/.config/themes → <pack>/warp/themes`, not a per-file listing. Falls back to per-file mode (current behavior) when `[symlink] protected_paths` or `[symlink.targets]` reach inside the directory, preserving every existing security guarantee.
- **Top-level dirs default to `$XDG_CONFIG_HOME/<name>`** (aligning code with the longstanding docs). Top-level *files* still default to `$HOME/.<name>`. `force_home`, `_home/`, `_xdg/`, `dot.`, and `targets` overrides all still apply.
- Handler trait gains `match_mode()` (`Precise` / `Catchall`) and `scope()` (`Exclusive` / `Shared`), with a registry invariant that at most one handler may be simultaneously `Catchall` + `Exclusive`. No behavior change for existing handlers; the formalization future-proofs adding non-claiming observers.
- `Handler::to_intents` now receives an `&dyn Fs` so handlers can inspect directory contents when deciding wholesale-vs-per-file treatment.

### Migration notes
- If you previously relied on nested files being individually symlinked (e.g. `warp/themes/nord.yaml → ~/.config/themes/nord.yaml`), the whole `themes/` directory is now one symlink. The observed path `~/.config/themes/nord.yaml` still resolves identically via the directory link.
- If you need nested per-file behavior (different targets per file, or selective inclusion), add `[symlink.targets]` entries or list individual files in `[symlink] protected_paths` — either triggers per-file mode for the containing directory.
- If you had nested `install.sh` / `aliases.sh` / `Brewfile` that were (perhaps unintentionally) being picked up by their handlers, move them to the pack's top level or use `[mappings]` overrides.

## [0.9.3] - 2026-04-16

### Added
- Structured logging via `tracing` with daily-rotating file output to `~/.cache/dodot/logs/`
- `--verbose` flag: show INFO-level log messages on stderr
- `--debug` flag: show DEBUG-level log messages on stderr
- INFO and DEBUG events across orchestration pipeline and executor subsystems
- Automatic cleanup of log files older than 7 days

## [0.9.2] - 2026-04-16

### Changed
- CI: force Node.js 24 for all GitHub Actions (future-proofing for June 2026 deprecation)
- CI: replace manual `actions/cache` with `Swatinem/rust-cache` for smarter Rust caching
- CI: e2e tests now download pre-built binary from check job instead of rebuilding from source

## [0.9.1] - 2026-04-15

### Fixed
- Release workflow: add MIT license and crate metadata required by crates.io
- Release workflow: macOS signing failures no longer block binary packaging/upload
- Release workflow: fix cross-compilation install on runners with pre-existing `cross` binary

## [0.9.0] - 2026-04-15

### Added
- `[pack] ignore` now ships with sensible compiled defaults (`.git`, `.svn`, `.DS_Store`, `*.swp`, etc.) — previously the default was an empty list
- Exhaustive unit test verifying all compiled default values for `pack.ignore`, `symlink.force_home`, `symlink.protected_paths`, and all `mappings` fields

### Changed
- **BREAKING:** `[mappings] ignore` renamed to `[mappings] skip` to disambiguate from `[pack] ignore`
- Removed `genconfig` command — use `dodot config gen` (via clapfig) instead, which auto-generates a commented TOML template from struct definitions

### Fixed
- Config docs: removed phantom "App Defaults" and "App Config" layers that never existed in code; documented the actual 3-layer hierarchy (compiled defaults → root `.dodot.toml` → pack `.dodot.toml`)
- Config docs: corrected merge semantics from "arrays append" to "arrays override (last value wins)" matching actual clapfig behavior
- Config docs: fixed `[symlink]` targets syntax from incorrect bare keys to correct `[symlink.targets]` table form
- Synced `force_home` defaults in docs (was 6 entries, now all 10 matching code)
- Synced `protected_paths` defaults across both doc files and code (was inconsistent between `config-system.lex` and `symlink-paths.lex`)

## [0.1.0] - 2026-04-14

### Added

- Core dotfiles management with pack-based organization
- Symlink handler with smart path resolution (home dotfiles, XDG config, custom targets)
- Install handler with content-based checksums for idempotent script execution
- Homebrew handler for Brewfile-driven installs
- Shell handler for sourcing shell configuration files
- Path handler for adding directories to PATH
- Commands: `up`, `down`, `status`, `list`, `init`, `adopt`, `fill`, `addignore`, `init-sh`, `config`
- Pack discovery with `.dodotignore` support
- Configuration system: defaults, root `.dodot.toml`, per-pack `.dodot.toml` overrides
- `dot.` prefix convention for top-level pack files
- Per-file custom symlink target overrides
- Double-link datastore architecture for state tracking
- Shell init script generation (`eval "$(dodot init-sh)"`)
- Dry-run mode for all deployment commands
- JSON output mode for scripting
- Themed terminal output via standout
