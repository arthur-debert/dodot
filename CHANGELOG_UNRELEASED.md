# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

### Changed

- Errors are now surfaced as indexed footnotes instead of being spliced into the per-file status column. Each item row stays one line with a short status label (`pending`, `error`, `stale`, …); long error bodies, stderr, and conflict reasons all render in a dedicated `Errors:` section at the bottom, referenced from the row by a `[N]` marker. Indices are command-wide, so the same scheme covers `status`, `up`, and `adopt` with one rendering path. Replaces the previous per-pack footnote mechanism and the "append a raw error row at the end of the pack" hack.

### Fixed

- `dodot up` now renders the full per-pack listing and notes section when a cross-pack conflict blocks deployment, matching what `dodot status` shows. Previously the CLI handler hardcoded an empty pack list on the cross-pack conflict branch, so users only saw the trailing conflicts dump and lost all context about what *would* have been deployed.
- Symlink deploys now refuse when an ancestor of the target path is a symlink resolving into `dotfiles_root` or `data_dir`. Writing through such an ancestor landed back inside the pack store — silently clobbering pack source files (top-level files built a pack↔data-dir cycle) or surfacing as a misleading `non-symlink file at target path` (pack directories). The check runs in both real and dry-run modes; `--force` does not bypass it. Relative ancestor targets like `~/.config/warp -> ../dotfiles/warp` are lexically normalized before the prefix comparison so they get caught too.
