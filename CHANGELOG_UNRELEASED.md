# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- **Pack ordering: numeric-prefix grammar.** Pack directories matching
  `^(\d+)[-_](.+)$` (e.g. `010-brew`, `100_zsh`, `900-starship`) now
  have their prefix recognised as ordering metadata. The full
  directory name remains the sort key (so prefixed packs apply in
  lex order), but the stem after the separator is treated as the
  pack's *display name* — what `dodot status`, `dodot list`, error
  messages, generated shell-init comments, and log lines all use.
  CLI arguments resolve against the display name first
  (`dodot up brew`) and fall back to the raw on-disk directory name
  (`dodot up 010-brew`). Symlink targets follow the display name too,
  so `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`. Three
  classes of collision are rejected at scan time with both offending
  paths in the error: logical-name (`nvim` + `010-nvim`),
  multi-prefix (`010-nvim` + `020-nvim`), and empty-stem (`010-` /
  `010_`). The internal datastore subtree continues to be keyed by
  the raw on-disk directory name.

### Changed

- Documented the cross-pack ordering contract: dodot processes packs in
  lexicographic order of their on-disk directory names, and that order
  determines shell init source order, `$PATH` entry order, and
  install/homebrew execution order. Added a "Cross-Pack Ordering"
  section to `docs/reference/handlers.lex` and a bootstrap-zone note to
  `docs/user/getting-started.lex` covering what belongs above the
  `eval "$(dodot init-sh)"` line.
- `dodot status`, `dodot list`, `dodot up`, and `dodot down` output
  now displays the stripped form for prefixed packs (a pack on disk
  as `010-nvim` shows as `nvim`). Scripts that referenced the raw
  form continue to work via the lookup fallback.
