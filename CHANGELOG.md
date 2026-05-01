# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`dodot transform install-hook`** (R4 of the template-magic track). Writes `<dotfiles_root>/.git/hooks/pre-commit` with a guarded block that runs `dodot transform check --strict || exit 1` on every `git commit`. Idempotent (re-running detects the guard line and no-ops) and additive (preserves any existing hook content). The installed hook refuses to commit when reverse-merge has work to do or unresolved `dodot-conflict` markers remain — matching the contract R3 set up. See `docs/proposals/magic.lex` §"The Commit Tier".
- **First-template-deploy prompt.** After a successful `dodot up` that leaves at least one template baseline in the cache, dodot offers (interactively, only when stdin is a TTY) to install the pre-commit hook. Y/n/show via the existing `PromptRegistry`; new catalog entry `template.install_hook`; soft failure pattern matching the plist filter prompt — never aborts `up`, never prints to stderr at default verbosity.

### Added (R3, prior)

- **`dodot transform check [--strict] [--dry-run]`** (R3 of the template-magic track). Reads every per-file baseline under `<cache_dir>/preprocessor/`, classifies each entry against the 4-state matrix (`source unchanged × deployed unchanged`), and acts: `Synced` / `InputChanged` → no-op; `OutputChanged` / `BothChanged` → run reverse-merge via burgertocow + diffy, write the patched template back to source on a clean unified diff, or surface a conflict block (no source mutation) on an ambiguous edit. `MissingSource` / `MissingDeployed` are reported. Exit 0 = clean, 1 = at least one finding. `--strict` additionally scans every cached source for unresolved `dodot-conflict` markers — exit 1 if any. `--dry-run` reports what would be patched without writing. The pre-commit hook in R4 runs `dodot transform check --strict`. See `docs/proposals/preprocessing-pipeline.lex` §6.1 and `docs/proposals/magic.lex`.
- **`preprocessing::divergence`** module with `DivergenceState` (the 4-state matrix + missing-source / missing-deployed) and walker `collect_divergences`.
- **`preprocessing::reverse_merge`** module wrapping `burgertocow::generate_diff_with_markers` + `diffy::Patch::apply` into a single `reverse_merge(template, cached_tracked, deployed) -> ReverseMergeOutcome { Unchanged | Patched | Conflict }`.
- **`Baseline::source_path`** field — captured at expansion so `transform check` can re-find the template to patch without re-walking the pack tree. Backward-compatible (`#[serde(default)]`).
- **`PENDING_EXIT_CODE` atomic** in the CLI handlers module, set by `transform_check_handler` so `main.rs` can `std::process::exit(1)` after rendering the report when there are findings. Standout's `Output` enum has only Render/Silent/Binary; this side-channel keeps the rendered output visible while still flipping the exit code for the pre-commit hook.
- **`transform-check.jinja` template** — per-file action list (synced/patched/conflict/missing) with a separate section for `--strict` unresolved-marker hits.

