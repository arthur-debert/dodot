# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- **`dodot adopt` recognises `~/Library/...` sources (macOS).** Adopting
  a path like `~/Library/Preferences/com.colliderli.iina.plist` now
  produces `<pack>/_lib/Preferences/com.colliderli.iina.plist` so the
  existing Priority 2d `_lib/` resolver round-trips the file back on
  `dodot up`. `--into <pack>` is required because plist filenames
  (typically reverse-DNS bundle IDs) don't make useful pack names.
  Same inference applies to `~/Library/LaunchAgents/...`,
  `~/Library/Fonts/...`, etc. — anything under `~/Library/` not nested
  in `Application Support` (which still routes through the more
  specific `_app/` encoding) or `Containers` (still refused as a
  sandboxed-app data store). Gated on macOS at inference time so
  non-macOS adopt declines instead of producing warn-and-skip plans.
- **Adopt prints a filter-install tip when adopting plists.** When at
  least one adopted source is a `.plist` and `dodot git-install-filters`
  has not been run, `dodot adopt` surfaces a one-liner pointing at the
  install command, complementing the up-time interactive prompt.
- **`dodot git-install-filters` / `dodot git-show-filters`.** P2 of the
  plist clean/smudge track. `git-install-filters` writes the
  `[filter "dodot-plist"]` block to the dotfiles repo's `.git/config`
  so `git status` / `git diff` / `git add` invoke `dodot plist clean`
  and `smudge` automatically on tracked `*.plist` files. Idempotent;
  per-clone, per-machine. `git-show-filters` prints the same snippet
  (plus the `.gitattributes` line) without writing, for inspection or
  manual install.
- **Up-time filter-install prompt.** On the first `dodot up` of a pack
  containing `*.plist` files, dodot now offers to install the filters
  if they are not already registered. Three responses: `Y` installs
  and dismisses the prompt; `n` skips (asks again next time); `show`
  prints the config snippets without committing. The prompt fires
  only on a TTY — CI runs and scripted invocations are unaffected.
- **Generic prompt registry — `dodot prompts list/reset`.** A new
  content-agnostic registry tracks "have I shown the user X yet?"
  state (currently powering the plist filter-install prompt; future
  callers slot in by picking a key). Persisted at
  `<XDG_DATA_HOME>/dodot/prompts.json`. New CLI verbs:
    - `dodot prompts list` — show every known prompt with its
      dismissed/active state and a one-line description.
    - `dodot prompts reset <key>` — clear one dismissal so the
      prompt fires again next time.
    - `dodot prompts reset --all` — clear every dismissal.
  Unknown keys lurking from older dodot versions appear in `list` so
  they can be reset.

- **`dodot plist clean` / `dodot plist smudge` subcommands.** A pair
  of stdin→stdout filters that translate macOS plists between binary
  (what apps read at `~/Library/Preferences/...`) and canonical XML
  (what git stores in the index). They are the conversion engine for
  the upcoming git clean/smudge integration that brings GUI-app
  preferences under the same review/diff/cherry-pick workflow as
  plain-text dotfiles. Canonicalisation sorts dictionary keys
  recursively (Unicode codepoint order) while preserving array order,
  so the same logical plist always produces byte-identical XML
  regardless of the encoder's internal layout. Powered by the `plist`
  Rust crate; no `plutil` dependency at runtime. The full design
  rationale (and why plists ship as filters rather than through the
  preprocessing pipeline) lives in `docs/proposals/plists.lex`.

### Changed

- **Plist filter ergonomics.** Several small polish items:
    - `dodot plist clean` and `smudge` now produce actionable error
      messages when input isn't a valid plist, pointing at the most
      common cause (`.gitattributes` mis-binding) and at
      `dodot git-show-filters` for diagnosis.
    - `dodot git-install-filters` success message now appends a macOS
      `cfprefsd` reminder so users know to `killall cfprefsd` after
      pulling plist changes from another machine, otherwise running
      apps keep serving cached values.
    - `dodot git-install-filters --help` now documents PATH
      considerations (filters use bare `dodot` and must find it on
      `$PATH` in whatever environment git is invoked from).

- **Plist proposal revised to ship via clean/smudge filters.**
  `docs/proposals/plists.lex` was rewritten around git clean/smudge
  filters. The earlier draft modelled plist support as a
  Representational preprocessor under `docs/proposals/preprocessing-pipeline.lex`,
  with reverse conversion driven by `dodot transform check` from a
  pre-commit hook. The revision keeps the lossless binary↔XML core
  but moves the plumbing to git's own filter machinery, because
  plists drift continuously (apps rewrite preferences on settings
  changes) and the pre-commit-hook approach leaves drift invisible
  to `git status` between commits. Clean/smudge attaches the reverse
  to every git read, making `git status` reflect drift for free.
  `preprocessing-pipeline.lex` and `magic.lex` were updated to point
  at the new plists.lex and to drop stale references.

### Fixed

- **Release notarization wait loop: 30 min → 60 min.** Apple's notary
  service usually returns in under 5 min, but on slow-queue days it
  can stretch past 30 min (observed during the v1.1.1 release re-run,
  where the submission stayed `In Progress` for the full 30-min
  window). The release workflow now polls for up to 60 min before
  giving up, and the timeout warning includes the submission ID so the
  result can be checked manually with `xcrun notarytool info`. Note:
  stapling the ticket into the binary is not done — Apple's stapler
  only supports `.app` / `.dmg` / `.pkg` containers, not standalone
  Mach-O binaries. Direct downloads still pass Gatekeeper via online
  verification (requires internet); Homebrew installs are unaffected
  either way (no quarantine bit on `brew install`).
