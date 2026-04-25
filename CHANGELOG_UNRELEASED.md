# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- `mappings.exclude` config list: filenames matched here are surfaced in
  `dodot status` as `ignored` instead of being claimed by the catchall
  symlink handler. Defaults cover documentation/legal files (`README`,
  `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, `AUTHORS`, `NOTICE`, `COPYING`
  and their `.*` variants), matched case-insensitively. Override per-pack
  by setting `[mappings] exclude = []` (or a different list) in the
  pack's `.dodot.toml`.
