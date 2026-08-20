Proposal: Package Manager Handlers

    This document generalizes dodot's package-manager support. Today `homebrew` and `nix` are two independent hand-written handlers with no shared concept between them. This proposal defines what a package manager must offer for dodot to support it at all, sorts the candidates into two tiers, adds `uv` and `npm` handlers, and reduces the per-manager surface to data.

    The organizing constraint is that dodot does not own bulk-install semantics. It does not parse manifests, reconcile versions, decide which packages are missing, or repair partial state. Where a manager cannot be driven without dodot taking on that work, dodot does not support that manager.

    Homebrew stops being a hard requirement. It becomes one supported manager among several, required only by packs that actually declare a `Brewfile`.


1. Motivation

    1.1. The Gap

        Operating-system packages increasingly live in language-specific managers. A dotfiles repo that wants `ruff`, `prettier`, and `ripgrep` on a new machine can express the third in a `Brewfile` and has nowhere to put the first two. The workaround is an `install.sh` that hand-rolls a loop over `uv tool install` or `npm install -g`, with hand-rolled idempotency, hand-rolled error reporting, and no status integration.

        That works, and it is what users do now. What it does not give is what a first-class handler gives: a receipt so the install does not re-run on every `dodot up`, a status row, a footnoted error when something fails, and a pre-flight answer to "is this manager even here?".

    1.2. What This Is Not

        This proposal does not make dodot a package manager, a dependency resolver, or a diagnostician of package-level failure. dodot can truthfully say _we ran this file, and here is what the manager said_. It cannot truthfully say which packages are present, and it will not guess.

        Trying to be more specific per manager — parsing a `Brewfile` to install only the delta, deciding a package is "already satisfied" — is an unwinnable game against the manager's own resolution logic. That position is already settled for `homebrew` and `nix` [./shipped/nix.lex]; this proposal extends it rather than revisiting it.


