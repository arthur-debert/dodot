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

### Fixed

- `dodot config gen` and `dodot config schema` panicked at startup
  with `Long option names must be unique for each argument, but
  '--output' is in use by both 'output' and '_output_mode'`. The
  collision was between standout's global `--output` (output-mode
  selector) and clapfig's gen/schema output-file flag. Renamed the
  latter to `--out`; short `-o` is preserved, so `dodot config gen
  -o .dodot.toml` keeps working and `--out` replaces `--output` as
  the long form.
