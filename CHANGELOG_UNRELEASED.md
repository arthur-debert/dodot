# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Documentation

- Documented the cross-pack ordering contract: dodot processes packs in
  lexicographic order of their on-disk directory names, and that order
  determines shell init source order, `$PATH` entry order, and
  install/homebrew execution order. Added a "Cross-Pack Ordering"
  section to `docs/reference/handlers.lex` and a bootstrap-zone note to
  `docs/user/getting-started.lex` covering what belongs above the
  `eval "$(dodot init-sh)"` line. No behavior change.