- **Template preprocessor produces a marker-tracked render and per-file baseline cache** (R1 of the template-magic track). Templates now flow through [burgertocow](https://crates.io/crates/burgertocow-lib)'s `Tracker`, which wraps every `{{ var }}` emission in invisible marker bytes alongside the visible render. Each successful expansion writes a JSON baseline to `<cache_dir>/preprocessor/<pack>/preprocessed/<filename>.json` carrying the rendered content, the marker-annotated tracked render, source/context hashes, and timestamp. This is the foundation the upcoming `dodot transform check` and template clean filter will read to compute reverse-merge diffs *without* re-rendering — re-rendering at clean-filter time would re-trigger any secret-provider auth prompts on every `git status`. See `docs/proposals/preprocessing-pipeline.lex` §5.2 and `docs/proposals/magic.lex` §"Cache That Makes It Cheap".
- **`Pather::preprocessor_baseline_path` / `preprocessor_baseline_dir`** for routing baseline-cache reads/writes under XDG `cache_dir`.
- **`dodot-conflict` markers and pipeline safety gate** (R2 of the template-magic track). New `preprocessing::conflict` module defines the `>>>>>> dodot-conflict (template)` / `====== dodot-conflict (deployed)` / `<<<<<< dodot-conflict` line shape and detection helpers. `dodot up` refuses to expand a template source that carries unresolved markers, returning a `DodotError::UnresolvedConflictMarker` that names the source path and line numbers and points the user at `git diff` for resolution. Without this gate, marker lines would render verbatim through MiniJinja and deploy as broken config. See `docs/proposals/preprocessing-pipeline.lex` §6.3.
- **`Preprocessor::supports_reverse_merge()`** trait method (default `false`). Generative preprocessors that emit a `tracked_render` and want their sources scanned for marker resolution before expansion override this to `true`. Templates do; identity / unarchive don't.

### Changed

- `preprocess_pack` takes an extra `paths: Option<&dyn Pather>` argument. Active callers (`dodot up`) pass `Some(...)` so baselines are written; passive callers (`dodot status`) pass `None` so they don't overwrite the last-`up` baseline.
- `ExpandedFile` gains optional `tracked_render` and `context_hash` fields, populated by generative preprocessors that support cache-backed reverse-merge (templates) and left `None` otherwise (identity, unarchive). Tests use `..Default::default()` for ad-hoc construction.

## [2.0.0] - 2026-05-01


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

- **Release pipeline migrated to canonical reusable workflow at
  `arthur-debert/release/.github/workflows/rust-cli.yml@v1`.** dodot's
  `.github/workflows/release.yml` is now a thin caller (~25 lines
  instead of 615). Bug fixes and improvements to the release pipeline
  now propagate via a single bump of the action's pin instead of
  hand-edits across 6 rust-CLIs. Behaviour-equivalent to the prior
  in-tree workflow except that the `## [Unreleased]` section in
  CHANGELOG.md replaces the separate `CHANGELOG_UNRELEASED.md` file
  (Keep-a-Changelog canonical form).
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
## [1.2.0] - 2026-04-30

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

- **macOS paths advisory probes (Phase M6).** Adds an opt-in,
  read-only enrichment layer on top of the deterministic resolver.
  The cardinal rule from the proposal still holds: probes are
  *advisory*, never authoritative — the resolver in §5 never consults
  this code, and a probe failure (no `brew` on PATH, malformed JSON,
  empty Spotlight result) never alters routing.
    - **homebrew-cask probe** (`probe::brew`) — wraps
      `brew list --cask --versions` and `brew info --json=v2 --cask
      <token>` with on-disk caching at `<cache_dir>/probes/brew/`,
      24-hour TTL, and `--refresh` invalidation. Parses each cask's
      zap stanza to extract Application Support folder candidates and
      Preferences plist paths.
    - **macOS-native probes** (`probe::macos_native`) — thin wrappers
      around `mdls` (bundle-id lookup) and `mdfind` (display-name →
      `.app` bundle path). Both gate on `cfg!(target_os = "macos")`
      and return `None` on every other host.
    - **`dodot adopt` enrichment** — when an AppSupport source's pack
      name matches an installed cask's app-support folder, adopt's
      success result includes a confirmation line ("homebrew cask
      `visual-studio-code` confirms this is …") and, when the cask's
      zap stanza lists Preferences plists, a sibling-adoption hint
      pointing at `dodot adopt ~/Library/Preferences/<file>`.
    - **`dodot up` / `dodot status` missing-target hints** — when a
      pack's planned deploy targets an `<app_support_dir>/<X>/`
      folder that doesn't exist on disk, the planner emits a soft
      warning. Cask-enriched when the brew probe finds a matching
      installed cask: "cask `<token>` is installed but `<folder>/`
      is missing — entries will deploy, but the app may not have
      created its config directory yet (try launching it once)".
      Falls back to a generic "no matching installed app appears to
      provide it" message when no cask matches. macOS-only;
      suppressed on Linux where `app_support_dir` collapses onto
      `xdg_config_home`.
    - **`dodot probe app <pack> [--refresh]` subcommand** —
      diagnostic surface listing every app-support folder a pack
      will route to (alias / force_app / `_app/`), folder existence,
      matching cask + install state, `.app` bundle, bundle ID, and
      sibling-adoption candidates. Run on demand; not part of any
      hot path.
    - **`ExecutionContext.command_runner`** — the production runner
      is now exposed on the orchestration context so probes (and any
      future advisory subprocess wrapper) reuse the same
      `CommandRunner` the datastore already drives. Tests inject a
      `CannedRunner` mock for deterministic probe coverage.
    - **Cask-aware rename suggestion** — when M5's
      capitalization-heuristic tip fires *and* the M6 brew probe
      finds an installed cask matching the pack's app-support
      folder, the suggested rename uses the cask token instead of a
      whitespace-strip-lowercase fallback. For reverse-DNS bundle-ID
      folders (`com.colliderli.iina` → cask `iina`) this is the
      difference between `iina` (sane) and `comcolliderliiina`
      (useless). The tip credits the cask so users know where the
      recommendation came from.

## [1.1.1] - 2026-04-30

### Fixed

- **CLI exits non-zero on handler errors.** Every standout-dispatched
  subcommand (`status`, `up`, `down`, `list`, `init`, `fill`, `adopt`,
  `addignore`, `probe …`) was printing `Error: …` to stdout and
  exiting **0** when the handler returned `Err`. Scripts piping with
  `&&` or CI invocations checking `$?` saw success on every failure
  path. The root cause was upstream in standout-dispatch: handler errors
  got stuffed into the success variant `RunResult::Handled(...)` and
  the binary couldn't tell them apart from real output. Fixed in
  standout 7.6.2 (arthur-debert/standout#141), which adds
  `RunResult::Error(String)`. dodot now matches that variant in
  `main.rs`, prints the message to stderr, and exits 1 — fixes every
  affected subcommand at once. A new `tests/e2e/bats/test_exit_codes.bats`
  pins the contract for `status`, `up`, `down`, and `adopt` (both
  pack-not-found and source-not-found shapes) so a future regression
  shows up immediately. Closes #86.

## [1.1.0] - 2026-04-29

### Added

- **`dodot adopt` infers the destination pack from the source path.**
  Adopt's CLI shifts from `dodot adopt <pack> <files...>` to
  `dodot adopt <files...> [--into <pack>]`. Sources under
  `~/.config/<X>/...` auto-infer pack `<X>` (created if missing) and
  use the resolver's default rule for round-trip — no `_xdg/` prefix
  in the pack tree when the inferred name matches. Sources under
  `~/.config/<X>/` itself expand to per-child plans, so each top-level
  entry of the directory becomes its own pack member instead of the
  whole directory becoming one big symlink-to-pack-root. `$HOME`-direct
  dotfiles (`~/.bashrc`, `~/.weechat/`) keep their existing
  `home.X` / `_home/X/` conventions but now require `--into <pack>`
  since HOME has no pack structure to mine. Multi-source invocations
  must agree on a single pack (or pass `--into`); disagreement is
  refused with a message naming the conflicting candidates.
  `~/Library/Containers/` is refused unconditionally (sandboxed app
  data) on every platform. See `docs/reference/symlink-paths.lex` §8
  and `docs/proposals/macos-paths.lex` §7 (the proposal is updated to
  reflect the implemented inference; the AppSupport row is reserved
  pending Phase M1's `Pather::app_support_dir()`).

  When `--into <Y>` differs from a source's natural pack name, adopt
  switches the in-pack path to `_xdg/<X>/<rest>` so Priority 2's
  `_xdg/` directory prefix bypasses pack-namespacing — the deployed
  path is unchanged regardless of pack reroute. The `_app/<X>/<rest>`
  override path is wired in the same way for the future AppSupport
  case. Pack auto-creation only happens for inferred names; explicit
  `--into <pack>` requires the pack to already exist (a typo guard).

- **Hand-written `--help` for every command.** Every dodot command (and
  the top-level binary) now ships rich `--help` text written as a
  standalone file under `dodot-cli/src/help/`, embedded into the binary
  via `include_str!` and rendered through the dodot theme with the
  same `[styling]` BBCode tags the rest of the output uses. Each
  command's help is a self-contained page with description, usage,
  options, examples, and cross-references — replacing the prior
  one-liner `about` strings that gave little more than a verb. The
  top-level `dodot --help` opens with a `GETTING STARTED` callout
  promoting `dodot tutorial` as the recommended starting point for
  new users, and the same prominence is mirrored in the `LEARN MORE`
  block at the bottom. The CLI intercepts `--help` / `-h` / `help [cmd]`
  before standout's own help dispatch so the embedded text is always
  what the user sees, including for nested probe subcommands. A
  per-file rendering test exercises every help text in `TermDebug`
  mode and fails the build if any styling tag isn't defined in the
  theme, catching typos before they ship. Standout was bumped from
  7.5 to 7.6 in passing.

- **Install-script visibility: header block, `# status:` markers, and `--verbose`.**
  `dodot up` previously discarded install-script stdout/stderr entirely,
  so a long-running script looked frozen and a misbehaving one was
  undebuggable. Three additions, all targeting install scripts:
  - The script's leading comment block (contiguous `#`-prefixed lines
    after the optional shebang) is printed when the script starts, so
    the user sees what's about to run.
  - Lines on stdout matching `# status: <message>` (or `#status:`) are
    streamed live as progress markers while the script runs. The
    convention is tool-agnostic: the markers are just shell comments
    when the script is run by hand.
  - `dodot up --verbose` (reusing the existing global flag) streams the
    script's raw stdout/stderr in real time. On failure, captured
    stderr is dumped automatically even without `--verbose` so the
    error is debuggable.

- **`probe shell-init` warns on stale profiles.** Shell-init profiles are
  written by `dodot-init.sh` only when a new shell starts, so running
  `dodot probe shell-init` from a shell that pre-dates the most recent
  `dodot up` would silently display pre-edit timings and sources. Each
  successful `up` now records a unix timestamp at
  `<data_dir>/last-up-at`, and every `probe shell-init` view (single,
  `--runs`, `--history`) compares the most recent profile's filename
  timestamp against that marker. If the profile predates the last `up`,
  a banner names both timestamps and prompts the user to open a new
  shell. Closes #59.

- **Per-source stderr capture and drill-down view for shell-init.**
  Until now, when a sourced shell file emitted to stderr or exited
  non-zero, dodot showed only an exit-code count in `--history` — the
  actual error message was on the user's terminal at startup time and
  gone. The shell wrapper now redirects each `. file` stderr to a
  per-shell scratch, re-emits live to the TTY (preserving the existing
  breadcrumb), and on non-empty stderr appends a record to a sibling
  `profile-<id>.errors.log` next to the TSV. Empty-stderr sources stay
  on the fast path — one `[ -s ]` test of overhead. Two new views read
  the sidecar:
  - `dodot probe shell-init <pack>[/<file>]` drills into one target's
    history across recent runs, inlining captured stderr under each
    failed run.
  - `dodot probe shell-init --errors-only` lists every target with a
    non-zero exit somewhere in the window, sorted by failure count desc.

  The `dodot status` runtime-failure footnote was also upgraded to
  inline a stderr excerpt from the most recent failing run (when
  captured) and to point users at the per-file probe view instead of
  `--history` (which only shows aggregate counts).

### Changed

- **`probe shell-init --history` lists newest first.** Previously the
  history table was reversed to put the latest run nearest the prompt;
  in practice this was confusing because every other dated listing in
  the tool reads newest-first. Now `--history` matches that
  convention.

### Fixed

- **`dodot up` reconciles deleted source files.** `up` was additive only:
  handler `to_intents()` emitted intents from current source, but nothing
  scanned the datastore for orphan entries. A file deleted from a pack
  would leave its data link behind, so the regenerated init script kept
  sourcing a now-missing path (silently swallowed by the `[ -f ]` guard,
  or surfacing as a non-zero exit row when profiling was on). Cleanup
  required `down + up`. Now `up` wipes each pack's datastore state for
  every configuration-category handler (path, shell, symlink) before
  re-applying from current source. Provisioning handlers (install,
  homebrew) are deliberately excluded so their sentinels keep gating
  re-runs by content hash rather than source presence. Closes #58.

## [0.22.0] - 2026-04-25

### Added

- **Interactive `dodot tutorial` subcommand.** Walks new users through
  their first pack deployment using their actual dotfiles repo. A
  hand-rolled state machine over 12 named steps renders templated
  bodies (via `standout-render`) and asks one question per step
  through a `Prompts` trait. `InquirePrompts` (production) wraps
  `inquire` with italic prompt styling tied to a new `tutorial-prompt`
  theme key; `ScriptedPrompts` (tests) feeds canned answers, surfacing
  wizard-reorder bugs at the offending step. `TutorialEnv` bundles
  fs/paths/datastore/config so tests run against a `TempEnvironment`
  fixture instead of process env. Branches handle the empty-repo case
  (offers `dodot init`), config-only packs (skip the shell-integration
  step), and the eval-line prompt (append/clipboard/skip). Resume
  state lives at `$XDG_DATA_HOME/dodot/tutorial.json` and is cleared
  on completion.

## [0.21.0] - 2026-04-25

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

## [0.20.0] - 2026-04-25

### Added

- **`dodot probe shell-init --runs N`** — aggregate the last N
  shell-startup profiles into a per-target table of `p50 / p95 / max`
  durations plus a `runs_seen / runs_total` ratio. Targets sort by
  `(pack, handler, target)` to match the deployment-map view.
  Percentiles use nearest-rank (no interpolation), which is the right
  resolution for sub-millisecond shell timings. When fewer than `N`
  profiles exist on disk the renderer warns "(requested N)" so the
  user can tell the data is sparse.
- **`dodot probe shell-init --history`** — one summary row per recent
  shell startup, oldest first, capped at 50 rows. Each row carries the
  parsed unix timestamp, shell label, total / user-sourced durations,
  entry count, and the count of entries with a non-zero
  `exit_status` — turning the once-silent "this source failed" signal
  into a single column on a trend view.
- Both new views honour `--output json` and serialize as
  `kind = "shell-init-aggregate"` / `kind = "shell-init-history"`.
  The JSON also includes raw `_us` fields alongside the humanised
  labels for programmatic consumers.

## [0.19.0] - 2026-04-25

### Added

- **`dodot probe`** — a new diagnostics command tree for introspecting
  deployed state. Three subcommands:
  - `dodot probe` — summary listing the available probe subcommands.
  - `dodot probe deployment-map` — rendered `pack / handler / kind /
    source / datastore` table derived live from the datastore. Paths are
    shortened to `~/…` where possible.
  - `dodot probe show-data-dir [--depth N]` — bounded-depth tree view of
    `<data_dir>` with per-node sizes and symlink targets. Truncated
    subtrees report `(… N more)` so nothing disappears silently. Default
    depth is 4; symlinks are never followed. Directories sort before
    files.

  All three honour `--output json` and emit a `{"kind": "…"}`-tagged
  document for programmatic consumers.
- **Deployment map file.** `dodot up` and `dodot down` now also write
  `<data_dir>/deployment-map.tsv` alongside the regenerated shell init
  script. The file is plain-text TSV with a `# dodot deployment map v1`
  header, one row per datastore entry (`pack\thandler\tkind\tsource\tdatastore`),
  overwritten on every run. Skipped during `--dry-run`. `dodot probe
  deployment-map` renders its table live from the datastore, not from
  this file; the TSV is a written snapshot for machine-to-machine
  consumers, including the forthcoming `dodot refresh` (see
  `docs/proposals/magic.lex`), which will use it for source-template
  mtime touches.
- `Pather::deployment_map_path()` trait method, returning
  `<data_dir>/deployment-map.tsv`.
- **Shell-init profiling** (Phase 2 of `docs/proposals/profiling.lex`).
  When `[profiling] enabled = true` (the default) the generated
  `dodot-init.sh` carries a runtime-detected timing wrapper around each
  `source` and `PATH` line. On bash 5+ / zsh, every shell startup writes
  one TSV under `<data_dir>/probes/shell-init/profile-<unix_ts>-<pid>-<rand>.tsv`
  with microsecond `EPOCHREALTIME` start/end pairs and the source's exit
  status — turning silent failures in user shell scripts into visible
  data. Older shells (`/bin/sh`, bash <5) fall through to the
  unmodified source/PATH path with one extra `[ "$_dodot_prof" = "1" ]`
  comparison of overhead. Disable via `[profiling] enabled = false` in
  the root `.dodot.toml`; the resulting init script is byte-identical to
  Phase 1.
- **`dodot probe shell-init`** — reads the most recent profile TSV and
  renders it grouped by pack and handler, with per-row durations,
  per-group subtotals, and a final user-sourced / dodot-framing / grand
  total breakdown. Falls back to a hint when no profile has been written
  yet, or when profiling is disabled in config.
- New `[profiling]` config section (root-only): `enabled` (default
  `true`) and `keep_last_runs` (default `100`, capped at the
  configured number per `dodot up`'s rotation pass; `0` disables
  rotation defensively rather than wiping history).
- `Pather::probes_shell_init_dir()` trait method, returning
  `<data_dir>/probes/shell-init`.

### Changed

- Shell-related handlers now recognize `.bash` and `.zsh` extensions in
  addition to `.sh`. The install handler's default claims are
  `install.{sh,bash,zsh}` and the shell handler's defaults cover
  `{aliases,profile,login,env}.{sh,bash,zsh}`. The install interpreter is
  selected from the script's extension (`.zsh` → `zsh`, otherwise `bash`),
  not from the user's login shell — the extension is the contract the
  pack author declares.
- `[mappings] install` in `.dodot.toml` now takes a list of patterns
  (e.g. `install = ["install.sh", "install.bash"]`) instead of a single
  string, matching the shape of `[mappings] shell`.

## [Unreleased]

### Changed

- **Intra-pack handler execution order is now explicit.** Previously ordering was `category → alphabetical by handler name`, which happened to produce the right sequence (homebrew, install, path, shell, symlink) but was fragile — adding a handler with a name sorted earlier alphabetically would have silently reordered the pipeline. Handlers now declare an `ExecutionPhase` (`Provision` → `Setup` → `PathExport` → `ShellInit` → `Link`), and `rules::handler_execution_order` sorts on the enum's declared order. The observable order is unchanged; the contract is now encoded in the type system, and adding a handler requires a deliberate choice of phase. `HandlerCategory` (used by `--no-provision`) is derived from phase. Catchall-last is now enforced by `Link` being the final variant rather than by convention.

## [0.18.4] - 2026-04-24

### Added

- **`--short` / `--full` output modes** for every command that renders pack status (`status`, `up`, `down`, `adopt`). `--short` collapses each pack to a single summary line (`git  (2) error`, `nvim  (3) deployed`) showing the count of files in the pack's worst-status bucket. `--full` keeps today's per-file listing and is the default. Flags are global on the root `dodot` command and mutually exclusive.
- **`--by-status` / `--by-name` grouping modes** for the same four commands. `--by-status` groups packs under coloured banners — `Ignored Packs` / `Deployed Packs` / `Pending Packs` / `Error Packs`, top to bottom — so errors land closest to the cursor where the user's eye finishes reading. Empty banners are hidden. `--by-name` keeps flat discovery-order listing and is the default. The two flag pairs are composable (all four combinations are valid) and applied via a max-status rollup per pack: `error`/`broken` → error, `pending`/`warning`/`stale` → pending, `deployed` → deployed.
- `DisplayPack.summary_status` and `DisplayPack.summary_count` exposed in JSON output for programmatic consumers.

### Changed

- **dodot's CLI theme is now adaptive for light and dark terminals.** Previously the theme hardcoded light-mode colours (`.pack-name: #000`, `.pending` with a near-white background, dim chromatics like `#008700` green and `#005F87` cyan), which were invisible or barely legible on dark backgrounds. The theme now splits into a mode-agnostic base plus `@media (prefers-color-scheme: light|dark)` blocks: monochrome values invert between modes (black → white for `.pack-name`, light-grey → dark-grey backgrounds for `.pending`), and dim chromatic values brighten for dark mode (`#008700` → `#5FD75F`, `#D70000` → `#FF5F5F`, etc). Standout's `standout-render` auto-detects the terminal colour scheme and selects the right variant per-session.
- **Shared templates moved into `dodot-lib`.** The three pack-status-rendering templates (`pack-status.jinja`, `list.jinja`, `message.jinja`) now live under `crates/dodot-lib/src/templates/` and are exported as `pub const` strings from `dodot_lib::render`. The CLI registers them via `EmbeddedTemplates::new()` using those constants. Previously the CLI owned the template files and the lib kept a drifting string-literal copy; this eliminates the duplication while keeping `dodot-lib` self-contained (no cross-crate `include_str!`).

### Fixed

- Status rows in `broken` or `stale` states now render with proper styling instead of falling through to standout's `[broken?]text[/broken?]` unknown-tag marker. The two styles were missing from the CSS theme; they now render as red (broken) and amber (stale) with the same light/dark adaptation as the rest of the theme.
- `dodot --help` (and all subcommand help) no longer shows `[about?]`, `[usage?]`, `[item?]`, `[desc?]`, `[example?]` unknown-tag markers. Setting `.default_theme("dodot")` in standout replaces the built-in help theme wholesale rather than merging with it, so those five tags were unregistered in the active theme. Added them to `dodot.css` (matching standout's defaults: `item` bold, the rest plain).

### Removed

- Unused `config.jinja` template (was embedded as a side effect of directory scanning but never referenced by any handler).

## [0.18.3] - 2026-04-23

### Changed

- Errors are now surfaced as indexed footnotes instead of being spliced into the per-file status column. Each item row stays one line with a short status label (`pending`, `error`, `stale`, …); long error bodies, stderr, and conflict reasons all render in a dedicated `Errors:` section at the bottom, referenced from the row by a `[N]` marker. Indices are command-wide, so the same scheme covers `status`, `up`, and `adopt` with one rendering path. Replaces the previous per-pack footnote mechanism and the "append a raw error row at the end of the pack" hack.

### Fixed

- `dodot up` now renders the full per-pack listing and notes section when a cross-pack conflict blocks deployment, matching what `dodot status` shows. Previously the CLI handler hardcoded an empty pack list on the cross-pack conflict branch, so users only saw the trailing conflicts dump and lost all context about what *would* have been deployed.

## [0.18.1] - 2026-04-23

### Fixed

- Symlink deploys now refuse when an ancestor of the target path is a symlink resolving into `dotfiles_root` or `data_dir`. Writing through such an ancestor landed back inside the pack store — silently clobbering pack source files (top-level files built a pack↔data-dir cycle) or surfacing as a misleading `non-symlink file at target path` (pack directories). The check runs in both real and dry-run modes; `--force` does not bypass it. Relative ancestor targets like `~/.config/warp -> ../dotfiles/warp` are lexically normalized before the prefix comparison so they get caught too.

## [0.18.0] - 2026-04-23

### Changed

- **BREAKING:** Symlink handler now deploys every pack-root entry — file or directory — to `$XDG_CONFIG_HOME/<pack>/<name>` by default (#48). Previously, top-level files defaulted to `$HOME/.<name>` and top-level directories to `$XDG_CONFIG_HOME/<name>` (no pack namespace). The new rule is consistent across files and dirs and matches modern tool conventions (nvim, helix, ghostty, kitty, alacritty, lazygit, starship, …) without forcing users to write `pack/program/` doubled paths.
- **BREAKING:** Per-file `$HOME` opt-in convention renamed: `dot.X` → `home.X`. The semantic is unchanged (`<pack>/home.bashrc` → `~/.bashrc`); the new name reads as "deploy to home as .X" instead of "this filename has a literal dot." All `[symlink.targets]`, `_home/`, `_xdg/`, `force_home`, and `protected_paths` overrides keep their existing semantics.
- The `_home/` and `_xdg/` directory prefixes are now always per-file (never wholesale-linked at the top level) — wholesale-linking the prefix dir itself would have baked the literal `_home`/`_xdg` segment into the deploy path, which is never what users meant.

### Migration notes (#48)

- A pack with `git/gitconfig` previously deployed to `~/.gitconfig`. It now deploys to `~/.config/git/gitconfig` (which git itself reads via XDG since 2.20). To keep the legacy `$HOME` path, rename the file to `git/home.gitconfig` (per-file home opt-in) or add a `[symlink.targets]` override.
- A pack with `warp/themes/` previously deployed to `~/.config/themes`. It now deploys to `~/.config/warp/themes`. Pin consumers to the new path or use `_xdg/themes/` inside the pack to skip the namespace.
- A pack with `git/dot.gitconfig` (old per-file convention) needs to be renamed to `git/home.gitconfig` — `dot.X` is no longer recognized.
- A pack containing literal `config/` or `.config/` subdirectory paths (e.g. `app/config/main.toml`) used to have that prefix silently stripped during resolution to avoid the `$XDG_CONFIG_HOME/.config/...` double-prefix. Under #48 the strip is gone; the file deploys to `$XDG_CONFIG_HOME/<pack>/config/main.toml` literally. If you relied on the old strip to land at `$XDG_CONFIG_HOME/main.toml`, move the file to the pack root (`app/main.toml` → `~/.config/app/main.toml`) or use `_xdg/main.toml` to skip the pack namespace entirely.
- No change for files matching `force_home` (ssh, gpg, bashrc, zshrc, profile, inputrc, etc.) — those still deploy to `$HOME/.<name>`.
- No change for files routed via `[symlink.targets]`, `_home/`, or `_xdg/` directory prefixes — those keep their existing behavior.

## [0.16.0] - 2026-04-23

### Added
- Provisioning script execution now prints `==== <pack> → <handler> → <script>  running…` before the spawn and `OK` (green) or `FAILED` (red) after, on stderr. Sentinel-skipped runs stay silent. Long-running scripts (e.g. brew installs) are no longer opaque from the user's side.

### Changed
- **`up` and `down` now render through `status::status()`** instead of using their own per-operation vocabulary. `dodot up video` and `dodot status video` produce identical right-column labels (`in PATH`, `sourced`, `deployed`) for the same observed state — previously `up` reported `staged bin` while `status` reported `in PATH`, which was confusing. Operation failures from `up` are overlaid as additional error rows on top of the status view. Dry-run output is unchanged (still shows planned operations). (#42)
- **`status` distinguishes "pending — clear to deploy" from "pending — would conflict on deploy"** (#43). When a non-symlink file or directory already occupies a symlink-handler target path, status renders the row with the `warning` style (label remains `pending` so the right column stays compact) and adds a per-pack footnote `(N) <path> (existing file/directory) — \`dodot up\` will refuse without --force`. Pre-existing symlinks (correct, dangling, or pointing elsewhere) are *not* flagged as conflicts because the executor's `create_user_link` gracefully replaces them on the next `up`.
- **`dodot up` auto-replaces content-equivalent pre-existing files** (#44). When `up` would deploy a symlink to a path where a regular file already exists, it now checks whether the file's content is byte-identical to the source. If so, it silently swaps the file for the dodot symlink chain — no `--force` required, no conflict reported. Direct (single-hop) symlinks pointing at the source — including relative-path symlinks — were already handled gracefully by `create_user_link`; this completes the picture for the file case. Multi-hop symlink chains are still replaced automatically (unchanged behavior). Only content-different non-symlink files still require `--force` — mismatched content is a real conflict.
- **`status` no longer flags content-equivalent files as conflicts** (#44). A pre-existing file at the user-target path whose bytes match the source is rendered as plain `pending` with no footnote, since `up` will handle it without `--force`.
- **`dodot adopt` distinguishes "fully managed" from "direct symlink to pack source"** (#44). When the user's existing symlink points directly into the dotfiles root (skipping dodot's data-link layer), adopt now skips with a clearer message that points at `dodot up <pack>` to upgrade to the full chain — instead of the previous opaque `already managed by dodot`. Sources whose symlinks already go through dodot's data dir keep the original wording.

## [0.14.0] - 2026-04-22

### Added
- **Cross-pack conflict detection** (#29): `dodot up` now collects intents from all packs before executing any, detects when multiple packs produce symlinks targeting the same resolved path, and halts with a clear error listing conflicting packs, handlers, and source files — no partial deployment occurs
- `dodot status` surfaces potential cross-pack conflicts as warnings, even for packs that aren't deployed yet
- Symlink target collisions detected across all resolution layers: `[symlink.targets]`, `_home/` prefix, `dot.` convention, `force_home`, XDG defaults
- PATH executable shadowing detected: two packs with `bin/` directories containing same-named files are flagged (one would shadow the other in `$PATH`)
- **Auto-executable permissions**: `dodot up` now automatically adds `+x` to files in path-handler directories (`bin/`), matching user intent that these files should be runnable. Controlled by `[path] auto_chmod_exec` (default: `true`). Already-executable files are left untouched; permission failures are reported as warnings, not hard errors.
- New `CrossPackConflict` error variant with structured conflict data
- `dodot status` now lists directories skipped via `.dodotignore` under an "Ignored Packs" heading, so users aren't baffled when a pack-shaped directory doesn't appear in the main listing

### Changed
- **`mappings.shell` default now includes `env.sh`** alongside `aliases.sh`, `profile.sh`, `login.sh`. Files named `env.sh` in any pack are now claimed by the shell handler (sourced at shell init) instead of falling through to the symlink handler (which previously dropped them at `~/.env.sh` and collided across packs)
- `dodot up` now uses a two-phase execution model: collect all intents first, then execute — replacing the previous sequential per-pack execution
- `--force` does not override cross-pack conflicts (it only applies to pre-existing non-dodot files); cross-pack conflicts require a configuration fix
- Orchestration pipeline split into `collect_pack_intents()` and `execute_intents()` for composability
- **Scanner is top-level only** (#37): rules match pack depth-1 entries only; nested files are the responsibility of the handler that owns the containing top-level entry. Fixes two long-standing issues:
  - A pack's `bin/tool` was being **both** staged via the path handler *and* symlinked individually via the catchall — now only the `bin/` directory is claimed (by path). One shell command, one status line.
  - A nested `foo/install.sh` used to trigger the install handler because matching was on basename-only; now only a top-level `install.sh` does.
- **Symlink handler links top-level directories wholesale**: `warp/themes/` now becomes a single symlink `~/.config/themes → <pack>/warp/themes`, not a per-file listing. Falls back to per-file mode (current behavior) when `[symlink] protected_paths` or `[symlink.targets]` reach inside the directory, preserving every existing security guarantee.
- **Top-level dirs default to `$XDG_CONFIG_HOME/<name>`** (aligning code with the longstanding docs). Top-level *files* still default to `$HOME/.<name>`. `force_home`, `_home/`, `_xdg/`, `dot.`, and `targets` overrides all still apply.
- Handler trait gains `match_mode()` (`Precise` / `Catchall`) and `scope()` (`Exclusive` / `Shared`), with a registry invariant that at most one handler may be simultaneously `Catchall` + `Exclusive`. No behavior change for existing handlers; the formalization future-proofs adding non-claiming observers.
- `Handler::to_intents` now receives an `&dyn Fs` so handlers can inspect directory contents when deciding wholesale-vs-per-file treatment.

### Migration notes
- If you previously relied on nested files being individually symlinked (e.g. `warp/themes/nord.yaml → ~/.config/themes/nord.yaml`), the whole `themes/` directory is now one symlink. The observed path `~/.config/themes/nord.yaml` still resolves identically via the directory link.
- If you need nested per-file behavior (different targets per file, or selective inclusion), add `[symlink.targets]` entries or list individual files in `[symlink] protected_paths` — either triggers per-file mode for the containing directory.
- If you had nested `install.sh` / `aliases.sh` / `Brewfile` that were (perhaps unintentionally) being picked up by their handlers, move them to the pack's top level or use `[mappings]` overrides.

## [0.9.3] - 2026-04-16

### Added
- Structured logging via `tracing` with daily-rotating file output to `~/.cache/dodot/logs/`
- `--verbose` flag: show INFO-level log messages on stderr
- `--debug` flag: show DEBUG-level log messages on stderr
- INFO and DEBUG events across orchestration pipeline and executor subsystems
- Automatic cleanup of log files older than 7 days

## [0.9.2] - 2026-04-16

### Changed
- CI: force Node.js 24 for all GitHub Actions (future-proofing for June 2026 deprecation)
- CI: replace manual `actions/cache` with `Swatinem/rust-cache` for smarter Rust caching
- CI: e2e tests now download pre-built binary from check job instead of rebuilding from source

## [0.9.1] - 2026-04-15

### Fixed
- Release workflow: add MIT license and crate metadata required by crates.io
- Release workflow: macOS signing failures no longer block binary packaging/upload
- Release workflow: fix cross-compilation install on runners with pre-existing `cross` binary

## [0.9.0] - 2026-04-15

### Added
- `[pack] ignore` now ships with sensible compiled defaults (`.git`, `.svn`, `.DS_Store`, `*.swp`, etc.) — previously the default was an empty list
- Exhaustive unit test verifying all compiled default values for `pack.ignore`, `symlink.force_home`, `symlink.protected_paths`, and all `mappings` fields

### Changed
- **BREAKING:** `[mappings] ignore` renamed to `[mappings] skip` to disambiguate from `[pack] ignore`
- Removed `genconfig` command — use `dodot config gen` (via clapfig) instead, which auto-generates a commented TOML template from struct definitions

### Fixed
- Config docs: removed phantom "App Defaults" and "App Config" layers that never existed in code; documented the actual 3-layer hierarchy (compiled defaults → root `.dodot.toml` → pack `.dodot.toml`)
- Config docs: corrected merge semantics from "arrays append" to "arrays override (last value wins)" matching actual clapfig behavior
- Config docs: fixed `[symlink]` targets syntax from incorrect bare keys to correct `[symlink.targets]` table form
- Synced `force_home` defaults in docs (was 6 entries, now all 10 matching code)
- Synced `protected_paths` defaults across both doc files and code (was inconsistent between `config-system.lex` and `symlink-paths.lex`)

## [0.1.0] - 2026-04-14

### Added

- Core dotfiles management with pack-based organization
- Symlink handler with smart path resolution (home dotfiles, XDG config, custom targets)
- Install handler with content-based checksums for idempotent script execution
- Homebrew handler for Brewfile-driven installs
- Shell handler for sourcing shell configuration files
- Path handler for adding directories to PATH
- Commands: `up`, `down`, `status`, `list`, `init`, `adopt`, `fill`, `addignore`, `init-sh`, `config`
- Pack discovery with `.dodotignore` support
- Configuration system: defaults, root `.dodot.toml`, per-pack `.dodot.toml` overrides
- `dot.` prefix convention for top-level pack files
- Per-file custom symlink target overrides
- Double-link datastore architecture for state tracking
- Shell init script generation (`eval "$(dodot init-sh)"`)
- Dry-run mode for all deployment commands
- JSON output mode for scripting
- Themed terminal output via standout
