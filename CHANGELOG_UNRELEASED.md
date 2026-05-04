# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- **Conditional running (gates)** — files, directories, and packs can
  be gated against host facts (OS, arch, hostname, username) so the
  same dotfiles repo deploys differently on different machines without
  templating every file. Five surfaces:
  - **Filename suffix**: `install._darwin.sh`, `Brewfile._darwin`,
    `home.bashrc._darwin` — the `._<label>` token sits before the
    extension (or as a trailing segment for extensionless files) and
    strips at deploy time.
  - **Directory segment**: `_darwin/_home/.bashrc`, `_arm-mac/setup.sh`
    — gate dirs at the pack root expand transparently on a matching
    host; on a mismatch they surface in `dodot status` as `gated out
    (label=...)`. Routing-prefix tokens (`home`/`xdg`/`app`/`lib`) are
    excluded from gate parsing.
  - **Pack-level**: `[pack] os = ["darwin"]` short-circuits a whole
    pack on the wrong OS. Inactive packs render under their own
    "Inactive on this OS" section in `dodot status`.
  - **Glob escape hatch**: `[mappings.gates] "install-mac.sh" =
    "darwin"` for legacy repos that can't rename. Conflicts with
    filename gates on the same file are a hard error.
  - **`dodot adopt --only-os <label>`** — wraps the adopted entry in a
    `_<label>/` gate dir so the deployed symlink only lands on
    matching hosts.
  - User-defined labels via `[gates]`: `[gates] arm-mac = { os =
    "darwin", arch = "aarch64" }` — built-ins ship for OS and arch
    (`darwin`, `linux`, `macos`, `arm64`, `aarch64`, `x86_64`).
  - Gate evaluation runs before preprocessing, so secret-provider
    calls and template renders never fire for entries the user opted
    out of. See `docs/proposals/conditional-running.lex` for the full
    design and §8.8 for documented limitations.
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
