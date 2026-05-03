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
- `Filter` execution phase, running before every deploying phase
  (`Provision`, `Setup`, `PathExport`, `ShellInit`, `Link`). The
  `ignore` and `skip` handlers live here; matched files are dropped
  before any deploying handler can claim them.
- `bb88003` — symlink catchall now excludes `README`-, `LICENSE`-like
  files at the rules layer (folded into the new `skip` handler).

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
- Pack-level `.dodotignore` marker is unchanged but is now referred to
  as the "pack-ignore" mechanism in docs, to disambiguate from the
  intra-pack `[mappings] ignore` filter handler.

### Performance

- `74ea3a5` — per-file basename-lowercase computation is now lazy: it
  only happens when at least one rule has `case_insensitive = true`.
  The default rule set has no such rules, so common-case scanning pays
  nothing for the new filter-handler infrastructure.
