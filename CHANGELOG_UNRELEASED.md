# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- **macOS paths: `_app/`, `_lib/`, `force_app`, `app_aliases`.** Dodot
  now models `~/Library/Application Support/<App>/` as a third
  filesystem coordinate alongside `$HOME` and `$XDG_CONFIG_HOME`, so
  cross-platform packs can deploy GUI-app config correctly on both
  macOS and Linux without `if os == "darwin"` branching inside packs.
  The full design lives in `docs/proposals/macos-paths.lex`; user-facing
  pieces (Phase M1–M5) shipping in this release:
    - **`_app/<name>/<rest>` directory prefix** — deploys raw under
      `<app_support_dir>/<name>/<rest>`. On macOS that's
      `~/Library/Application Support`; on Linux it collapses to
      `$XDG_CONFIG_HOME` so the same pack tree works on both. New
      Priority 2c in the symlink resolver.
    - **`_lib/<rest>` directory prefix (macOS only)** — deploys to
      `$HOME/Library/<rest>` for non-Application-Support Library
      subtrees (`LaunchAgents/`, `Fonts/`, `Services/`). On
      non-macOS platforms emits a soft warning and skips with no
      symlink. New Priority 2d.
    - **`[symlink] force_app`** — curated list of GUI-app folder
      names (case-sensitive, capped at 100) whose first path segment
      routes to `<app_support_dir>/<name>/<rest>` without a `_app/`
      prefix. Ships seeded with `Code`, `Cursor`, `Zed`, `Emacs`.
      New Priority 4.
    - **`[symlink.app_aliases]` table** — pack-name → app-folder-name
      rewrites. A pack named `vscode` aliased to `Code` deploys to
      `<app_support_dir>/Code/...` so the pack name stays
      lowercase-ergonomic. New Priority 5; modifies the default rule
      only — explicit prefixes still win.
    - **`[symlink] app_uses_library`** (default `true` on macOS,
      ignored elsewhere) — set to `false` on macOS to opt the entire
      pack tree into Linux-style `~/.config` placement.
    - **`dodot adopt ~/Library/Application Support/<X>/...`** —
      AppSupport sources now infer pack `<X>` and produce
      `_app/<X>/<rest>` in-pack paths that round-trip back to the
      same deployed location. Pack-root directory expansion works
      the same as for XDG.
    - **Capitalization heuristic for adopt suggestions** — when an
      AppSupport adopt's inferred pack name passes the GUI-app
      heuristic (uppercase / space / reverse-DNS shape), adopt
      surfaces an advisory tip pointing at the `app_aliases`
      ergonomic. Purely advisory; the resolver and pack tree are
      unaffected. See `docs/proposals/macos-paths.lex` §8.1.

  Phase M6 (homebrew-cask probing, `dodot probe app` subcommand) is
  intentionally deferred to a separate PR.
