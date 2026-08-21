:: verified ::
The homebrew handler

Runs `brew bundle` against your source `Brewfile` once per content-hash, tracked by a sentinel. Functionally a specialization of the install handler with a more ergonomic default for the common case: "install these packages on every machine I use."

1. Default claim

    A source file named `Brewfile` at the pack root. Single-string match — the homebrew handler claims one Brewfile per pack.

    dodot does not gate this handler by OS. Homebrew is macOS-first but not macOS-only, and the three provisioning handlers are best read as a ladder rather than as three equivalent options:

    - *brew, where its prerequisites are acceptable.* Homebrew runs on Linux, and where it does, it is the same `Brewfile` you already have — one manifest across all your machines. The prerequisites are real, though: a system compiler and build tools, and on older distributions brew brings along its own gcc and glibc. That is a large dependency to accept on a machine you do not fully control, like a shared build box or a work VM.
    - *nix, where they are not.* [./nix.lex] covers the same need — a declarative per-pack package manifest — without leaning on the host toolchain.
    - *`install.sh`, where neither fits.* A package the system's own package manager should own, or a one-off setup step. It is a script, so it can call `apt`, `dnf`, `pacman`, or anything else the host ships. See [./install.lex].

    A pack can carry more than one rung and choose per platform with a gate — a filename token (`Brewfile._darwin`, `packages._linux.nix`), a `_linux/` directory segment, or a `[mappings.gates]` glob. A gated-out file is visible in `dodot status` and deploys on a matching host; the full surface is in [./../conditional-running.lex]. `[pack] os` in the pack's `.dodot.toml` gates the whole pack instead of one file.

