# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!--
  ⚠️  PUT UNRELEASED CHANGES UNDER `## [Unreleased]` BELOW — NOT IN A
  SEPARATE FILE. The release pipeline
  (`arthur-debert/release/.github/workflows/rust-cli.yml@v1`, which
  shells out to `roll-changelog.sh` in that repo — not in this one)
  reads this section directly: it copies the bullets into the GitHub
  release notes and rewrites the heading to `## [<version>] - <date>`
  at release time. If `[Unreleased]` is empty, the release job fails
  with "CHANGELOG section [Unreleased] is missing or empty in
  CHANGELOG.md" and aborts before tagging.

  Use level-3 subsections (`### Added`, `### Changed`, `### Deprecated`,
  `### Removed`, `### Fixed`, `### Security`) so they nest cleanly
  under the rewritten version heading. The historical
  `CHANGELOG_UNRELEASED.md` staging file (deprecated in 4.1.0 when
  the release pipeline migrated, but not actually removed until this
  cycle) is gone.
-->

## [Unreleased]

### Added

- **User-facing reference page for the `nix` handler (#161, PR 2).** `docs/user/handlers/nix.lex` covers the manifest shapes (list / bare derivation / attribute set), the shape-agnostic `nix profile install --expr <wrapper>` invocation, the sentinel + snapshot layout, the three-state `dodot status` output, the `--provision-rerun` re-run flow, the "ensure installed" semantics that distinguish the handler from a side-profile owner, and the §9 list of explicit non-goals (no `flake.nix` / `home.nix` matching, no NixOS / nix-darwin system config, no auto-bootstrap, no removal, no channel-drift upgrades). README handler-table and the docs/user/handlers index now include the `nix` row alongside `homebrew` and `install`; `docs/user/handlers/mappings.lex` lists the default `nix = "packages.nix"` mapping in the priority table, raw-TOML example, and key-shape table.
- **Nix handler — first slice (#161, PR 1).** `packages.nix` at pack root is now matched and routed to a new `nix` handler, which runs `nix profile install` once per content hash via the same `RunOnceHandler` machinery as `install` / `homebrew`. Inherits the three-state notify-don't-rerun policy and `dodot status` integration unchanged. Accepts list-of-derivations, bare-derivation, and attribute-set manifests (e.g. `{ pkgs ? import <nixpkgs> {} }: with pkgs; [ ripgrep fd ]`). Default mapping: `nix = "packages.nix"`. (Final install-command form and shape-handling details are in the PR 3 entry below.)
- **Three-state output for run-once files in `dodot status` (#169, PR D).** `install.sh` and `Brewfile` rows now report one of: `never run` / `brew packages not installed` (no sentinel), `installed` / `brew packages installed` (sentinel matches current content), or `older version (N lines added, M removed)` / `brew packages older version (…)` (sentinel exists for a *different* content hash — the script ran successfully against an earlier version). Sentinels written before the snapshot convention was introduced fall back to `older version (no diff data)` so the run state is still surfaced. Pair the new state with `dodot up --provision-rerun` to apply the pending edits.
- **`dodot status --diff` flag (#169, PR D).** For each `older version` row whose snapshot is on disk, print a unified diff between the previously-run snapshot and the current source. Optional pack filter scopes the report (`dodot status nvim --diff`); `--diff` against packs with no `older version` entries prints just the normal status output. Old sentinels with no snapshot continue to surface in the row label, but no diff payload is emitted.
- Handler reference pages for `install` and `homebrew` updated with the new three-state semantics, the `--provision-rerun` escape hatch, the `dodot status --diff` workflow, and the on-disk snapshot location for users who want to manage state directly (#169, PR D).
- README and the `dodot status` help page now reflect the notify-don't-rerun posture for run-once files instead of the older "edits auto-rerun" wording (#169, PR D).
- Internal: `RunOnceCommand` trait + generic `RunOnceHandler<C>` in `crates/dodot-lib/src/handlers/run_once.rs`, the shared shape behind the `install` / `homebrew` / forthcoming `nix` handlers (#169, PR A). The shared body owns checksum, sentinel, intent emission, and status lookup; per-handler specialization (program name, argument shape, optional pre-flight validation, status copy) is a small trait surface.
- `DataStore::did_run` query: three-way result (`NeverRan` / `RanCurrent` / `RanDifferent`) reporting whether a run-once file has been run by a handler and whether the recorded content hash matches the current file (#169, PR C). Implementations also write a `<sentinel>.snapshot` sibling capturing the file content as it was at the time of a successful run, enabling future diff display in `dodot status`.

### Changed

- **Behavior change for `install.sh` and `Brewfile` (#169, PR C):** when these files change after a successful run, `dodot up` no longer auto-reruns them. Instead, `dodot up` reports `ran older version of <file> — run \`dodot up --provision-rerun\` to apply current` and leaves the prior state untouched. Run-once semantics are now strictly *opt-in re-execution* — the dodot user, not the file's mtime, decides when to re-run consequential scripts. To apply the new content, pass `--provision-rerun` to `dodot up`. (Note: `--force` is a *separate* flag that only overwrites pre-existing files at symlink target paths; it does not trigger run-once re-execution.) Manual `brew uninstall` of packages a `Brewfile` still lists likewise stays sticky: the sentinel records "we ran with this content" and dodot considers the work done until the file changes or `--provision-rerun` is passed.
- **Nix handler — uniform run-once lifecycle (#161, PR 3).** Removed the planning-time content-shape probe (`nix eval --apply`) and the v1 rejection of attribute-set manifests. The handler now invokes `nix profile install --expr <wrapper>` with a shape-normalizing Nix expression that collapses list / bare-derivation / attribute-set manifests to a single list before installing — same install command for every accepted shape, no per-shape dispatch. **Attribute-set manifests now install** (`{ pkgs ? import <nixpkgs> {} }: { ripgrep = pkgs.ripgrep; }`). Malformed content (syntax errors, unsupported shapes) surfaces at apply time via the `nix` subprocess, the same way a broken `Brewfile` surfaces a `brew bundle` error and a broken `install.sh` surfaces a `bash` error. This restores the lifecycle invariant that every run-once handler treats `has-run` / `which-version-has-run` / `will-run` identically: a previously-installed `packages.nix` the user later edits is now reported as `older version` (not failed planning), uniformly with `install` and `homebrew`. Constraint documented as a "Lifecycle invariant" section on `RunOnceCommand`.
- Internal: `install` and `homebrew` handlers are now built on top of `RunOnceHandler<C>` via `InstallCommand` / `BrewfileCommand` specializations (#169, PR B). Duplicated checksum helpers consolidated in the shared module. Aside from the policy flip in PR C, no other observable behavior changed in the retrofit.

## [4.1.1] - 2026-05-12


### Added

- **User docs for the externals handler (PR 6 of stacked series)** —
  New `docs/user/handlers/external.lex` covering the four entry
  types (file / git-repo / archive / archive-file), the freshness
  model per type, the datastore layout, drift detection, failure
  posture, and the "live edits" section per the standard handler
  doc shape. Verified against source.

- **`dodot status --check-drift` for externals (PR 5 of stacked series)** —
  Opt-in flag that hashes each deployed external entry's content
  and compares against the configured signature, surfacing any
  divergence as a warning on the status output.

  - `file` entries — compute sha256 of the datastore copy; report
    drift when it diverges from the configured `sha256`.
  - `git-repo` entries — shell out to `git status --porcelain`
    on the local clone; non-empty output indicates the working
    tree diverged from HEAD.
  - `archive` / `archive-file` — drift detection deferred to a
    later release; surfaced as "not implemented" rather than
    silently skipped (silence would suggest "clean").

  Off by default because hashing every deployed external on every
  `status` is wrong for big trees like `oh-my-zsh`. Drift is the
  user-edit oracle, distinct from upstream-freshness which fires
  automatically on every `up`.

- **Externals handler: `archive` and `archive-file` types (PR 4 of stacked series)** —
  Two new spec variants in `externals.toml`:
  - `type = "archive"` — download an archive, sha256-verify, extract
    the whole tree into the datastore. The user-visible target
    symlink points at the extracted directory.
  - `type = "archive-file"` — same fetch + verify path, but only
    one named member is extracted. The target symlink points at
    the single extracted file.

  Supported formats: gzipped tar (`.tar.gz` / `.tgz`) and zip
  (`.zip`). Format is inferred from the URL filename; pass
  `format = "tar-gz"` or `format = "zip"` explicitly for URLs
  without a recognized suffix. Archive entries with unsafe paths
  (absolute, `..` components) are rejected at extraction time.
  `zip = "2"` added as a dependency for zip support.

- **Externals handler: `subpath`, `ref`, `commit` (PR 3 of stacked series)** —
  `type = "git-repo"` now accepts three optional fields:
  - `subpath = "themes"` — sparse-checkout pattern. Only that
    subtree is materialized on disk, and the user-visible target
    symlink points at it (not the whole clone).
  - `ref = "v1.2.3"` — tracking reference (tag, branch, etc.).
    Each `up` runs `git ls-remote <ref>` instead of `HEAD`;
    refresh fires when that reference's SHA changes.
  - `commit = "<full-sha>"` — frozen commit. Skips `ls-remote`
    entirely; the local clone is compared against the configured
    commit and only refreshes when the user edits the TOML.

  `ref` and `commit` are mutually exclusive — parsing rejects an
  entry that sets both. Git transport gains sparse-checkout cone
  mode and `--branch <ref> --single-branch` when configured.

- **Externals handler `type = "git-repo"` (PR 2 of stacked series)** —
  `externals.toml` now accepts `type = "git-repo"` entries. The
  executor shallow-clones (`--depth=1 --filter=blob:none`) into the
  datastore on first run and uses `git ls-remote HEAD` as the
  freshness oracle on subsequent runs: only when the upstream SHA
  differs from the local clone's HEAD does dodot shell out to
  `git fetch --depth=1` + `git reset --hard FETCH_HEAD`. Offline
  is tolerated — `ls-remote` failing during a refresh leaves the
  cached clone in place and surfaces a non-success result. The
  `git` binary on PATH is required; missing-git is a hard error
  (not transient). Sparse-tree fetch via `subpath` and
  `ref` / `commit` pinning arrive in a follow-up PR.

- **Externals handler (PR 1 of stacked series, file type only)** —
  packs can now declare remote resources in an `externals.toml` file
  at the pack root. Each section declares one entry; this PR
  implements `type = "file"` with mandatory `sha256` content pinning.
  Fetched bytes land in the datastore under
  `<data_dir>/packs/<pack>/external/<name>/<filename>` and a
  user-visible symlink is created at the configured `target`.
  Sentinel-gated: re-running `dodot up` with an unchanged sha256 is a
  no-op; bumping the sha256 in the TOML invalidates the old sentinel
  and triggers a re-fetch. Transient network failures soft-fail and
  leave any cached copy in place; integrity (sha256) mismatches are
  hard failures that refuse to write. New `ExecutionPhase::External`
  slot runs between `Filter` and `Provision`. `git-repo`, `archive`,
  and `archive-file` types parse but report "unsupported" — they land
  in subsequent PRs of the series. Closes part of #152.

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

- **`[mappings] shell` default is now `["*.sh", "*.bash", "*.zsh"]`** —
  any file at a pack's root with a shell extension gets sourced,
  replacing the older fixed allowlist (`aliases`/`profile`/`login`/`env`
  × three extensions). The wildcard matches the convention in
  hand-curated dotfile repos (holman, mathiasbynens, paulirish, …),
  where loose shell files at the top of a pack — `aliases.sh`,
  `path.zsh`, `functions.bash`, `50_prompt.sh` — are uniformly meant
  to be sourced into the interactive shell. Pack authors no longer
  have to rename files to a fixed set of names. Three companion
  guarantees:
  - **Recursion safety**: scanning is depth-1, so a nested
    `hypr/scripts/foo.sh` (a window-manager helper invoked by another
    tool, not the shell) is *not* sourced — it flows through the
    symlink handler unchanged. Tiling-WM and tmux helper scripts
    keep working.
  - **`install.sh` is preserved**: the install rule sits at priority
    20, above the priority-10 shell wildcard, so an install hook
    never gets accidentally sourced regardless of mapping order.
  - **`README.sh` and friends still get skipped**: the `skip` filter
    at priority 50 also outranks the shell wildcard.

  Migration: users who relied on a non-`aliases`/`profile`/`login`/`env`
  `.sh` file at a pack root being symlinked rather than sourced should
  set `[mappings] shell = ["aliases.sh", "profile.sh", …]` (the old
  default) per-pack. Users with `.sh` files inside subdirectories
  (`bin/foo.sh`, `scripts/bar.sh`) need no migration — those have
  always flowed through the symlink handler and continue to do so.
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
- **User-facing documentation rewritten as a snippet library.** The
  former monolithic `docs/user/handlers.lex` and `docs/user/commands.lex`
  are split into a per-topic library under `docs/user/glossary/`,
  `docs/user/handlers/`, and `docs/user/commands/` (38 files total),
  each carrying a `:: verified ::` stamp confirming claims were checked
  against source. The two original files are now thin index pages
  (option *a*) that point at the per-topic snippets. New
  `docs/user/commands/git-augmentation.lex` is the conceptual overview
  for the install-ladder + three rungs (pre-commit hook, plist filter,
  template filter) + Tier-2 alias. Cross-references in `getting-started`,
  `dev/handlers`, `reference/handlers`, `dev/cli-output`, and
  `proposals/shipped/conditional-running` updated to point at the new
  per-topic homes.
- **Top-level user docs as a narrative layer over the snippet library.**
  Adds `paths.lex`, `adopting.lex`, `shell-integration.lex`,
  `filters.lex`, `plists.lex`, `troubleshooting.lex`, and rewrites
  `getting-started.lex` to a shared skeleton (intro / when you reach
  for this / mental model / walkthrough / watch-out-for / see-also).
  Each cross-links into the existing per-topic snippets under
  `docs/user/{glossary,handlers,commands}/` and carries `::
  verified ::` after cross-checking against source. The README is
  rewritten with corrected install instructions (Homebrew tap is
  `arthur-debert/tools/dodot`), the actual release-artifact list
  (macOS arm64, Linux x86_64+arm64, .deb amd64+arm64 — no
  fictitious macOS Intel build), a minimal-vs-advanced two-tier
  capability framing, and right-aligned status/up output blocks.
  Stale references to the old `{aliases,profile,login,env}`
  shell-handler allowlist cleaned up across
  `docs/reference/handlers.lex`, `docs/dev/handlers.lex`, and
  `docs/user/glossary/rule.lex`.
- **`docs/user/configuration.lex` brought into sync with the schema.**
  The doc now covers every key in `DodotConfig`: added `[symlink]
  force_app` / `app_aliases` / `app_uses_library` / `plist_extensions`,
  the `[path]` section (`auto_chmod_exec`), `[preprocessor.template]
  no_reverse`, `[preprocessor.age]`, `[preprocessor.gpg]`,
  `[profiling]` (root-only), and `[secret]` (root-only) with all six
  providers. The intro is reframed around the convention vs.
  customization choice and the per-file (`_home/`, `home.X`, `_app/`,
  …) vs. per-pack/repo (`.dodot.toml`) scope axes, and surfaces the
  CLI helpers (`config list` / `get` / `set` / `unset` / `gen`).
  Stock values cross-checked against `dodot config list` output.
- **Removed the staging `CHANGELOG_UNRELEASED.md` file.** Unreleased
  entries now go directly under `## [Unreleased]` in `CHANGELOG.md` —
  the convention the canonical release pipeline
  (`arthur-debert/release/.github/workflows/rust-cli.yml@v1`) expects.
  The migration was announced in 4.1.0 ("`[Unreleased]` section in
  CHANGELOG.md replaces the separate `CHANGELOG_UNRELEASED.md` file")
  but the file itself wasn't removed and contributors kept staging in
  it, leaving `[Unreleased]` in `CHANGELOG.md` empty and silently
  breaking every `Release` run. Final pre-deletion content has been
  folded into this section.

### Fixed

- `dodot tutorial`'s pack-kind classification (`is_shell_filename`)
  recognized only the legacy `aliases`/`profile`/`login`/`env`
  stems × shell extensions, so a pack containing only `path.sh`,
  `functions.zsh`, or `50_prompt.bash` mis-classified as
  `ConfigOnly` despite those files being sourced by the shell
  handler under the `*.{sh,bash,zsh}` wildcard default. Now
  classifies any shell-extension file at pack root as a shell file.
- `dodot config gen` and `dodot config schema` panicked at startup
  with `Long option names must be unique for each argument, but
  '--output' is in use by both 'output' and '_output_mode'`. The
  collision was between standout's global `--output` (output-mode
  selector) and clapfig's gen/schema output-file flag. Renamed the
  latter to `--out`; short `-o` is preserved, so `dodot config gen
  -o .dodot.toml` keeps working and `--out` replaces `--output` as
  the long form.
- `dodot status` (and the post-`up` render that runs through it) now
  drives symlink rows off the planner's `HandlerIntent::Link` list
  instead of re-walking scanner matches and re-resolving targets.
  Previously, escape-prefix dirs (`_home/`, `_xdg/`, `_app/`, `_lib/`)
  surfaced as a single bogus row pointing at the default-rule path
  (e.g. `iina/_app ➞ ~/.config/iina/_app pending`) — even immediately
  after a successful `up` that had correctly deployed leaf files to
  the prefix-resolved target (`~/Library/Application Support/...` for
  `_app/`). Status now expands the prefix per-leaf and reflects the
  real chain state, so "deployed" actually means deployed. `_lib/` on
  non-macOS continues to surface only the planner warning, since
  those leaves produce zero intents.
## [4.1.0] - 2026-05-04


### Added

- **Conditional running (gates) — phases C1–C5** (`docs/proposals/shipped/conditional-running.lex`). Pack files, directories, and whole packs can now opt out of deployment based on host facts (OS, arch, user-defined dimensions) without touching code or rendering templates. **File-level gates** use a `_<label>` filename infix: `install._darwin.sh` deploys on macOS only, `aliases._linux.sh` on Linux only; passing-gate entries surface under their stripped names so existing `[mappings]` rules still match (`install._darwin.sh` → `install.sh`), and failing-gate entries surface as `gated out (label=...)` rows in `dodot status` with a footnote showing the predicate vs the host. **Directory-level gates** use `_<label>/` segments: `_darwin/install.sh` descends transparently when the predicate passes (children surface at pack root with the segment stripped), or surfaces as a single `gate` match when it fails. Gates nest (`_darwin/_arm64/install.sh` requires both) and compose with templates (`aliases._darwin.sh.tmpl` strips to `aliases.sh.tmpl`) and with routing prefixes (`home.bashrc._darwin` strips to `home.bashrc`; `_darwin/_home/.bashrc` surfaces under `_home/` at pack root). Routing-prefix tokens (`home`, `xdg`, `app`, `lib`) are explicitly excluded from gate-directory parsing. **Pack-level `[pack] os = [...]`** skips entire packs as a successful no-op (the same shape `.dodotignore` produces); empty list means "all OSes," `macos` is recognized as an alias for `darwin`. **Built-in label table** seeds `darwin` / `linux` / `macos` / `arm64` / `aarch64` / `x86_64`; **user-defined labels** in `[gates]` are inline tables of `(dimension, value)` AND-equality pairs (`arm-mac = { os = "darwin", arch = "arm64" }`) layered with the standard 3-tier root → pack inheritance. Unknown labels are a hard scan-time error (typo guard). New **`--only-os <os>`** flag on `up`/`status` overrides detected OS at the root-`[gates]` level so `--only-os linux` on a macOS dev host previews the Linux deployment. New **`HostFacts`** cached on `ExecutionContext`, detection helpers shared with the template preprocessor. New **`Health::Gated { label, expected, actual }`** status variant; symbol/description for the `gate` handler mirror `skip`. Filter-phase `GateHandler` claims gated entries and emits no intents. New `tests/e2e/bats/test_gates.bats` covers all five surfaces (filename gates, directory gates, nesting, pack-level `os`, `--only-os`, composition with templates and routing prefixes) with 18 e2e tests; ~24 new unit tests in `gates/` and ~16 new scanner integration tests.

- **Unified `mappings.ignore` / `mappings.skip` as Filter-phase handlers.** Pack files can be excluded from deployment via two real handlers in a new `ExecutionPhase::Filter` slot that sorts before all other phases. **`mappings.ignore`** (silent — drops the match before status rendering) replaces the previous `!`-prefix exclusion phase and the `Rule::is_exclusion` shape in the matcher. **`mappings.skip`** (visible — surfaces as `skipped` in `dodot status`) replaces the synthetic `excluded` handler-name (which had no registry entry) used by the previous `mappings.exclude` for documentation/legal files. Both handlers produce zero intents and win via ordinary descending priority — `mappings.ignore` at 100, `mappings.skip` at 50, precise mappings at 10 — so README/LICENSE/CHANGELOG-like files can no longer be accidentally claimed by `mappings.shell` or the catchall, while explicit silent-skip still wins when both apply. Drops the hard-coded `handler == "excluded"` case-insensitivity check; `case_insensitive` is now a per-rule property on `Rule`. Per-scan `has_ci_rules` short-circuit so `match_file` only allocates the lowercased filename copy when at least one compiled rule actually requested case-insensitive matching. The pack-level `.dodotignore` marker is unchanged on disk but is now referred to as the "pack-ignore" mechanism in user-facing docs to disambiguate it from the intra-pack `mappings.ignore` / `mappings.skip` keys.
## [4.0.0] - 2026-05-03


### Changed

- **Secrets documentation polish (post-S5).** Spec drift audit + new user / dev guides now that all five S1–S5 phases have shipped: new user-facing guide `docs/user/secrets.lex` walks through both shapes (value injection, whole-file decryption), the six providers, the edit loop, the `dodot secret probe` / `list` / `transform status` inspection surface, and trust-boundary expectations; new developer guide `docs/dev/secret.lex` covers the module layout, the `SecretProvider` trait, `SecretRegistry` + within-run cache, the sentinel-based line tracking in the `secret()` MiniJinja function, the sidecar lifecycle, two linear code walks (a template `secret()` call and a `*.age` whole-file decrypt) and a "how to add a new provider" cookbook. The original design proposals `docs/proposals/secrets.lex` + `secrets-testing.lex` are now under `docs/proposals/shipped/` (matching the existing `magic.lex` / `plists.lex` precedent) with shipped-status banners and a new §9 "Implementation Notes vs. Spec" section that documents each deviation and deferral (install-ladder rung deferred, AST pre-walk + per-provider batching deferred, `secret-tool` underscore TOML key wart, `[secret]` root-only deviation, `cache_within_run` hard-coded as default behavior, `age` 2-arg form silently dropped, ProbeResult variant additions). `docs/reference/pre-processors.lex` §6 — the "what's planned" line for secrets — is now a full description of what shipped, replacing the stale "designed but not yet shipped" mention.

### Added

- **Phase S5 of the secrets feature** — ergonomics: two new read-only commands for inspecting secret state without running `dodot up`, plus a `transform status` enhancement that surfaces sidecar references. **`dodot secret probe`** runs `probe()` on every configured provider and prints one row per provider with its outcome (Ok / NotInstalled / NotAuthenticated / Misconfigured / ProbeFailed) and rendered hint — useful as a "is my secrets setup healthy?" check before relying on `dodot up`'s own preflight to surface the same diagnostics; the disabled-section / no-providers-enabled branch surfaces a beginner-friendly config snippet listing every available scheme. **`dodot secret list`** walks every pack's templates, finds every `secret(...)` call via a hand-rolled scanner (canonical MiniJinja shapes — single/double quotes, whitespace tolerance, word-boundary anchoring so `mysecret(...)` / `secrets(...)` don't match), and prints one row per occurrence with per-row warnings when the referenced scheme has no provider enabled in the current config; rollup at the bottom names schemes that need a provider before `dodot up` can resolve them. Useful BEFORE the first `dodot up` to inventory what a repo needs. **`dodot transform status`** now reads each baseline's `<baseline>.secret.json` sidecar and surfaces the resolved references inline as `secret: <ref>` lines under each entry — users can see WHICH secrets each baseline depends on without re-rendering (no provider invocations, no auth prompts). Sidecar reads are best-effort: a parse error leaves that row's references empty rather than failing the whole report. New `secret-probe.jinja` and `secret-list.jinja` templates; eight new bats tests in `test_secrets_probe_list.bats` (4 probe + 3 list cases) plus one in `test_transform_status_and_alias.bats`. **Deferred to a follow-up**: the spec'd install-ladder rung for missing secret providers (`secrets.lex` §S5 first bullet) — that integration requires structural changes to `LadderRung` (`&'static str` → `String` for per-provider component keys) plus a new "Yes action = write `[secret.providers.X] enabled = true` to `.dodot.toml`" shape unlike the existing file-writing rungs; it's reviewable in isolation.
- **Phase S4 of the secrets feature** — OS-level providers `keychain` (macOS Keychain via `/usr/bin/security`) and `secret-tool` (freedesktop Secret Service). Reference shape is `<scheme>:<service>` or `<scheme>:<service>/<account>` for both — the same shape on either platform so users moving dotfiles between macOS and Linux only swap the scheme prefix. The `keychain` provider runs `security find-generic-password -s <service> [-a <account>] -w` and maps the documented exit codes (44 → "not found" with `security add-generic-password` recovery hint, 51 → "User interaction is not allowed" with unlock guidance); the `secret-tool` provider runs `secret-tool lookup service <s> [account <a>]` and maps the canonical "exit 1, empty stderr" no-result shape plus the D-Bus / locked-keyring failure modes (`Cannot autolaunch D-Bus`, `not activatable`, etc.) to actionable diagnostics. Both providers are opt-in via new `[secret.providers.keychain]` and `[secret.providers.secret_tool]` config sections (note the underscore in the TOML key — the scheme prefix in `secret(...)` references stays hyphenated as `secret-tool:` to match the binary name). Cross-platform behavior: `keychain` on Linux probes `NotInstalled` with a "use secret-tool instead" pointer; `secret-tool` on macOS probes `NotInstalled` with a "use keychain instead" pointer — preflight diagnostics name the wrong-platform provider rather than emitting a silent "no provider for scheme" mismatch. 31 new tier-0 tests via `ScriptedRunner` cover parse / probe / resolve. **No tier-1 e2e bats this phase** — both providers talk to live OS keystores and a hermetic test setup needs sandboxed `security create-keychain` (macOS) or per-fixture `gnome-keyring-daemon` + `dbus-daemon` (Linux), which lands with the dedicated CI runners per `secrets-testing.lex` §5.3.
- **Phase S3 of the secrets feature** — whole-file decryption preprocessors for `age` and `gpg`. Pack files like `ssh/id_ed25519.age` or `vault/secret.toml.gpg` are now decrypted at `dodot up` time, the plaintext lands in the datastore, and the symlink handler links it to the user's home at the suffix-stripped path. Both preprocessors are `TransformType::Opaque` (no reverse-merge — the user's edit loop is decrypt → edit → re-encrypt → commit), and both emit `deploy_mode = Some(0o600)` so the pipeline chmods the rendered datastore file to 0600 regardless of umask, per `secrets.lex` §4.3. New `[preprocessor.age]` (with `enabled`, `extensions`, `identity` — defaults to `~/.config/age/identity.txt` via `from_env`) and `[preprocessor.gpg]` (with `enabled`, `extensions = ["gpg", "asc"]`) config sections; both default to `enabled = false` so a fresh dodot install never shells out to `age` / `gpg` against random files. `gpg` runs with `--decrypt --quiet --batch` so a `dodot up` never blocks on a TTY-only passphrase prompt; users pre-cache via gpg-agent or use a smartcard-backed key. Diagnostic mapping covers each tool's documented failure modes (age: "no identity matched the recipients", "identity file does not exist"; gpg: "No secret key", "Bad passphrase", "agent_genkey failed", "can't open"). New `ExpandedFile.deploy_mode: Option<u32>` plumbing pins mode 0600 with a tier-0 test using `ScriptedPreprocessor`. New tier-1 hermetic bats: `test_secrets_age.bats` (3 tests) and `test_secrets_gpg.bats` (3 tests) generate fresh keypairs in `$SANDBOX`, encrypt fixtures, and verify end-to-end decryption + 0600 enforcement; tests `skip` when the host lacks the binary. `dev-shell.sh secrets-age` and `secrets-gpg` add interactive fixtures.
- **Phase S2 of the secrets feature** — `bw` (Bitwarden CLI) and `sops` (Mozilla SOPS) providers, within-run caching, and reverse-merge sidecar masking. The `bw:` scheme resolves `bw:<item>` (default `password` field) or `bw:<item>#<field>` for `username` / `password` / `notes` / `totp` / `uri`; vault state is probed via `bw status` so a locked vault surfaces as `NotAuthenticated` with a "run `bw unlock` and export `BW_SESSION`" hint up-front rather than a generic resolve failure. The `sops:` scheme resolves `sops:<file>#<dot.path>` — files are anchored at the dotfiles root by default so the in-tree `.sops.yaml` configures both `sops --decrypt --extract` and dodot's reference parsing, and the dot-separated key path is translated to SOPS's bracketed `--extract` argument. `SecretRegistry` now holds a within-run cache so a reference appearing in N templates fires the underlying provider exactly once; pinned with `MockSecretProvider`'s invocation counter. `commands::transform::check` and the `template clean` filter now load each baseline's `<baseline>.secret.json` sidecar and pass the secret line ranges to burgertocow 0.4.0's new `DiffOptions::with_mask` so a rotated secret value in the deployed file no longer surfaces as a template-space diff that would rewrite the `{{ secret(...) }}` expression to the literal new value (defeating the abstraction). New bats e2e in `test_secrets_bw.bats` (5 tests, stub `bw` on PATH) and `test_secrets_reverse_merge_mask.bats` (2 tests, masked rotation invisible to `transform check`); `dev-shell.sh secrets-bw-stub` adds a Phase S2 fixture mirroring `secrets-pass`. Bumps `burgertocow-lib` 0.3 → 0.4.

- **Phase S1 of the secrets feature** (`docs/proposals/secrets.lex` + `docs/proposals/secrets-testing.lex`). Templates can reference values stored outside the dotfiles repo via a new `secret(...)` MiniJinja function — `{{ secret("pass:test/db_password") }}` — which dispatches to a registered `SecretProvider` impl, resolves the value at render time, and zeroizes the in-memory copy on drop (`SecretString`, no `Debug` / `Display`). Phase S1 ships two providers: `pass` (password-store) and `op` (1Password CLI), both opt-in via `[secret.providers.<scheme>] enabled = true` so a fresh dodot install never shells out to a secret tool unprompted. A run-level preflight runs once per `dodot up`, probes every enabled provider, and aggregates `NotInstalled` / `NotAuthenticated` / `Misconfigured` outcomes into a single user-facing message before any rendering begins — so the user sees every fix-it pointer at once instead of one error per template. Per-render `<baseline>.secret.json` sidecars track the line ranges produced by each `secret(...)` call so downstream consumers (dry-run preview, burgertocow#13 mask integration) can mask resolved values without re-rendering. Multi-line provider returns are refused at render time (security: no whole-file deploys via the value-injection path; use a future whole-file provider instead). `dodot up --dry-run` and `dodot status` honor the `PreprocessMode::Passive` envelope (`secrets.lex` §7.4) — provider `resolve()` is never called from these commands; pinned in tests with a `PanickingProvider` that aborts on call. New `tests/e2e/bats/test_secrets.bats` covers the `pass` happy path, sidecar generation, preflight blocking on missing binaries, missing-reference errors, and the Passive contract end-to-end via a stub `pass` binary; `tests/e2e/bats/helpers/dev-shell.sh secrets-pass` drops developers into an interactive sandbox with the same fixture pre-seeded.
## [3.0.0] - 2026-05-02


### Changed

- **E2E suite runs ~7× faster on macOS dev hosts** (`tests/e2e/bats/helpers/setup.bash` — `hide_brew_from_path`). Every bats sandbox now installs a per-test `brew` shim (under `$SANDBOX/.brew-muzzle/brew`, exits non-zero with no output) and prepends it to `PATH`. dodot's macOS-side advisory probes (`probe::brew::list_installed_casks` and friends) hit the shim, get a non-zero exit, and fall through to their empty-set branches — same code path Linux CI runs. Previously, every `brew` invocation on a host with Homebrew installed spawned two `curl` processes phoning home to Homebrew's analytics endpoint with `--max-time 3`, turning a sub-30s CI run into a 7-minute local run; the worst single file (`test_macos_paths.bats`) went from 397s to ~2s. The shim is reset per-test (lives under `$SANDBOX`, wiped by `sandbox_teardown`), and `HOMEBREW_NO_ANALYTICS=1` / `HOMEBREW_NO_AUTO_UPDATE=1` / `HOMEBREW_NO_INSTALL_FROM_API=1` are exported as belt-and-suspenders. Tests that need to exercise the *real* brew probe code path call `unhide_brew_for_test` — opt-in by design, so a future PATH refactor can't silently re-introduce the slowdown. New `test_brew_probe.bats` is the canonical opt-in user and a regression guard for `crates/dodot-lib/src/probe/brew.rs` on macOS dev hosts (skips on Linux CI / fresh-brew machines).

### Added

- **Per-file routing prefixes `app.X`, `xdg.X`, `lib.X`** (parallel to the existing `home.X`). Each is the single-file counterpart to its `_app/` / `_xdg/` / `_lib/` directory cousin, with the same skip-pack-namespace semantics: `app.settings.json` → `<app_support_dir>/settings.json`, `xdg.mimeapps.list` → `$XDG_CONFIG_HOME/mimeapps.list`, `lib.foo.plist` → `$HOME/Library/foo.plist` (macOS only; warn-and-skip on other platforms). Prefixes apply to top-level files only; nested `subdir/app.X` keeps the literal name. Empty remainders (`app.`, `xdg.`, `lib.`) fall through to the default rule rather than targeting a bare directory root, mirroring the existing `home.` regression. Fills the symmetry gap between the `home.X` per-file form and the `_home/`-family directory forms.
- **`RoutingOverrideConflict` error.** When a pack file has both a `[symlink.targets]` config entry and a filesystem-naming routing prefix (`home.X`/`app.X`/`xdg.X`/`lib.X` or `_home/`/`_xdg/`/`_app/`/`_lib/`), `dodot up` and `dodot status` refuse to deploy that one file and surface a message naming the pack, the pack-relative path, and the `targets` value — forcing the user to pick a single source of truth instead of inheriting a silent precedence rule. Other files in the pack and other packs are unaffected. Conflict detection is intentionally narrow: only `[symlink.targets]` (which names a single file) participates; `force_home` / `force_app` / `app_aliases` are pack- or list-scoped policies that prefixes are explicitly the way to opt out of. `docs/reference/symlink-paths.lex` §6.6 documents the rule.
- **`[preprocessor.template] no_reverse` per-file opt-out** (R9 of the template-magic track). New config field accepts a list of glob patterns (matched against each template source's filename) that flag files as opted out of burgertocow's reverse-merge. Matching files: still render normally on `dodot up`, still tracked in the baseline cache, still surfaced by `dodot transform status` — but `dodot transform check` short-circuits to Synced for them (no source mutation, no findings, exit 0), and the template clean filter falls through its slow path to "echo stdin." Useful for templates whose content is mostly dynamic — the heuristic degrades there and tends to produce more conflict markers than usable diffs. Honors the standard root → pack config inheritance, so per-pack overrides work.
- **Status banners on shipped proposals.** `docs/proposals/magic.lex` and `docs/proposals/template-expansion.lex` carry "implemented and shipped" headers (mirroring `plists.lex`'s pattern), with magic.lex gaining a §6 "Implementation Notes vs. Spec" section that documents the deviations between the original design and what shipped: the R0–R8 phased rollout, the two-line hook command, bash/zsh-only alias coverage, `git-install-alias` shipping in R7 (vs deferred), and the `no_reverse` opt-out itself.
- **`docs/reference/template-magic.lex`** — user-facing walkthrough of the install ladder (Tier 1 hook / Tier 2 filter / Tier 3 alias), the day-to-day workflow (vanilla `git status` / `git diff` / `git commit`), conflict resolution, opting out (including `no_reverse`), and the cost ladder. The reference complement to the proposal docs.
- **`pre-processors.lex` §6 flipped from "git-integration layer NOT yet shipped" to a comprehensive list of shipped commands** (`transform check/status/install-hook`, `template install-filter/clean`, `refresh`, `git-install-alias`, the per-file baseline cache).

### Fixed

- **`dodot up` no longer silently overwrites deployed-file edits** (issue #110, preprocessing-pipeline.lex §6.4). When a deployed template output has diverged from its cached baseline (the user edited the deployed file in place since the last `up`), `dodot up` now preserves the edit instead of clobbering it. The render is skipped, the user's bytes stay on disk, and a one-line warning surfaces telling them to run `dodot transform check` to reconcile or re-run with `--force` to overwrite. Both row-3 (`OutputChanged`) and row-4 (`BothChanged`) of the §6.4 matrix collapse to the same outcome — `dodot up`'s contract is now "I will not destroy your work, period." The clever 3-way reverse-merge still lives in `dodot transform check` (the pre-commit hook) where it belongs. Staleness is defined from file content only: env vars referenced via `{{ env.X }}` are read live at render time and intentionally do not participate in the divergence guard's cache-invalidation signal. Plain `dodot up` re-renders templates every run, so an env-var change is picked up automatically as long as the deployed file still matches its baseline; the only case where `--force` is needed is when the divergence guard is preserving an in-place edit on the deployed file. Stable values that should participate in the guard's invalidation belong in `[preprocessor.template.vars]` (the `user_vars` namespace), not `env.*` — docs (`pre-processors.lex`, `template-magic.lex`, `template-expansion.lex` §3.2, `preprocessing-pipeline.lex` §6.4) reflect this contract.

### Added (R8, prior)

- **Full magic-stack e2e showcase** (R8 of the template-magic track). New `tests/e2e/bats/test_full_magic_stack.bats` walks the complete user workflow end-to-end: install ladder (hook + filter + alias in order, with idempotent re-installs), the headline "edit deployed → `git status` sees template-space diff" scenario, the commit-refused-on-markers-then-resolved cycle, `transform status` across the editing lifecycle, the R4→R6 hook-block upgrade path, and the alias install + actual shell sourcing. Per-PR bats files cover their phases in isolation; this file pins the integration story so a future change to one phase can't silently break the whole flow.

### Fixed

- **Test flake in `git_alias::tests`** caused by parallel cargo tests racing on `$SHELL`. The four env-driven detect/resolve cases are now consolidated into a single `shell_detection_env_driven_cases` test with labelled sections, so cargo's parallel runner can't interleave them. No coverage lost — same scenarios, serial.

### Added (R7, prior)

- **`dodot transform status`** (R7 of the template-magic track). Read-only view of every cached preprocessed file with its current state (`synced` / `input_changed` / `output_changed` / `both_changed` / `missing_source` / `missing_deployed`). Always exits 0 — informational, not actionable. Useful as a "what's currently out of sync?" check before deciding whether to run `dodot transform check`.
- **`dodot git-show-alias [--shell <bash|zsh>]`** (R7). Prints the Tier 2 shell alias `alias git='dodot refresh --quiet && command git'` in a copy-paste-ready block. No filesystem mutation. Auto-detects shell from `$SHELL`; `--shell` overrides. Reports "already installed" when the rc file already carries the managed block.
- **`dodot git-install-alias [--shell <bash|zsh>]`** (R7). Writes the Tier 2 alias to the user's shell rc file (`~/.bashrc` or `~/.zshrc`) with an idempotent guard block, mirroring the pre-commit hook installer. Outcomes: Created / Appended / AlreadyInstalled / Updated. Surfaces the `source <rc>` command the user needs to pick it up immediately.

### Added (R6, prior)

- **Template clean filter — `dodot template clean --path <path>`** (R6 of the template-magic track). The piece that makes `git status` and `git diff` show the truth between commits. Git invokes this filter when reading any working-tree file matched by `*.tmpl filter=dodot-template`; the filter looks up the cached baseline (R1), and on a deployed-side edit it rehydrates the cached marker stream and runs burgertocow + diffy to emit a patched template — without re-rendering, so secret-provider auth is never re-triggered. Fast path (no edit) echoes stdin in microseconds. Slow path lands the patched form (or a conflict block, surfaced inline) so the next `git diff` sees the template-space change.
- **`dodot template install-filter`** (R6). Registers the `[filter "dodot-template"]` block in the dotfiles repo's `.git/config` (clean → `dodot template clean --path %f`, smudge → `cat`, required → `true`). Idempotent. Surfaces the matching `.gitattributes` line for the user to commit. First-deploy prompt offers it after the user has accepted the pre-commit hook.
- **`template.install_filter` prompt-catalog entry**. Documents the new prompt alongside the existing ones in `dodot prompts list`.
- **Hook upgrade path** (R6). The pre-commit hook now runs `dodot refresh --quiet || exit 1` before `dodot transform check --strict || exit 1`. Re-running `dodot transform install-hook` on a repo with the older R4-shape block detects the stale managed block (via the same guard lines) and rewrites it in place, preserving any non-managed user content. New `Updated` outcome distinguishes this from `Created` / `Appended` / `AlreadyInstalled`.

### Added (R5, prior)

- **`dodot refresh [--quiet] [--list-paths]`** (R5 of the template-magic track). Walks the per-file baseline cache, hashes the deployed file at `<data_dir>/packs/<pack>/<handler>/<filename>`, and copies the deployed file's mtime onto the template source when the hashes differ. Why: git's stat-cache skips re-reading working-tree files when their mtime is unchanged, so a deployed-side edit to a template never surfaces in `git status` until the source mtime is bumped — refresh is the bump. `--quiet` suppresses output (for the Tier 2 shell alias `alias git='dodot refresh --quiet && command git'`); `--list-paths` prints out-of-sync source paths and exits without writing (for editor / file-watcher integrations). Exit 0 in all healthy cases. See `docs/proposals/magic.lex` §"Update Trigger Bit".
- **`Fs::modified` / `Fs::set_modified`** trait methods for reading/writing file mtimes — used by `refresh` to copy mtimes between deployed and source files.

### Added (R4, prior)

- **`dodot transform install-hook`** (R4 of the template-magic track). Writes `<dotfiles_root>/.git/hooks/pre-commit` with a guarded block that runs `dodot transform check --strict || exit 1` on every `git commit`. Idempotent (re-running detects the guard line and no-ops) and additive (preserves any existing hook content). The installed hook refuses to commit when reverse-merge has work to do or unresolved `dodot-conflict` markers remain — matching the contract R3 set up. See `docs/proposals/magic.lex` §"The Commit Tier".
- **First-template-deploy prompt.** After a successful `dodot up` that leaves at least one template baseline in the cache, dodot offers (interactively, only when stdin is a TTY) to install the pre-commit hook. Y/n/show via the existing `PromptRegistry`; new catalog entry `template.install_hook`; soft failure pattern matching the plist filter prompt — never aborts `up`, and any background failures (registry parse error, etc.) go to the debug log rather than stderr. The interactive prompt body itself prints to stderr by design (Y/n/show + the resulting confirmation/show output).

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
