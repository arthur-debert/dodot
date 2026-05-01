Design Specification: macOS Plist Support

    This document specifies how dodot manages macOS property list (plist) files. Plist support is implemented as a pair of git clean/smudge filters — the architecture sketched as Phase 2 of the Magical Git Experience [./magic.lex]. It is *not* a preprocessor in the sense of [./preprocessing-pipeline.lex], and intentionally so: the trade-offs that make the pipeline-and-pre-commit-hook approach right for templates make it the wrong shape for plists. The reasoning is in §2.

    Plists are where macOS GUI applications store their preferences. Bringing them under dodot's control means a dotfiles repo can carry not just shell and config-file state but also GUI-app state — Finder settings, Terminal profile, Xcode preferences, per-app panes — and still deliver the review/diff/cherry-pick/revert workflow users expect from plain-text dotfiles.

1. The Problem

    1.1. Plist Is Everywhere on macOS

        Every macOS GUI application — system settings panes, first-party apps, third-party apps — stores its preferences as a plist, most commonly at:

            ~/Library/Preferences/<bundle-id>.plist
            ~/Library/Application Support/<app>/Preferences/*.plist

        Since macOS 10.9 (2013), plists ship in a binary format by default. Every change the user makes through a GUI settings pane writes that binary.

    1.2. Binary Defeats the Point of Source Control

        A committed binary plist "works" in the narrow sense: `git checkout` restores a bit-identical file, apps read it, settings come back. But every other property source control exists to provide is lost:

            - `git diff` shows hex noise
            - Cherry-picking a single setting is impossible
            - Code review of a settings-only change is blind
            - Reverting one app's preferences without disturbing others requires manual byte-splicing

        The dotfiles repo becomes a backup, not a history.

    1.3. Apps Drift Continuously

        Unlike templates — where the user has a single explicit editing moment ("I opened `config.toml.tmpl` in vim") — plists drift continuously and silently. macOS apps rewrite preferences on settings changes, on quit, sometimes on launch. A plist that was clean five minutes ago can be dirty now, with no user-visible action.

        This shapes the right architecture. Whatever mechanism surfaces drift back to git must do so passively, on every `git status`, not only at commit time. Otherwise users will go days or weeks unaware that their committed source is stale.

    1.4. The Goal

        Bring macOS GUI app settings under the same discipline as plain-text dotfiles:

            - Source-controlled form: human-readable XML, with a canonical key ordering so diffs reflect *settings* changes, not encoder noise
            - Deployed form: binary at the location the app expects
            - User interaction: edit through the normal macOS UI, or edit the XML directly
            - Drift visibility: `git status` and `git diff` reflect the truth at all times, with no dodot-specific commands required

2. Architecture: Clean/Smudge Filters

    2.1. The Mechanism

        Plist support uses git's native `clean` and `smudge` filter machinery. The relationship between the working tree, the git index, and the deployed location is:

        Layout:

            <pack>/<x>.plist                  # working tree: BINARY (what apps read)
            git index entry for <x>.plist     # canonical XML (what `git diff` shows)
            ~/Library/Preferences/<x>.plist   # symlink → <pack>/<x>.plist

        :: text ::

        The working-tree file is the binary. The deployed file is the same binary, reached via a normal dodot symlink. The XML form exists only inside git's object database, materialised by the clean filter on `git add` / `git status` / `git diff`, and rematerialised as a binary by the smudge filter on `git checkout`.

        Filter behaviour:

            clean   (binary -> canonical XML):  invoked when git reads a working-tree file for staging or comparison
            smudge  (XML -> binary):            invoked when git writes a working-tree file from the index

        :: text ::

    2.2. Why This Shape

        Clean/smudge is uniquely suited to representational transforms whose reverse path is lossless and deterministic. Plists check both boxes:

            - `plutil` (and the `plist` Rust crate) round-trip XML and binary without information loss.
            - With canonical key ordering applied (§4), the reverse output is byte-stable: the same binary always produces the same XML.

        This combination is what unlocks "git status tells the truth for free." When the app writes the deployed binary, the working-tree file's mtime updates. Git's next `status` invokes the clean filter, gets fresh canonical XML, compares against the index, and reports the diff. No dodot command runs. No pre-commit hook is required for visibility.

    2.3. Why Not the Preprocessing Pipeline

        An earlier draft of this document specified plist support as a Representational preprocessor under [./preprocessing-pipeline.lex], with reverse conversion driven by `dodot transform check` invoked from a pre-commit hook. That approach was sound for uniformity with templates, but wrong for plists in three ways:

            1. **Drift visibility lag.** A pre-commit hook only fires when the user runs `git commit`. Between commits, `git status` says clean even when the deployed binary has been rewritten by the app. Given §1.3, this gap can stretch to days. Clean/smudge closes it.
            2. **Architectural overkill.** The pipeline's reverse-merge framework exists to handle generative transforms with ambiguous reversal (templates with conflict markers, secret-line sidecars). Plists need none of that — the reverse is exact. Routing them through the pipeline imports machinery they will never exercise.
            3. **Extra files for no benefit.** The pipeline model puts the rendered binary in the datastore (`<state>/dodot/packs/<pack>/symlink/<x>.plist`) and the source XML in the pack. That is three on-disk artefacts per plist. The clean/smudge model has two: pack working-tree binary and the deployed symlink. The datastore is unused.

        Templates still belong in the pipeline. Their reverse is generative, their rendering can touch secret-providers (auth-fatigue under clean/smudge would be hostile), and their reverse-merge needs human review. Plists have none of those properties; they're the textbook clean/smudge case.

    2.4. The Working-Tree-Binary Trade-Off

        The cost of this architecture, stated plainly: the file in the pack is binary. `cat pack/com.app.plist` shows noise. A user editing the pack directly with an editor will see binary garbage.

        What is *not* lost:

            - `git diff`, `git show <ref>:<path>`, `git log -p` all show XML — that is what git stores.
            - `git status` shows whether the binary differs from the canonical XML in the index.
            - PR reviews show XML diffs.
            - The user authors plists by editing through the app's normal GUI, or by editing the deployed file with a plist editor — they almost never want to hand-edit the file in the pack.

        For users who do want a hand-editable XML in the pack, the recommended workflow is `plutil -convert xml1 pack/com.app.plist`, edit, `plutil -convert binary1 pack/com.app.plist`. dodot does not ship a sugar command for this; it's a one-liner against `plutil`, and surfacing it as a dodot command would imply a workflow that should remain rare.

3. Engine

    3.1. The `plist` Rust Crate

        Format conversion uses the `plist` crate (https://crates.io/crates/plist). It parses both XML and binary plists and re-serialises in either format from the same in-memory representation. This is the only third-party dependency needed for the conversion path; `plutil` is not required at runtime.

        Why not shell out to `plutil`:

            - `plutil` has no flag to canonicalise key order. Sorting (§4) needs to happen between parse and serialise, which means an in-process AST regardless. Once the AST is in process, calling `plutil` adds a subprocess for nothing.
            - The `plist` crate works on every platform. `plutil` is macOS-only. Tests can run on Linux CI; Linux machines that happen to have plists in a synced repo can still smudge them.
            - One code path for both directions is simpler than mixing subprocess and library calls.

        `plutil` remains useful as a debugging tool the user invokes by hand. dodot does not depend on it.

    3.2. Filter Subcommands

        The clean and smudge filters are exposed as two thin subcommands on the dodot CLI:

            dodot plist clean   # stdin: binary,  stdout: canonical XML
            dodot plist smudge  # stdin: XML,     stdout: binary

        :: text ::

        Both read from stdin and write to stdout, the contract git's filter mechanism expects. Both are pure functions of their input — no environment, no config, no filesystem access — which makes them trivially testable and safe to run in arbitrary git contexts (rebase, cherry-pick, archive, etc.).

4. Canonical XML Form

    A representational transform that produces non-deterministic output is useless: every `git status` would invent a diff. The clean filter must emit byte-stable XML for any given binary input.

    4.1. Sources of Non-Determinism

        Three things can shuffle on round-trip if not actively controlled:

            - **Top-level dictionary key order.** Binary plists store dict keys in encoder-defined order. macOS rewrites can shuffle keys with no semantic change.
            - **Nested dictionary key order.** Same problem, recursively.
            - **Whitespace and serialiser quirks.** Indent width, trailing newlines, attribute ordering on tags.

        Arrays are *not* a source of non-determinism. Array order is semantically meaningful in plists — `LSHandlers`, ordered toolbar items, recent-files lists — and must be preserved verbatim.

    4.2. Canonicalisation Rules

        The clean filter applies, in order:

            1. Parse binary input via the `plist` crate.
            2. Walk the AST. For every `Dictionary` node (top-level or nested), sort its entries by key, lexicographically (Unicode codepoint order, stable).
            3. Re-serialise as XML with fixed formatting: tab indent, LF line endings, no trailing whitespace, no XML declaration variation.
            4. Emit on stdout.

        Arrays are walked into (their elements may contain dicts that need sorting) but their own order is preserved.

        The smudge filter is the inverse and does not need to canonicalise — XML in, binary out, any valid XML accepted.

    4.3. Determinism Test

        A round-trip test (binary → clean → smudge → clean) must produce the same XML twice. This is the simplest property test that catches every form of non-determinism in one shot, and is wired into the unit suite from day one.

5. Filter Installation

    Filters live in `.git/config` and `.gitattributes`. Splitting responsibility:

    5.1. `.gitattributes` (in the repo)

        The dotfiles repo carries a `.gitattributes` entry binding `*.plist` files to the `dodot-plist` filter:

            *.plist filter=dodot-plist

        :: text ::

        This file is committed and travels with the repo. Every clone gets the binding for free. Without the matching `filter.dodot-plist.*` entries in `.git/config`, however, git falls back to the identity filter (no transform) — so `.gitattributes` alone is harmless but inert.

    5.2. `.git/config` (per clone, per machine)

        The active filter is registered in the local `.git/config`:

            [filter "dodot-plist"]
                clean  = dodot plist clean
                smudge = dodot plist smudge
                required = true

        :: text ::

        `required = true` means git aborts loudly if the filter binary is missing or fails — preferable to silently storing or checking out the wrong representation.

    5.3. The Install Flow

        Filter registration is a one-time, per-clone, per-machine step. dodot exposes:

            dodot git-install-filters     # writes the [filter "dodot-plist"] block to .git/config
            dodot git-show-filters        # prints what would be written, for inspection or manual install

        :: text ::

        On the first `dodot up` of a pack containing `*.plist` files, dodot checks whether the filter is registered. If not, it prompts:

            dodot detected plist files in pack `mac-defaults`.
            Plist support uses git filters to keep the source diffable.
            Install filters now? [Y/n/show]

              y    run `dodot git-install-filters`
              n    skip (and ask again next time)
              show print the config block for manual install

        :: text ::

        One Y/n per machine. The `magic.lex` document specifies the same install flow as the entry point for templates' Phase 3 clean filter; both reuse this command, so a user who has done the prompt once is set up for both.

    5.4. Filter Discovery

        `dodot plist clean` and `dodot plist smudge` are subcommands of the same `dodot` binary the user already has on their PATH. No extra installation. If `dodot` is not on PATH at git-filter-invocation time (rare, but possible in restricted shells or hooks), `required = true` makes the failure loud and recoverable.

6. Deployment and Drift Flow

    6.1. First `dodot up` for a Pack

        For each `*.plist` in the pack:

            1. The smudge filter has already materialised the working-tree file as a binary at clone time (assuming filters were installed). If filters were not installed at clone, the working-tree file is XML; dodot up detects this, prompts the user to install filters, and re-runs `git checkout -- <path>` to smudge.
            2. The symlink handler links `<pack>/<x>.plist` into the destination (e.g., `~/Library/Preferences/<x>.plist`).
            3. The app sees a normal binary plist at its expected location.

        No datastore involvement. No virtual matches. No collision detection beyond what the symlink handler already does.

    6.2. App Modifies Settings

        Standard macOS behaviour: the app writes a new binary plist at the deploy location. The symlink resolves to the working-tree file in the pack. The working-tree file's bytes change; its mtime updates.

        From git's perspective:

            1. `git status` is invoked (by the user, by an editor's git gutter, by lazygit, etc.).
            2. Git compares the working-tree file's mtime to its index entry. They differ, so git runs the configured filter on the working-tree file.
            3. The clean filter emits canonical XML.
            4. Git compares the resulting XML to the canonical XML stored in the index. If they differ, the file is reported as modified.
            5. `git diff` shows the XML diff.

        The user reviews, runs `git add <path>`, and commits. The committed object is canonical XML. No `dodot` command was involved in surfacing the change.

    6.3. Cross-Machine Sync

        On another machine:

            git pull
            dodot up

        :: text ::

        `git pull` invokes the smudge filter on any updated `.plist` files, materialising the new binary in the working tree. `dodot up` ensures the symlinks are in place. Running apps may need `killall cfprefsd` to see the new values (see §7.1).

    6.4. cfprefsd Caching

        macOS's `cfprefsd` caches plist values in memory. Writing to the working-tree binary — even via the deploy symlink — may not be picked up by a running app until cfprefsd is restarted or the app re-reads its preferences.

        Baseline behaviour: document the caveat. Users run `killall cfprefsd` when immediate visibility is required. A future ergonomics pass may detect plist deployments and offer to invalidate cfprefsd automatically; not in scope for the initial implementation.

7. Adopt Flow

    A user with existing settings in `~/Library/Preferences/com.app.plist` brings them into dodot via the existing adopt command:

        dodot adopt --into <pack> ~/Library/Preferences/com.app.plist

    :: text ::

    `--into` is required for `~/Library/...` sources because plist filenames are typically reverse-DNS bundle IDs (`com.colliderli.iina.plist`), which don't make useful pack names. Pack inference declines and the caller picks the pack name explicitly.

    With clean/smudge in the picture, adopt is simpler than it would be under the pipeline model:

        1. Move the binary into the pack at `<pack>/_lib/Preferences/com.app.plist`. The `_lib/` prefix is the resolver's Priority 2d encoding for `~/Library/...` paths (see [./macos-paths.lex] §4) — adopting under it preserves the round-trip on `dodot up` without requiring per-file `[symlink.targets]` entries.
        2. Symlink back: `ln -s <pack>/_lib/Preferences/com.app.plist ~/Library/Preferences/com.app.plist`.
        3. Print a `tip:` line pointing at `dodot git-install-filters` if filters are not yet registered.

    No extension renaming. No XML emission at adopt time. The first `git add` after adopt invokes the clean filter and produces canonical XML for the index.

    7.1. Other `~/Library/` Subtrees

        The same inference applies to every `~/Library/<subdir>/<file>` path. `~/Library/LaunchAgents/com.example.foo.plist` adopts to `<pack>/_lib/LaunchAgents/com.example.foo.plist`; `~/Library/Fonts/MyFont.otf` to `<pack>/_lib/Fonts/MyFont.otf`. The Library inference is gated on `cfg!(target_os = "macos")` so non-macOS hosts produce `UnrecognizedRoot` instead of plans that would warn-and-skip at deploy.

    7.2. Sandboxed Apps

        macOS sandboxed apps write to `~/Library/Containers/<bundle-id>/Data/...` rather than the canonical location. As specified in [./macos-paths.lex] §7.8, `dodot adopt` refuses sources under `~/Library/Containers/`. Plist support inherits this refusal — the container path's contents are not intended for external editing, and the same refusal text applies whether or not the source happens to be a plist.

8. Configuration

    8.1. Schema

        Plist support has minimal configuration. The relevant block lives under `[symlink]` because plists deploy via the symlink handler:

            [symlink]
            plist_extensions = ["plist"]   # filename suffixes treated as plists for adopt hints

        :: toml ::

        The extension list controls *adopt hints and prompts*, not deployment. Deployment is governed by the symlink handler and the file's location, the same as any other file. The list exists so that adopt can produce plist-aware prompts ("install filters?") and so future advisory layers (e.g. linting hints) have a place to hang off.

        There is no `[preprocessor.plist]` section. Plists are not preprocessed.

    8.2. Inheritance

        Follows the existing 3-layer hierarchy (compiled defaults < root .dodot.toml < pack .dodot.toml). A pack that contains no plists is unaffected by any of this — the filter only fires on tracked `*.plist` files, governed by `.gitattributes`.

9. User Workflow

    9.1. First-Time Setup

        1. User runs `dodot adopt <pack> ~/Library/Preferences/com.app.plist` (or drops a plist into a pack manually).
        2. `dodot up` deploys the symlink and prompts to install git filters if needed.
        3. User runs `dodot git-install-filters` (or chooses `y` at the prompt).
        4. The app runs normally, modifying the binary live.

    9.2. Day-to-Day

        User runs normal git commands. `git status` reflects whether app-driven settings changes are pending. `git diff` shows XML. `git add` and `git commit` proceed with no extra steps. There is no dodot-specific command in the daily loop.

    9.3. Cross-Machine Sync

        `git pull` followed by `dodot up`. The smudge filter handles XML→binary; dodot ensures the symlink is in place. `killall cfprefsd` if immediate visibility is needed.

10. Implementation Phases

    Plist support ships as an independent track. It does *not* depend on the preprocessing pipeline; it can land before, after, or in parallel with that work.

    Phase P1: Conversion Engine
        - `plist` crate dependency
        - Parser/serialiser pair with recursive dict-key sort
        - Determinism property test (binary → clean → smudge → clean)
        - `dodot plist clean` and `dodot plist smudge` subcommands (stdin/stdout)
        - Unit tests covering: nested dicts, arrays preserved, mixed scalars, edge cases (empty plist, plist with binary data values)

    Phase P2: Filter Installation
        - `dodot git-install-filters` writes the `[filter "dodot-plist"]` block
        - `dodot git-show-filters` prints the config without writing
        - On `dodot up`, detect tracked `*.plist` files and prompt to install if filters are unregistered
        - `.gitattributes` template emitted by `dodot init` (or merged into existing `.gitattributes`)

    Phase P3: Adopt
        - Extend `dodot adopt` to recognise binary plists at the source
        - First-use hint pointing at filter installation
        - Tests: round-trip adopt → up → app modifies → git status shows XML diff

    Phase P4: Ergonomics
        - cfprefsd handling documentation, optional `killall` prompt after large changes
        - Error messages for missing `dodot` on PATH at filter-invocation time
        - `dodot probe app` integration: when probing a pack, surface plist-related cask/preferences associations

11. Open Questions and Future Work

    11.1. Defaults Import for cfprefsd

        Writing directly to the file on disk works but does not invalidate cfprefsd's cache. A future extension may opt into `defaults import <domain> <file>` for deployments where bundle-id-to-file mapping is unambiguous, which goes through the proper APIs and invalidates the cache correctly. Not in scope for the initial implementation; called out so the design space is recorded.

    11.2. Hand-Editable XML in the Pack

        Some users may prefer the source-of-truth pack file to be XML, accepting that the deployed binary is generated at deploy time. That is the architecture this document explicitly rejects in §2.3, but it is a coherent alternative for a user who never wants binary in their working tree. If the demand is real, it could be supported as a per-pack opt-in (`[pack] plist_storage = "xml"`), routing through the preprocessing pipeline for those packs only. Not implemented; tracked here as a known alternative.

    11.3. Non-`.plist` Extensions

        Some apps store plists with non-standard suffixes (`.savedState`, `.mobileconfig`, etc.). The `plist_extensions` config key (§8.1) is the extension point for this; the default list covers the common case.

    11.4. Plist Subset of `_lib/Preferences/`

        [./macos-paths.lex] §11.3 notes that `_lib/Preferences/<bundle-id>.plist` is a syntactically valid path and that this proposal covers the format-management half. The two compose: the `_lib/` prefix routes the file to `~/Library/Preferences/`; the clean/smudge filter handles the binary↔XML translation. No further coordination is required.
