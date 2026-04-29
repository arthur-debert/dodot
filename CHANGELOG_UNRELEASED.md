# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

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
