# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- `[mappings] ignore` config list. Files matching any glob in this list
  are dropped silently — same contract as `.gitignore`, nothing
  surfaces in `dodot status`. Default is empty; common build / VCS
  clutter is already covered by `[pack] ignore` (which stops discovery
  one layer earlier).
- New `Filter` execution phase, running before every deploying phase
  (`Provision`, `Setup`, `PathExport`, `ShellInit`, `Link`). The
  `ignore` and `skip` handlers live here; matched files are dropped
  before any deploying handler can claim them.
- `Rule.case_insensitive` flag, used by `skip`'s defaults so common
  documentation casings (`README`, `Readme`, `readme`) all match the
  same rule.

### Changed

- `[mappings] skip` is now a real registered filter handler instead of
  an `!<pattern>` exclusion rule. Three user-visible consequences:
  - Files matched by `skip` surface in `dodot status` as `skipped`
    (previously dropped silently like `.gitignore`).
  - Default value is no longer `[]`; ships with the common
    documentation/legal patterns (`README`, `README.*`, `LICENSE`,
    `LICENSE.*`, `CHANGELOG`, `CHANGELOG.*`, `CONTRIBUTING`,
    `CONTRIBUTING.*`, `AUTHORS`, `AUTHORS.*`, `NOTICE`, `NOTICE.*`,
    `COPYING`, `COPYING.*`), matched case-insensitively. Override
    per-pack with `skip = []` to deploy a `README` intentionally.
  - For the older silent-drop semantics, use `[mappings] ignore`
    instead.
- Symlink catchall no longer claims `README`-, `LICENSE`-like files;
  they are now claimed by the `skip` filter handler instead, which
  surfaces them in status rather than depositing them at
  `~/.config/<pack>/README.md`.
- Pack-level `.dodotignore` marker is unchanged but is now referred to
  as the "pack-ignore" mechanism in docs, to disambiguate from the
  intra-pack `[mappings] ignore` filter handler.
