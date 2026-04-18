Design Specification: macOS Plist Support

    This document specifies how dodot manages macOS property list (plist) files. It is a concrete implementation of a Representational (Two-Way) transformation as defined in the Preprocessing Pipeline design [./preprocessing-pipeline.lex].

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

    1.3. The Goal

        Bring macOS GUI app settings under the same discipline as plain-text dotfiles:

            - Source-controlled form: human-readable XML
            - Deployed form: binary at the location the app expects
            - User interaction: edit through the normal macOS UI, or edit the XML directly
            - Git history: a meaningful record of which settings changed, when, and why

        The user treats plists like any other dotfile. The preprocessing pipeline handles the format juggle.

2. Pipeline Classification

    2.1. Representational Transform

        Plist conversion is a Representational (Two-Way) transform per preprocessing-pipeline.lex section 2.2:

            - XML and binary are different representations of the same data
            - `plutil` converts losslessly in both directions
            - The reverse path is exact: no heuristics, no ambiguity, no user-facing conflict markers

        This is the cleanest preprocessor case. Contrast with template expansion, where reverse-merge requires heuristics because the transform is generative and loses information.

    2.2. Source of Truth

        Unlike templates — where the `.tmpl` in the repo is the authoritative source — plists have a split source of truth:

        The live binary at `~/Library/Preferences/...`:
            Authoritative during use. Apps modify it continuously (window positions, recent files, last-used tabs, as well as user-meaningful settings).

        The XML in the repo:
            Git-friendly mirror. Canonical only at commit time.

        This matters operationally: dodot cannot assume the repo is always up-to-date. `dodot transform check` before each commit is what reconciles the two.

3. Engine

    3.1. plutil

        `plutil` ships with macOS; no external dependency. Two invocations cover the pipeline:

            plutil -convert binary1 -o - <path>   # XML -> binary  (forward, for dodot up)
            plutil -convert xml1 -o - <path>      # binary -> XML  (reverse, for transform check)

        Both operations are lossless and deterministic. dodot shells out; no Rust-native plist library is required.

    3.2. Platform Gating

        The plist preprocessor is active only on macOS. On other platforms its `matches_file()` returns false and `expand()` is never called. The preprocessor is still registered (for config consistency and for test coverage via a mock plutil CommandRunner) but contributes nothing to non-macOS deployments.

4. Deployment Flow

    4.1. Forward: `dodot up`

        Standard preprocessing pipeline flow (preprocessing-pipeline.lex section 3.1):

            1. Scan identifies `com.app.plist.xml` in a pack
            2. The plist preprocessor strips `.xml`: expanded filename is `com.app.plist`
            3. `expand()` invokes `plutil -convert binary1` on the XML source
            4. Binary output is written to the datastore as a regular file
            5. Virtual RuleMatch for `com.app.plist` enters the normal pipeline
            6. The symlink handler links it into the destination (e.g., `~/Library/Preferences/com.app.plist`)

        The app sees a normal binary plist at its expected location.

    4.2. Reverse: `dodot transform check`

        Because the transform is Representational, reverse is automatic:

            1. Divergence detection finds that the deployed binary's hash differs from the baseline
            2. The preprocessor's `contract()` invokes `plutil -convert xml1` on the deployed binary
            3. The resulting XML overwrites the source file in the working tree
            4. `dodot transform check` reports what changed and (as a pre-commit hook) blocks the commit so the user can review the updated XML

        No conflict markers are ever produced. The reverse is exact, so any deployed state can be round-tripped back to a committable XML form.

    4.3. cfprefsd Considerations

        macOS's `cfprefsd` caches plist values in memory. Writing directly to the file on disk — as dodot does via symlink — may not be picked up by a running app until cfprefsd is restarted or the app re-reads its preferences.

        Baseline behavior: document the caveat and let the user run `killall cfprefsd` when immediate visibility is required. A future extension may opt into `defaults import <domain> <file>` for deployments where bundle-id to file mapping is unambiguous, which goes through the proper APIs and invalidates the cache correctly.