2. The Bar

    A package manager qualifies for a dodot handler when both hold:

    1. *One command consumes one file, and the manager owns the whole batch.* dodot passes a path and reads an exit code. It does not split, parse, or iterate over package specs itself.
    2. *The manager installs globally or per-user.* Project-local installs are useless here — at the moment dodot runs, there is no project directory. Where a globally-installing manager places its binaries is the user's configuration and the user's business.

    Clause 1 has an escape hatch, added after research showed it excludes a manager the ecosystem is actively converging on. See [#4.2].

    2.1. What The Bar Excludes, And Why That Is The Point

        The bar is doing real work: it eliminated two of the five managers investigated, including one whose exclusion is unintuitive.

        The failure mode it protects against is a manager whose batch interface looks adequate and is not. `cargo install a b c` is a single command taking multiple packages, which reads like a pass. In practice it installs `a` and `c`, skips `b`, and exits `101` — the same code as total failure, with the list of what failed available only as prose on stderr. Supporting it would mean dodot scraping that prose to tell a user which crate broke.


3. Research Findings

    Five managers were investigated against the bar. Except where noted, findings were reproduced by execution on a macOS arm64 machine rather than taken from documentation.

    Manager verdicts:
        | Manager | Verdict | Mechanism |
        | homebrew | qualifies | `brew bundle install --file=<path>` |
        | nix | qualifies, deferred | `nix-env --install --remove-all --file <path>` |
        | uv | qualifies via [#4.2] | `uv tool install -- <spec>`, one per line |
        | npm | qualifies via [#4.2] | `npm install -g -- <spec>`, one per line |
        | cargo | *fails* | no manifest, no bulk-install-from-file |

        :: table align=lll header=1 ::

    The per-manager findings follow.

    3.1. Homebrew

        `brew bundle` became a built-in core command in Homebrew 4.5.0 (2025-04-29); the `homebrew/bundle` tap is no longer needed, and a tap entry left over from an older install is not consulted. Probing for the tap is therefore a useless availability signal.

        Failure is two-phase and the exit code cannot distinguish them. A nonexistent package name aborts the entire batch during the fetch phase, before anything installs. A package that exists but fails to build lets the rest proceed, counting failures. Both exit `1`. There is no machine-readable output; `brew bundle list --json` is not a valid flag.

        `brew bundle install` *upgrades by default*. On a machine with outdated formulae, what reads as "install this Brewfile" is a long mutating upgrade of unrelated packages. See [#11].

    3.2. Nix

        There is no declarative sync for `nix profile`. Its subcommands are `add`, `diff-closures`, `history`, `list`, `remove`, `rollback`, `upgrade`, `wipe-history` — no manifest input. The profile's `manifest.json` is an internal record of what Nix installed, not a file a user authors. This did not change in 2025-2026; the standing issue is [https://github.com/NixOS/nix/issues/7965].

        The one genuine fit is the older CLI: `nix-env --install --remove-all --file <path>`. `--remove-all` is what makes it declarative — the file becomes the whole truth for the profile, atomically, including removals.

        Nix claims in this section are documentation-derived. No Nix installation was available for reproduction, and exit codes and re-run costs were not measured.

    3.3. uv

        `uv tool install` takes exactly one package. There is no `-r`, no `sync` subcommand, and no tools table in configuration. The declarative-tools request [https://github.com/astral-sh/uv/issues/12533] is open and unassigned.

        Everything else about uv is excellent for this purpose. A re-install of an already-present tool takes ~60ms, exits `0`, and works with no network at all — verified against an unreachable index. Each tool gets its own virtual environment, so a failed install cannot corrupt a successful one.

    3.4. npm

        npm has no global manifest, the two long-standing requests are closed on an archived repository, and nothing successor-shaped has shipped.

        The mechanism that superficially looks like the answer does something else. `npm install -g` with no arguments in a directory containing a `package.json` installs *that directory as a global package*, not its dependencies. Verified twice:

            $ npm install -g --dry-run
            add my-global-tools 1.0.0
            added 1 package in 207ms      EXIT=0

        :: console ::

        A real install into a scratch prefix produced `<prefix>/lib/node_modules/my-global-tools` as a symlink to the source directory, and no `<prefix>/bin` at all. The declared dependencies were nowhere.

        The mechanism is npm's linking rule: `<prefix>/bin` receives binaries only from the installed package's *own* `bin` field. A dependency's `bin` is never promoted. The trap is that the same `package.json` in *local* mode does put every dependency's binary in `<dir>/node_modules/.bin` — adding `-g` flips the meaning from "satisfy these dependencies" to "install this package".

        Homebrew hit the same wall and resolved it the same way this proposal does: its npm extension for `brew bundle`, merged March 2026, loops `npm install -g <name>` per package.

        The re-run cost was measured, because the entry test in [#4.3] turns on it. Re-installing an already-present global takes ~620ms cold and ~170ms warm — against uv's ~60ms — and npm reports it as `change <pkg> 0.10.0 => 0.10.0` rather than as a no-op. The resolution is not optional: pointed at an unreachable registry, the same already-satisfied re-install *fails*, and takes 70 seconds of retry backoff to do it. uv's no-op succeeds offline in the same situation.

    3.5. cargo

        No manifest of user-installed tools exists, and the request [https://github.com/rust-lang/cargo/issues/9527] has been open since 2021. Two findings make the naive shape worse than it appears.

        Partial failure is not atomic and the exit code is lossy, as described in [#2.1].

        Idempotency is silently broken for a class of crates. Any crate declaring a `[[bin]]` with `required-features` is considered stale by cargo permanently, and gets a full from-source rebuild on every `cargo install` — reproduced three times on a real crate, 26-37 seconds each, exiting `0` every time. This is [https://github.com/rust-lang/cargo/issues/8703], open since 2020.

        A third-party tool, `cargo-liner`, does offer a genuine one-file interface. It is not recommended here; see [#10.1].


4. Two Tiers

    4.1. Manifest PMs

        The manager defines a file format and a verb that applies it. dodot passes the path and reads the result.

        Members: `homebrew` (`Brewfile`), `nix` (`packages.nix`, deferred — see [#10.4]).

    4.2. Mapped-txt PMs

        The manager has no manifest but installs one package per invocation cleanly. dodot supplies the file format — a plain list — and loops.

        This is a deliberate narrowing of the bar's clause 1, not an abandonment of it. The clause was never about loops as such; it was a proxy for the things that make loops bad — version reconciliation, partial-state repair, guessing what is installed, scraping prose to discover what broke. Splitting a file on newlines and passing each line through does none of those.

        Members: `uv` (`uv.tools.txt`), `npm` (`npm.packages.txt`).

    4.3. The Entry Test

        A manager joins the mapped-txt tier only when both hold:

        1. *The per-item command is cheaply idempotent.* Re-running it against an already-satisfied package is fast and side-effect-free.
        2. *The exit code is trustworthy.* Zero versus non-zero reliably distinguishes success from failure for that one package.

        uv passes both cleanly. cargo fails both, which is the whole reason it is excluded: `101` means everything, and the `required-features` bug makes a nominal no-op a 30-second rebuild.

        npm passes clause 2 cleanly and clause 1 with two qualifications worth naming rather than rounding off. It is slower, and the slowness is mandatory network resolution, not caching that has not warmed up yet — offline, a re-install of an already-installed global fails outright ([#3.4]). And "side-effect-free" holds only for a pinned line: an unpinned spec re-resolves to whatever is current, so a re-run triggered by a *different* broken line in the same file can upgrade an item that already succeeded.

        npm is admitted anyway. Both qualifications are bounded and visible — they cost time, they fail loudly rather than silently, and pinning removes the second one entirely — which is a different category from cargo's failures, where the exit code lies and the rebuild is unavoidable at any pinning. The honest statement is that npm is the tier's weaker member and its re-run is not free.

        The test currently admits exactly two members and rejects the most-requested candidate. That is the intended shape. Without it, "just loop over the lines" becomes the answer for every manager, including the ones where looping is actively harmful.


5. The Mapped-txt Format

    A `<pm>.<noun>.txt` file at pack root is a list of package specifications, one per line.

    The middle word is the manager's own noun for what it installs — `uv tool install` installs tools, npm installs packages. The frame is uniform so the files read as a family; the noun is the manager's so each filename is accurate on its own terms.

    5.1. Parsing Rules

        In full:

        - A line that is empty, or contains only whitespace, is ignored.
        - A line whose first non-whitespace character is `#` is ignored.
        - Every other line is trimmed of leading and trailing whitespace, including carriage returns, and passed to the manager as *one argument*.

        That is the entire specification. dodot does not validate, normalize, or interpret the content of a line. Version syntax, pinning, extras, git URLs, and local paths are whatever that manager accepts on its own command line — if one manager writes `foo==3.2` and another writes `foo@3.2`, both are opaque to dodot.

        Lines are passed in file order.

    5.2. Two Consequences Worth Stating

        _No inline comments._ `#` starts a comment only at the beginning of a line. A trailing `# like this` becomes part of the argument and the line fails. This differs from `requirements.txt`, and Python users will expect otherwise. It is strict on purpose: deciding whether `#` may legally appear inside a package specification would be parsing.

        _No word splitting._ A line is one argument, never several. `ruff>=0.1,<0.2` passes through intact with no quoting — which the same string would require in a shell. The cost is that per-package flags cannot be expressed; that is intentional, since per-line flags differ per manager and would reintroduce exactly the per-manager specialization this design removes.

        _One argument is not the same as one package._ Passing a line as a single argument stops it being split; it does not stop it being read as an *option*. A line of `--force` is one argument and still a flag, which would quietly re-admit the per-package flags the previous paragraph just refused, and could change the installation scope. Every mapped-txt argv template therefore places the option terminator `--` before the spec: `uv tool install -- <spec>`, `npm install -g -- <spec>`. Both honour it — verified: after `--`, uv rejects `--version` as a malformed package name and npm looks up `--force` in the registry, where without the terminator uv parses it as a flag. A manager with no reliable terminator does not belong in this tier.

        Rejecting option-shaped lines up front would be the other approach, and it is not available: `RunOnceCommand`'s lifecycle invariant confines planning-time checks to the environment and explicitly rules out per-content gatekeeping, so a line's shape can only be judged where it is run. With the terminator in place, that is exactly where it surfaces — as the manager's own "no such package" error, footnoted with the line number under [#8].

    5.3. Example

        A `uv.tools.txt`:

            # linting and formatting, pinned together
            ruff==0.14.2

            # ad-hoc data work
            visidata
            httpie

        :: text ::

        dodot runs `uv tool install -- ruff==0.14.2`, then `uv tool install -- visidata`, then `uv tool install -- httpie`.

    5.4. Failure Handling

        dodot continues past a failed line and collects every failure, rather than stopping at the first.

        Each item is independent — uv isolates each tool in its own environment, and npm's globals do not share a dependency tree — so continuing cannot corrupt anything. Stopping early would cost the user one round trip per broken line: fix, re-run, discover the next one, fix, re-run. It is also the consistent choice; `brew bundle`'s install phase continues and counts.

        The receipt is written only if every line succeeded. See [#7].


6. Availability And Bootstrapping

    6.1. Availability Is Resolved By Path, Not By PATH

        dodot probes an ordered list of fixed candidate locations and executes the binary directly. It does not use `command -v` and does not depend on its own PATH. Candidates resolve to absolute paths before probing; a leading `~` in a candidate is written for legibility and expands to the invoking user's home directory.

        This is more robust and it is also safer. A PATH lookup for `cargo` can resolve to a rustup shim which, invoked from a directory containing a `rust-toolchain.toml`, will silently download an entire toolchain before answering — observed during research. Executing a known path cannot do that.

        Candidate lists must include the Homebrew prefixes, not only each manager's native location, because brew-installed `node` and `uv` are the common macOS case. For `uv`: `~/.local/bin/uv`, `/opt/homebrew/bin/uv`, `/usr/local/bin/uv`, `/home/linuxbrew/.linuxbrew/bin/uv`. For `npm`: `/opt/homebrew/bin/npm`, `/usr/local/bin/npm`, `/home/linuxbrew/.linuxbrew/bin/npm`, plus the version managers that publish a stable shim — `~/.volta/bin/npm`, `~/.asdf/shims/npm`, `~/.local/share/mise/shims/npm`.

        A useful consequence: when one pack's `Brewfile` installs `uv`, a later pack's `uv.tools.txt` finds it in the same run — no new shell, no re-sourcing.

        npm is the one manager where a fixed list is knowingly incomplete, and the gap is worth stating rather than discovering later. nvm and fnm expose npm from a path containing the selected Node version, and neither installs a stable shim outside the shell function they define — so there is nothing to enumerate at compile time. Combined with the absent-manager outcome in [#6.2], a user whose only npm comes from one of those would get a silent permanent skip of their `npm.packages.txt`. Two things follow. The absent-manager line must report the locations that were probed, so "npm is not installed" and "npm is installed somewhere dodot does not look" are distinguishable ([#6.2]). And the remaining choice — a controlled PATH lookup for this one manager, versus a root-only trusted executable path — is left open here rather than settled in passing, because either answer has to be argued against the trust boundary in [#6.3] and [#9.1] rather than around it. See [#12].

    6.2. Three Outcomes, Not Two

        Manager absent:
            Skip. No receipt is written, no error is recorded. The status output carries an informational line naming the manager and the locations that were probed — without the second half, a manager installed somewhere dodot does not look is indistinguishable from one that is not installed at all, and the user has nothing to act on. Because no receipt exists, the next `dodot up` runs it for free once the user has installed the manager — no `--provision-rerun`, no stale state to clear.

        Bootstrap command runs and fails:
            A real error. Logged, surfaced as a status row, footnoted with the captured output. This is distinct from absence and must not be silently absorbed.

        Manager present and working:
            Proceed.

        The consequence of the first outcome is that dodot needs no ordering logic between packs. A pack whose `Brewfile` installs `uv` may run after a pack that needs `uv`; that pack simply skips and succeeds on the next run. dodot does not build a dependency graph, and does not pre-install managers on the user's behalf.

    6.3. dodot Never Installs A Package Manager

        Both Homebrew and Nix ship `curl | bash` as their official, documented, and only supported install path. Neither project offers an official signed installer.

        dodot will not execute such an installer, and will not offer to. Doing so would make dodot a high-value target: compromising dodot would mean intercepting a pipe-to-shell on the user's machine.

        What dodot does instead is name the manager and print its canonical project URL, shipped as a compile-time constant in the dodot binary. This is not the same risk class as executing a script — an attacker who can alter that constant can already alter everything else dodot does — and withholding the URL does not prevent the attack, it just sends the user to a search engine. What dodot must never do is take install instructions from configuration, from a pack, or from the network.

    6.4. PATH Bootstrapping Is The Exception, Not The Rule

        Most managers' installers already put their own output on PATH. The bootstrap question is only live where nobody else owns that path.

        Who owns the PATH entry:
            | Manager | Who puts its output on PATH | dodot acts? |
            | homebrew | nobody — the installer instructs the user to add `eval $(brew shellenv)` | *yes* |
            | cargo | rustup writes `~/.cargo/env` and sources it from `.zshenv` / `.profile` | no |
            | nix | the installer writes `/etc/zshrc`, `/etc/bashrc`, `/etc/profile.d` | no |
            | uv | its own installer edits shell profiles; tools land in `~/.local/bin` | no |
            | npm | the user's configured global prefix, already on PATH | no |

            :: table align=llc header=1 ::

        Homebrew is the only member, and it is a member for a specific reason: brew's own documentation delegates the shell setup to the user. dodot's existing `shell/homebrew.rs` is therefore not the first instance of a general pattern — it is a special case that stays special. There is no `shellenv_cmd` field in the per-manager descriptor of [#9], because generalizing it would imply a uniformity that does not exist. See [#11.2].

    6.5. Diagnose Everywhere, Fix Only Where dodot Owns The Path

        Even where dodot will not touch PATH, it should still check whether a manager's binary directory is actually reachable at shell-init time, and report when it is not.

        This catches the failure that otherwise appears silently much later — in a shell the user opens tomorrow, when a tool dodot installed is not found. The diagnosis sits outside the manager, so it is dodot's to make; the fix belongs to whoever owns the path.

        `~/.local/bin` is worth handling as a general convention rather than as part of any one handler. uv, pipx, and pip-on-Linux all write there.


7. Receipts

    Unchanged from the existing run-once machinery. The sentinel lives at `<data_dir>/packs/<pack>/<handler>/<filename>-<hash>`, with a `.snapshot` sibling holding the raw bytes of the file that was run, and the hash is taken over the file's raw content.

    Two points specific to this proposal:

    - _Raw bytes, not normalized content._ Editing a comment in a `.txt` file changes the hash. Normalizing first would be parsing, so it is not done, and the consequence is accepted rather than special-cased: an edited file follows the same run-once policy as an edited `Brewfile` or `install.sh`. A changed hash classifies as `RanDifferent`, which *skips* with a "ran older version" notice and applies only under `--provision-rerun`. dodot does not silently re-run an edited list, and mapped-txt does not get an exception to that. (The notice currently names the wrong flag — see [#11.8].)
    - _All-or-nothing for mapped-txt._ The receipt is written only when every line succeeded. A permanently broken line therefore means the receipt is never written, and this is the case that genuinely does re-run unprompted: with no sentinel at all the classification is `NeverRan`, not `RanDifferent`, so dodot runs the full list on every `dodot up` and reports the error every time, with no flag needed. That is correct — the declared state genuinely is not achieved — and it matches how a broken `Brewfile` already behaves.

    What a receipt asserts is narrow and deliberate: *this exact file content was applied successfully*. It asserts nothing about which packages are currently present. A package the user removed by hand, or a floating version that moved upstream, does not invalidate it.


8. Errors And Status

    The status row stays uniform across every manager: ran, failed, or skipped. Manager-specific richness goes into the footnote, not the row.

    dodot already has the display mechanism — `DisplayNote` with `note_ref`, rendered as a dim marker beside the status label and expanded under `Errors:`. What is missing is the plumbing: `CommandOutput.stderr` is captured today but never reaches the display layer as data. It is flattened into `CommandFailed`'s `Display` string and separately dumped to the terminal. See [#11.6].

    Where a manager offers something better than stderr, the footnote body should use it, configured per manager rather than assumed. For mapped-txt managers dodot additionally knows which *lines* failed, because it ran them, and should say so.

    dodot does not interpret, rewrite, or summarize a manager's error output. It attaches it verbatim.


9. The Declarative Surface

    A mapped-txt handler is four data fields:

    1. handler name
    2. ordered list of binary candidates
    3. argv template
    4. trigger filename

    Adding a mapped-txt manager later is one more entry in that table — no new handler logic, no new control flow. Manifest managers need the same four plus any required environment. Only bootstrapping remains code, and only for Homebrew.

    Where the table lives is itself a decision, and it is not user configuration: fields 2 and 3 name an executable and the arguments it receives, which is precisely the input [#6.3] says must never come from a pack. They belong in a compile-time registry, next to the canonical URLs of [#6.3]. Field 4 — which filename a handler claims — is ordinary user configuration and stays in `MappingsSection`. See [#9.1].

    This lands on an existing seam. `RunOnceCommand` already factors `install`, `homebrew`, and `nix` into one generic `RunOnceHandler<C>`; a data-driven descriptor replaces the per-manager struct. The availability hook `RunOnceCommand::validate` already exists, is already called, and is documented for exactly this purpose — "verify the tool is invokable" — with zero implementors today. Its signature is the wrong shape for [#6.2] and has to widen; see [#9.1].

    9.1. New Machinery Required

        Four things are missing today. Only one of them is user configuration.

        - _The manager descriptor, in code rather than in `MappingsSection`._ `HandlerConfig` carries nothing relevant to provisioning, and `RunOnceHandler::to_intents` ignores its config parameter entirely — so the fields have to go somewhere new either way. `MappingsSection` is the wrong somewhere: it resolves through defaults, then root, then pack, so a pack's own `.dodot.toml` overrides anything held there. Binary candidates and an argv template held at that layer would let a pack in a cloned dotfiles repo point the `npm` handler at an executable of its choosing, which is the trust boundary [#6.3] exists to hold. The descriptor is a compile-time registry instead.
        - _The new trigger filenames, at rule priority 10 or higher._ This is the part that genuinely is user configuration — which filename a handler claims, not what gets run — and it stays in `MappingsSection`. The catchall `*` to `symlink` sits at priority 0, so an unmapped `npm.packages.txt` would be deployed into `$HOME` as a dotfile.
        - _A mapped-txt execution intent._ This is the substantive execution change in the proposal, and the four descriptor fields do not imply it. `RunOnceCommand::command_for` returns one `(executable, arguments)` pair for the whole matched file, and `RunOnceHandler` emits one `Run` intent whose receipt is written when that single command exits zero. Nothing in that shape expresses reading the file, running one command per line, continuing past a failure ([#5.4]), collecting which line numbers failed so the footnote can name them ([#8]), or withholding the single file-level receipt until every line has succeeded ([#7]). Mapped-txt needs its own intent carrying the parsed lines, with those aggregation and receipt rules attached. Manifest managers keep the existing one-command-per-file path untouched.
        - _A pre-flight with three outcomes._ `RunOnceCommand::validate` returns `Result<()>` — two outcomes — and its `Err` aborts intent generation for the file and propagates as an error, where the three outcomes of [#6.2] need a third: absence produces neither a receipt nor an error. The shape to copy is the secret providers' `ProbeResult` — `Ok`, `NotInstalled{hint}`, `NotAuthenticated{hint}`, `Misconfigured{hint}`, `ProbeFailed{details}` — which maps onto manager availability nearly one-to-one, including where the "get it from the project's site" hint belongs (`NotAuthenticated` is the one variant with no package-manager analogue). The `preflight` aggregation is the model for reporting, not for policy: it turns any non-`Ok` probe into a hard error, which is right for secrets and wrong for a merely-absent manager.


10. Scope

    10.1. cargo: Not Supported

        Documented with the reason: cargo has no tool manifest and no bulk-install-from-file command, and the upstream request has been open since 2021.

        `cargo-liner` is a genuine one-file interface and is still declined. It requires bootstrapping a second tool that brew does not; it is a single-maintainer project two orders of magnitude less used than its neighbours; and it inherits the `required-features` rebuild bug, since it shells out to `cargo install`. Users who want cargo tools can put `cargo install --locked` lines in an `install.sh`, where owning the semantics is their choice rather than dodot's.

    10.2. pipx: Not Supported

        pipx does pass the bar outright — `pipx manifest sync <file>` is a real manifest interface with pruning and structured JSON results. It is declined anyway, for two reasons.

        Supporting two Python installers is a trap. pipx and uv write to the same `~/.local/bin` and will collide over identical symlink names with no arbitration between them.

        Between the two, uv is the better target despite needing the mapped-txt tier. pipx's `manifest sync` requires network on every run and *fails offline even when every tool is already installed*, at roughly 0.65s per tool; uv's per-tool install is ~60ms and genuinely offline. The manifest interface is the more principled one and the worse-behaved one. pipx has also delegated to uv as its backend since 1.12.0 when uv is present, so a uv-using machine with pipx installed is already running uv underneath.

    10.3. apt And System Managers: Not Supported

        Not a manifest question — `apt-get install -y $(cat file)` already works. The disqualifier is privilege. Every manager in this proposal is user-level and never needs root. apt is system-wide, needs `sudo`, contends on the dpkg lock, and brings the interactive-prompt hazard described in [#11.5]. dodot invoking `sudo` is a different risk class from everything else here.

    10.4. nix: Deferred

        The existing handler ships and keeps working. Reconciling it with this model is deliberately out of scope, because doing so surfaces a pre-existing problem that deserves its own decision.

        The current handler runs `nix profile install --expr <wrapper>`, where the wrapper is dodot-authored Nix code that normalizes list, bare-derivation, and attribute-set forms to a list. That is dodot owning semantics, in the one handler meant to serve as a baseline. It also uses the CLI with no removal story, and `nix-env` and `nix profile` write mutually incompatible profile formats — users who straddle them hit `profile manifest has unsupported version 3`.

        Moving to `nix-env --install --remove-all --file` would give real declarative semantics including atomic removal, at the cost of a migration and of adopting the CLI the ecosystem is moving away from. That trade needs its own proposal.

    10.5. Overlap With Brewfile Entries

        Since Homebrew 6.0.0 (2026-06-11) a `Brewfile` accepts `npm`, `cargo`, `uv`, `go`, and `krew` entries alongside `brew` and `cask`. On macOS, the handlers in this proposal therefore duplicate a capability users already have through the `Brewfile` handler dodot ships today.

        Note this does not violate the bar. Brew's extensions do loop per package internally — but brew owns that loop, not dodot.

        The documented position: use `Brewfile` entries if the dotfiles are macOS-only and brew is already the single front door — fewer moving parts, one file. Use `uv.tools.txt` and `npm.packages.txt` if the dotfiles must work on a machine with no brew, which is the reason these handlers exist. Do not declare the same package in both; dodot cannot reconcile two sources of truth and will not try.


11. Defects To Fix Alongside

    Research surfaced these in shipped behavior. They are listed here because this work touches the same code, not because they depend on it.

    11.1. brew invocations do not suppress auto-update

        dodot's argv is exactly `["brew", "bundle", "--file", <path>]`. The first brew invocation of any day triggers an auto-update — measured at 6.65s, with network traffic, upgrading brew itself and four taps. Set `HOMEBREW_NO_AUTO_UPDATE=1`.

    11.2. `brew shellenv` capture is sensitive to dodot's own PATH

        `brew shellenv` returns zero bytes when PATH already begins with the brew prefix — verified. dodot captures from whatever shell invoked `dodot up`, so a user who already has `eval $(brew shellenv)` in `.zprofile` can cause an *empty* block to be cached. If they then delete their manual line trusting dodot, brew silently disappears from new shells. The capture should run with a sanitized PATH so the guard cannot fire.

        Relatedly, on macOS 14+ `brew shellenv` does not emit `export PATH` at all — it emits an `eval` of `/usr/libexec/path_helper`, which rebuilds PATH rather than prepending to it. The current cache on the development machine contains exactly this form. It works today because the brew block is emitted before the packs PATH tier, but that ordering deserves an explicit decision against [./path-precedence.lex] rather than holding by accident.

    11.3. A failing bootstrap keeps the stale capture

        A detected-but-failing `brew` currently keeps the previous cached capture and emits a warning. Under [#6.2] that is an error, and the stale-capture fallback is precisely the silent-wrong-answer that produces [#11.2].

    11.4. `brew bundle install` upgrades by default

        What reads as "install this Brewfile" can be a long mutating upgrade of every outdated formula on the machine. Pass `--no-upgrade`.

    11.5. Cask password prompts write to `/dev/tty` and hang

        Homebrew's own source comments confirm cask installs can trigger interactive `sudo` prompts written directly to `/dev/tty`, bypassing stdin and stdout entirely. dodot pipes both, so such a prompt is invisible and the run *hangs* rather than failing. No flag disables it, and a cask that was quiet last month can begin prompting after an upstream version bump with no change to the `Brewfile`. Needs a timeout and a message telling the user to run that cask by hand.

    11.6. Captured stderr never reaches the display layer

        Described in [#8].

    11.7. A failing provisioning command aborts the rest of the pack

        The error propagates up and the pack reports a single pack-level error rather than a per-file failure row. That conflicts with the per-item footnote output this proposal assumes.

    11.8. Re-run advice names the wrong flag

        Three user-facing messages in `execution/run.rs` tell the user to run `dodot up --force`. The correct flag is `--provision-rerun`; `--force` does something else entirely.

    11.9. `Brewfile` fires on Linux by default

        OS gating currently requires the user to add a gate by hand.


12. Open Questions

    - Whether npm 12's default blocking of dependency install scripts affects top-level global installs. If it does, mapped-txt npm has nowhere to record `allowScripts` approvals, and tools needing a postinstall will install "successfully" and be broken at runtime. Not reproduced during research.
    - How dodot should find an npm supplied by nvm or fnm, whose path carries the selected Node version. The fixed candidate list in [#6.1] cannot cover it, and the alternatives — a controlled PATH lookup for this one manager, or a root-only trusted executable path — both reopen the trust boundary in [#6.3] and need arguing on their own terms.
    - PATH precedence for any entry these handlers contribute, against [./path-precedence.lex].