2. When brew is not installed

    Before it plans anything for a `Brewfile`, dodot looks for `brew` itself, at a fixed list of paths, in order:

        $HOMEBREW_PREFIX/bin/brew             # only when the variable is set and non-empty
        /opt/homebrew/bin/brew                # Apple silicon macOS
        /home/linuxbrew/.linuxbrew/bin/brew   # Linuxbrew, system install
        /usr/local/bin/brew                   # Intel macOS
        ~/.linuxbrew/bin/brew                 # Linuxbrew, per-user install

    :: text ::

    The first of those that is a regular file carrying an execute bit wins, and *that exact path* is what the run spawns — not whatever `brew` your `PATH` happens to resolve later. dodot never runs brew to find out whether brew is there: the check is a `stat` and a look at the mode bits, which is what lets `dodot status` ask the same question without spawning anything or touching your machine. It is the same prefix list dodot's shell bootstrap uses, so a host whose brew dodot puts on your `PATH` is a host whose `Brewfile` dodot will run.

    When none of those paths holds an executable, dodot plans nothing for the file. No command runs, no sentinel is written, and it is not an error: `dodot up` exits 0, and the rest of the pack deploys exactly as it would otherwise — symlinks, `$PATH` entries, and shell init are untouched. The `Brewfile` still gets its own row in `dodot up` and `dodot status`, styled `skipped` and labelled:

        homebrew not installed

    :: console ::

    with a warning footnote naming every location that was probed, in probe order, and what to do:

        homebrew is not installed — probed /opt/homebrew/bin/brew,
        /home/linuxbrew/.linuxbrew/bin/brew, /usr/local/bin/brew,
        /Users/you/.linuxbrew/bin/brew. Nothing was recorded, so installing homebrew
        (https://brew.sh) and re-running `dodot up` runs this file.

    :: console ::

    "Nothing was recorded" is the sentence that matters. Absence is a fact about this machine right now, never a receipt: because dodot wrote no sentinel, installing brew and re-running `dodot up` runs the file — no flag, no stale state to clear first. The project page is the only install instruction dodot gives; it does not run a third-party installer and does not offer to.

    2.1. When a receipt already exists

        Absence does not rewrite history. If the `Brewfile` ran on this machine and brew was uninstalled afterwards, `dodot status` keeps the row's normal `installed` verdict and adds a warning footnote instead:

            homebrew is not installed (probed /opt/homebrew/bin/brew, …) — the receipt stands:
            Brewfile ran when it was

        :: console ::

        And if the receipt is for older content — the `older version` state of section 4 — the row stays stale and the remedy says why the usual fix will not fire:

            ran an older version of Brewfile — run `dodot up --provision-rerun` to apply the
            current one — homebrew is not installed (probed /opt/homebrew/bin/brew, …), so that
            re-run skips until it is back

        :: console ::

        That is the honest version: `--provision-rerun` against an absent brew skips the file exactly as a plain `up` would. Install brew first.

    2.2. When dodot cannot look

        A probe that fails for a reason other than "not there" — a permission error on `/opt/homebrew`, an I/O failure — is a different event, and dodot reports it as one. The row is styled `broken`, labelled `cannot probe homebrew`, with an error footnote:

            could not check whether homebrew is installed: /opt/homebrew/bin/brew (permission
            denied). Nothing was run for this file.

        :: console ::

        Unlike absence, this fails the run: `dodot up` reports "Packs deployed with errors." and exits 1, because the question dodot was asked went unanswered and its report about that file is not to be trusted. It also outranks any receipt — a `Brewfile` with a current sentinel still renders this row, rather than claiming a state dodot could not verify. The rest of the pack deploys either way.

3. Sentinels

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `Brewfile-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source Brewfile's bytes. Alongside it dodot also writes a sibling file `<filename>-<checksum>.snapshot` containing the Brewfile bytes as they were at the time of that run, so a future `dodot status` can show what changed.

    Same flag set as install:

    - `--no-provision` — skip every code-execution handler entirely on this run: install, homebrew, nix, and external. The skipped files still get a row, labelled `skipped (--no-provision)`, so a run you asked to be partial doesn't look like a pack dodot found nothing in.
    - `--provision-rerun` — the canonical "apply pending content edits" escape hatch for the run-once handlers: install, homebrew, and nix. Re-executes them even when a sentinel exists. Use it after editing the Brewfile to opt back into running the new content.
    - `--force` — overwrite pre-existing files at symlink target paths. Distinct from `--provision-rerun`; does **not** trigger run-once re-execution.

4. Editing a Brewfile after it ran (the three states)

    When you edit your `Brewfile` after a successful run, dodot does **not** re-run `brew bundle` automatically. The conservative posture is to *notify* and let you decide.

    `dodot up` and `dodot status` report one of three states for the Brewfile:

    - **`brew packages not installed`** — no sentinel exists. `dodot up` will run `brew bundle` on the next invocation.
    - **`installed`** — a sentinel exists for the *current* content hash. The bundle has run, and the source hasn't changed since. `dodot up` is a no-op.
    - **`brew packages older version (N lines added, M removed)`** — a sentinel exists, but for a *different* content hash. The bundle ran successfully against an earlier version of the Brewfile, and you've edited it since. `dodot up` does not auto-rerun. To apply the edits, run `dodot up --provision-rerun`.

    Section 2 covers what each of these three looks like when brew itself is missing.

    For sentinels written before the snapshot convention was introduced, the third state shows `brew packages older version (no diff data)` — the run state is still tracked, but dodot has no record of the prior content to summarize what changed. Manual `brew uninstall` of packages the Brewfile still lists likewise stays sticky: the sentinel records "we ran with this content," and dodot considers the work done until the file changes or `--provision-rerun` is passed.

    To inspect the actual diff before deciding to re-run:

        dodot status --diff           # all packs
        dodot status dev --diff       # one pack

    For each `older version` entry, `--diff` prints a unified diff between the snapshot (the bytes that were last successfully run) and the current source.

    Snapshots live alongside sentinels in the handler data dir: `<datastore>/packs/<pack>/homebrew/<filename>-<hash>.snapshot`. If you want to manage state directly, removing the sentinel + snapshot pair flips the file back to `brew packages not installed`.

5. How dodot invokes brew

    The command is `brew bundle --no-upgrade --file <your Brewfile>`, with `HOMEBREW_NO_AUTO_UPDATE=1` set for that invocation only. Both turn off a brew default that would do work you did not ask for in this run:

    - *No upgrades.* `brew bundle` upgrades every outdated formula it encounters by default. dodot passes `--no-upgrade`, so a run installs what the Brewfile declares and leaves the rest of your machine's packages at the versions they were. Upgrading stays something you ask for, with `brew upgrade`.
    - *No auto-update.* The first brew invocation of a day normally runs a `brew update` first — a network round-trip that upgrades brew and its taps and takes seconds before your packages get a look in. dodot suppresses it, so `dodot up` costs what your Brewfile costs. Updating brew stays something you ask for, with `brew update`.

    The variable is layered onto the environment dodot is running with, not substituted for it: the `brew` process still sees your `PATH`, `HOME`, and the rest of your `HOMEBREW_*` settings. The one exception is `HOMEBREW_NO_AUTO_UPDATE` itself — dodot's value wins for the bundle it runs, so a `HOMEBREW_NO_AUTO_UPDATE=0` you export does not re-enable auto-update here. Run `brew bundle` yourself if you want brew's defaults back for a particular run.

6. The version floor: dodot tells you when brew is too old

    Suppressing the auto-update has one consequence dodot then has to cover for you. A `Brewfile` may declare `go`, `cargo`, `uv`, `npm`, and `krew` entries alongside `brew` and `cask`, and brew grew support for them one at a time — `go` in 4.6.17, `cargo` in 5.0.7, `uv` in 5.0.16, and `npm` and `krew` in 5.1.2 (2026-03-30). Those entry types are the reason dodot ships no handlers of its own for the language-bound package managers: they are Homebrew's job. Brew's own auto-update would normally have carried an old installation past all of them without you noticing — but dodot turns auto-update off, so an old brew stays old. Having taken that away, dodot owes you the check.

    So before running your `Brewfile`, dodot asks `brew --version` and compares the answer against a floor of *5.1.2* — the newest of those four releases, because dodot does not read your `Brewfile` and so cannot know which entry types you wrote. Below the floor, with `brew update` as the remedy:

        dodot: homebrew at /opt/homebrew/bin/brew is 5.0.16, older than 5.1.2: not all of the
        `Brewfile` entry types go, cargo, uv, npm, and krew are supported, and one of them may
        fail to parse. Run `brew update` to update it. dodot runs this file anyway — everything
        that does not need the newer homebrew still works.

    :: console ::

    And when brew answers but the answer is not a version dodot can compare — a `Homebrew >=4.1.0 (shallow or no git repository)` from a brew that cannot read its own git history, a non-zero exit, no output at all — dodot says that instead of guessing, because telling you to `brew update` a brew whose version nobody has read would be inventing an answer:

        dodot: homebrew at /opt/homebrew/bin/brew did not report its version: reported `Homebrew
        >=4.1.0 (shallow or no git repository)`, which is a lower bound rather than a version.
        dodot cannot tell whether it is at 5.1.2, below which not all of the `Brewfile` entry
        types go, cargo, uv, npm, and krew are supported, and one of them may fail to parse.
        This run continues.

    :: console ::

    `dodot: ` is dodot's own stderr prefix — brew's output arrives unprefixed, so you can always tell which program is speaking. The warning is printed at the moment dodot asks the question, before your `Brewfile` runs, so brew's own parse error cannot arrive first. It also appears in the run's closing warnings, which is where `--output json` carries it for anything reading dodot's output as data. Nothing about it is recorded: a version is a fact about the machine at that moment, and a receipt asserts only that this exact file content ran.

    It is a report, not a refusal, and a deliberately hedged one: dodot does not read your `Brewfile`, so it knows neither which entry types yours uses nor whether the brew you have is new enough for them. The 5.0.16 above already supports `go`, `cargo`, and `uv` — only `npm` and `krew` would fail on it — and a `Brewfile` of plain `brew` lines runs perfectly on an installation far older than that. The point is that if a line does fail afterwards, you are looking at a named condition with a remedy instead of a parse error out of `brew bundle`.

    A brew at or above the floor says nothing at all. The question is asked only by `dodot up` — `dodot status` and `dodot up --dry-run` never spawn brew — and only when a `Brewfile` is actually about to run, so a pack whose receipt is already current is not asked about at all. It costs one `brew --version` per brew installation per run, however many packs declare a `Brewfile`.

    Neither nix nor install declares a floor. `nix profile install` is a years-old part of the Nix CLI, and dodot has no Nix installation to verify a floor against — an unverified floor would refuse hosts on guesswork.

7. Configuration

    Under `[mappings]`:

        [mappings]
        homebrew = "MyBrewfile"

    :: toml ::

    Single string only — unlike `install`, the homebrew handler claims one filename. There's no dedicated `[homebrew]` section.

8. Live edits

    Edits to the source Brewfile — adding or removing a `brew "..."` line, changing a `cask` — change its content hash. dodot detects the change but **does not re-run `brew bundle` automatically** — instead `dodot status` reports `brew packages older version` and `dodot up` skips it with the same notice. Apply the edits explicitly with `dodot up --provision-rerun`. See section 4 for the full three-state model and `--diff` workflow.

    `brew bundle` itself is mostly idempotent: running it with the same Brewfile installs nothing new and leaves your system as it was. So `--provision-rerun` is cheap if you want to reconfirm; the only cost is brew's own work to check each entry.

    Removing the source Brewfile from the pack stops dodot from running the bundle, but does not uninstall the packages it installed earlier — `brew bundle cleanup` is the brew-side mechanism for that, run by hand against the previous Brewfile.

    On a machine without brew, an edit changes what the row says but not what happens: nothing runs either way. If the `Brewfile` had run here before, the row still goes `brew packages older version` and the footnote adds that the re-run waits until brew is back; if it never ran here, the row stays `homebrew not installed` and your edit simply takes effect the first time you run `dodot up` on a machine that has brew. Section 2 has both.