5. Git Integration

    5.1. Shared Mechanism With Template Expansion

        Plist support reuses the git-integration layer specified in Template Expansion [./template-expansion.lex]:

            - `dodot transform install-hook` installs a pre-commit hook that calls `dodot transform check` before every commit
            - `dodot transform check` iterates over all preprocessed files and invokes the appropriate reverse behavior per transform type
            - The user's git workflow is unchanged: `git add`, `git commit`, and the hook keeps XML sources in sync with whatever the apps wrote to the deployed binaries

        One setup step enables both templates and plists. A mixed dotfiles repo gets the same ergonomics for both.

    5.2. What's Simpler Than Template Expansion

        Where template expansion's pre-commit behavior runs heuristics (static-vs-dynamic line classification, conflict-marker insertion, user review), plist's pre-commit behavior is a direct conversion:

            - No line-level reasoning
            - No user-facing conflicts
            - No `no_reverse` opt-out needed

        The entire Phase-2 reverse-merge framework from template expansion collapses to one `plutil -convert xml1` call per diverged file. This is the payoff of being a Representational transform: the "magic" that makes the user experience seamless is cheap to implement because the math is on our side.

6. Adopt Flow

    A user with existing settings in `~/Library/Preferences/com.app.plist` brings them into dodot via the existing adopt command, extended to recognize binary plists:

        dodot adopt <pack> ~/Library/Preferences/com.app.plist

    The extended adopt:

        1. Reads the existing binary
        2. Invokes `plutil -convert xml1` to produce XML
        3. Writes `<pack>/com.app.plist.xml` into the dotfiles repo
        4. Moves the binary into the datastore and creates the symlink back to `~/Library/Preferences/` (standard adopt behavior)

        From then on, the file participates in the normal pipeline: XML in git, binary deployed, round-trip via `transform check`.

7. Configuration

    7.1. Schema

        [preprocessor.plist]
        enabled = true                        # default true on macOS, false elsewhere
        extensions = ["plist.xml"]            # only files ending in .plist.xml are preprocessed

    7.2. Inheritance

        Follows the 3-layer hierarchy (compiled defaults < root .dodot.toml < pack .dodot.toml) like all dodot configuration. A pack that contains no plists can set `[preprocessor.plist] enabled = false` to skip the preprocessor entirely for that pack.

8. User Workflow

    8.1. First-time Setup

        1. User drops `com.app.plist.xml` into a pack (or runs `dodot adopt`)
        2. `dodot up` converts XML to binary and deploys
        3. `dodot transform install-hook` installs the pre-commit hook
        4. The app runs normally, modifying the binary live

    8.2. Day-to-day

        User runs normal git commands. Before each commit the hook runs `dodot transform check`, which reverse-converts any divergent plists. The XML changes are included in the commit and `git diff` shows a readable diff of what settings moved.

    8.3. Cross-machine Sync

        On another machine:

            git pull
            dodot up

        The pulled XML is converted to binary and deployed. Running apps may need `killall cfprefsd` to see the new values (see 4.3).

9. Implementation Phases

    Plist support depends on Phase 1 of the preprocessing pipeline (core pipeline infrastructure).

    Phase P1: Core Conversion
        - PlistPreprocessor implementing the Preprocessor trait (Representational)
        - `plutil -convert binary1` for `expand()`
        - macOS platform gating
        - Basic deployment via the pipeline
        - Unit tests using a mock plutil CommandRunner

    Phase P2: Reverse Conversion
        - `contract()` implementation (`plutil -convert xml1`)
        - Integration with `dodot transform check` for automatic reverse
        - `dodot transform status` output for plist files

    Phase P3: Adopt
        - `dodot adopt` support for existing binary plists

    Phase P4: Ergonomics
        - cfprefsd handling documentation and/or `defaults import` mode
        - Error messages for common issues (plutil not found, malformed XML, permission denied on ~/Library/Preferences)
        - First-use onboarding hint pointing at `dodot transform install-hook`
