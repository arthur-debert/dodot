# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- `[mappings] ignore` and `[mappings] skip` config lists, backed by two
  new filter handlers in a dedicated execution phase that runs before
  every other phase:
  - `ignore` (default `[]`): silently drops matching files, mirroring
    `.gitignore` — nothing surfaces in `dodot status`.
  - `skip` (defaults: `README`, `LICENSE`, `CHANGELOG`, `CONTRIBUTING`,
    `AUTHORS`, `NOTICE`, `COPYING` and their `.*` variants, matched
    case-insensitively): listed in `dodot status` as `skipped`, but no
    handler runs on them. Override per-pack with `[mappings] skip = []`
    to deploy a README intentionally.
  Filter handlers are real registered handlers (no synthetic-name
  dispatch) and win over precise mappings and the catchall via ordinary
  rule priority. The pack-level `.dodotignore` marker is unchanged but
  is now referred to as the "pack-ignore" mechanism in docs.
