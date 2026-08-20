Proposal: Provisioning Handlers

    dodot has three provisioning handlers — `install`, `homebrew`, `nix`. They share one lifecycle, one receipt mechanism, and one set of unanswered questions about the tools they invoke. Two of those questions have no implementation at all, and the shared path carries defects severe enough that an absent tool costs a pack its entire deployment.

    An investigation into adding handlers for the language-bound package managers — `cargo`, `npm`, `uv`, `pipx` — concluded that none of them should be added. What it surfaced instead is the work in this document.

    This proposal settles which package managers dodot supports and why, names the three axes a provisioner has to answer, reduces the per-handler surface to a descriptor, and lists the defects that live in the same code.

    Facts marked as verified were reproduced on 2026-08-20 against Homebrew 6.0.18 on macOS arm64, or against the cited upstream source. Nix findings are documentation-derived; no Nix installation was available.


1. Position: Two Package Managers

    dodot supports `homebrew` and `nix`, and adds no others. Neither is required; each is required only by packs that declare its file.

    1.1. The Language-Bound Managers Are Out

        `cargo`, `npm`, `uv`, and `pipx` are not supported, and the reason is structural rather than a matter of effort.

        Each of them installs for exactly one language. A dotfiles tool that ships handlers for them acquires a handler count that grows with the number of languages that ever become popular, and always lags the newest one. Homebrew and Nix are general-purpose: their coverage grows without dodot changing, because their own maintainers absorb each new ecosystem.

        Homebrew has already absorbed these four. A `Brewfile` accepts `go`, `cargo`, `uv`, `npm`, and `krew` entries alongside `brew` and `cask`, and a user who wants `ruff` and `prettier` on a new machine writes them there.

        Verified by execution — a `Brewfile` containing all five entry types parses, and `brew bundle check` type-checks each one under its own label:

            $ brew bundle check --verbose --file=Brewfile.test
            → Cask firefox needs to be installed or updated.
            → Go Package golang.org/x/tools/cmd/goimports needs to be installed.
            → Cargo Package ripgrep needs to be installed.
            → uv Tool ruff needs to be installed.
            → Krew Plugin ctx needs to be installed.

        :: console ::

        Two facts make this a stronger answer than "the same thing, elsewhere".

        _brew installs the toolchain itself._ `Bundle::Extension.preinstall!` calls `ensure_package_manager_installed!`, which runs `brew install --formula <manager>` when the manager is absent — `node` for npm entries, `rust` for cargo, and the manager's own formula for uv, go, and krew. An `npm "typescript"` line works on a machine with no Node. The obvious objection to delegating — _it only helps if you already have the manager_ — does not apply.

        _The capability is older than it looks._ Support did not arrive together in one recent release:
            | Entry | Added in | Released |
            | `go` | 4.6.17 | 2025-10-13 |
            | `cargo` | 5.0.7 | 2025-12-21 |
            | `uv` | 5.0.16 | 2026-03-02 |
            | `npm` | 5.1.2 | 2026-03-30 |
            | `krew` | 5.1.2 | 2026-03-30 |

            :: table align=llr header=1 ::

        The floor is therefore Homebrew 5.1.2, released 2026-03-30 — five months old at the time of writing, against a patch cadence of roughly a week, so most live installations are already well past it.

        That is a statement about the population, not a mechanism dodot can rely on. Brew's automatic `brew update` before most commands would carry a stale installation over the floor on its own, but [#8]'s defect 6 sets `HOMEBREW_NO_AUTO_UPDATE` precisely to stop it: once that lands, an old brew stays old, and a `Brewfile` using these entry types fails on a line the user was entitled to write. So the floor has to be checked rather than assumed. `brew --version` is already the natural probe for [#4]'s availability check, and a brew below 5.1.2 is a reportable condition with a named remedy — not a parse error the user has to decode.

        One consequence to document rather than discover: brew's detection resolves through `ORIGINAL_PATHS`, so a `cargo` supplied by rustup or an `npm` supplied by nvm is _adopted_ rather than replaced. Which side owns the toolchain depends on the machine's PATH.

    1.2. Why Not apt

        `apt-get install -y $(cat file)` already works, so this is not a manifest question. The disqualifier is privilege: every manager dodot supports is user-level and never needs root. apt is system-wide, contends on the dpkg lock, and prompts interactively. dodot invoking `sudo` is a different risk class from everything else here.

    1.3. Where Linux Actually Lands

        Homebrew runs on Linux, and that is what makes a single macOS-shaped answer defensible for a second platform. It is not parity, and the documentation should say so rather than assert it.

        What a Linux user takes on, from Homebrew's own documentation:

        - A working system C compiler and the distribution's build tools — `build-essential procps curl file git` on Debian and Ubuntu, the equivalents elsewhere.
        - Homebrew's own `gcc` and, on older distributions, its own `glibc`, installed alongside the system's.
        - No support at all on 32-bit x86; Tier 3 on ARM32 and WSL 1.

        Casks are no longer strictly macOS-only — the `app_image` artifact stanza is Linux-only and `binary` works on both — but coverage is thin: 119 of 7,704 cask files mention `app_image`, about 1.5%. "Casks work on Linux" is true of the mechanism and false of the catalog.

        The honest shape is a ladder, not a claim of equivalence: Homebrew where its prerequisites are acceptable, Nix where they are not, `install.sh` where neither fits.

    1.4. The Nix Pillar Is Unsettled

        The reason to support Nix is not Linux coverage — Homebrew already provides that. It is that Nix _can_ pin exact package versions on both platforms, where a `Brewfile` installs whatever is current and cannot portably declare either an exact version or the tap snapshot it resolves against. The capability is Nix's; it is not yet dodot's, because the manifest shape dodot ships resolves `<nixpkgs>` from the user's mutable `NIX_PATH`. See [#7.4].

        That is a narrower claim than "Nix is the reproducible one", and deliberately so. dodot's use of Nix does not converge a machine on its manifest, and the routes to making it do so are either destructive to existing users or corrosive to the uniformity this proposal is built on. See [#7].


2. What A Provisioner Answers: Three Axes

    `install.sh`, `Brewfile`, and `packages.nix` look different and are the same kind of thing: a file handed to a command, run at most once, leaving a receipt. That position is already settled — [./shipped/nix.lex] §5.5 argues it directly, holding that the question _did we run this exact file successfully?_ "applies uniformly to `install.sh`, `Brewfile`, and `packages.nix` … because the underlying epistemic problem … is identical".

    What the code has not done is separate the three distinct questions that sameness contains. Naming them is most of this proposal.

    Axis 1 — _Can we run it?_:
        Is the executable this handler will actually spawn present on this machine? Answered per command, at planning time — which for `install` means the interpreter the matched file's extension selects, not "a shell". Implemented for none of the three today.

    Axis 2 — _Should we run it?_:
        Has this exact file content already been applied? Answered by the sentinel, at execution time. Fully generic already, with no per-handler branching.

    Axis 3 — _Is what it produced reachable later?_:
        Will the binaries it installed be found in a shell the user opens tomorrow? Answered differently by each, and by none of them today.

    Per handler:
        | Handler | Axis 1 | Axis 3 |
        | `install` | probe the interpreter the extension selected — `bash`, or `zsh` for `.zsh` | _unknowable_ — dodot cannot know what the script wrote or where |
        | `homebrew` | probe fixed candidate prefixes | dodot owns the PATH entry, via `brew shellenv` — on macOS today, and a requirement rather than a fact on Linux; see [#3.2] |
        | `nix` | probe fixed candidate prefixes | the Nix installer owns it |

        :: table align=lll header=1 ::

    The `install` row is the one worth stating explicitly, on both axes. It is tempting to treat `install.sh` as trivially fine everywhere, since a shell is always there. Neither half survives contact with the code.

    On axis 1, `install` does not run "a shell". `interpreter_for` picks `zsh` for `.zsh` and `bash` for everything else, including unknown extensions, and the intent spawns that bare name through PATH. bash is absent from minimal container images and zsh is optional nearly everywhere, so what gets probed is the interpreter the extension selected — the same three outcomes as brew and nix, against a different executable.

    On axis 3, dodot has no idea what an arbitrary script installed or where it put it. _Unknowable_ is a real answer, distinct from _yes_, and collapsing the two would have dodot asserting something it cannot know.


3. The Descriptor

    A provisioner becomes a data row plus the shared handler.

    3.1. Fields

        The descriptor:

            struct ProvisionerDescriptor {
                handler_name:  &'static str,
                phase:         ExecutionPhase,
                availability:  Availability,
                argv:          ArgvTemplate,
                env:           &'static [(&'static str, &'static str)],
                manifest_arg:  ManifestArgPosition,
                status_copy:   RunOnceStatusMessages,
                reachability:  Reachability,
            }

        :: rust ::

        Three of the eight are the obvious ones and need no defense: a handler name, an argv template, and the status copy. The other five are each shaped by something specific in the current implementation:

        `availability`:
            Not a constant per handler. `homebrew` and `nix` probe a fixed list of candidate prefixes, but `install` spawns whatever interpreter the matched file's extension selects, so the executable to probe depends on the file rather than on the row. The field names _how to resolve_ the executable, not the executable.

        `phase`:
            `install` runs at `Setup`, `homebrew` and `nix` at `Provision` — so install scripts run _after_ package managers. That ordering is deliberate rather than accidental: it is what lets an `install.sh` assume brew has already run, which is what makes it a viable escape hatch for anything not supported here.

        `env`:
            There is no way to set an environment variable for a provisioning command at any layer — not on `RunOnceCommand`, not on `HandlerIntent::Run`, not on `Operation::RunCommand`, not on `CommandRunner::run`. The spawn site inherits dodot's entire environment and sets nothing. `HOMEBREW_NO_AUTO_UPDATE` and the other fixes in [#8] are unreachable without this field.

        `manifest_arg`:
            `run_and_record` identifies the file that was run as `arguments.last()`, by convention. `nix` violates the convention — its last argument is `"nix-command flakes"` — so nix runs write no snapshot and print the wrong name in their progress header. Declaring which argument carries the manifest replaces a positional assumption with a stated fact, and repairs that defect as a consequence rather than as a patch.

        `reachability`:
            Axis 3 from [#2], with three values: `DodotOwns`, `InstallerOwns`, `Unknowable`.

    3.2. What Stays Code

        Homebrew's shell bootstrap. Brew's own documentation delegates PATH setup to the user, and no other manager here does — Nix's installer writes `/etc/zshrc` and `/etc/profile.d` itself. Generalizing the bootstrap into a descriptor field would imply a uniformity that does not exist, so `shell/homebrew.rs` stays a special case and stays special.

        Staying a special case is not the same as staying as it is. Both entry points — `capture` and `cached_or_capture_at` — return early on `!host.is_macos`, so off macOS dodot emits nothing regardless of what is installed, and `DEFAULT_PREFIXES` holds `/opt/homebrew` and `/usr/local` only, neither of which is where Linux brew lands. [#1.3] makes Homebrew a supported Linux path, which turns [#2]'s reachability row into a requirement on Linux rather than a description of it: the macOS short-circuit has to go, `/home/linuxbrew/.linuxbrew` and its `~/.linuxbrew` fallback have to join the probe order, and Linux emission needs tests of its own. Until that lands, a Linux user owns their own brew PATH setup and the documentation should say so, rather than promise macOS behavior everywhere. Recorded as defect 12.

    3.3. Where The Descriptor Lives

        In a compile-time registry, not in `MappingsSection`.

        Two of the fields — `availability` and `argv` — name an executable and the arguments it receives. `MappingsSection` resolves through defaults, then root, then pack, so a pack's own `.dodot.toml` overrides anything held there. A cloned dotfiles repo could then point a handler at an executable of its choosing.

        Which _filename_ a handler claims is ordinary user configuration and stays in `MappingsSection` at rule priority 10 or higher. What gets executed is not.

        This boundary has no ADR behind it. The three existing ADRs cover the Safety Lock and bind trust to a root's canonical path, not to pack content; [./../spec/safety-lock.md] explicitly places content evaluation of install scripts, Brewfiles, and Nix manifests out of scope. The distinction this section draws — configuration may select _which file_, never _which executable_ — is new durable ground and should land as an ADR rather than as a paragraph in a proposal that is later marked historical.

        A related inconsistency to settle in the same place: secret providers locate their binaries by spawning a bare name against dodot's inherited PATH, while the Homebrew bootstrap probes fixed absolute candidates. Provisioners follow the Homebrew model, for the reasons in [#4]. Two philosophies coexisting is acceptable; two philosophies coexisting unremarked is not.


4. Availability: Three Outcomes

    4.1. The Pattern Already Exists

        Availability probing reads as new machinery. It is not — it is an existing pattern in a sibling module that the handler path does not reuse.

        `shell/homebrew.rs` already probes an ordered candidate list (`$HOMEBREW_PREFIX`, then `/opt/homebrew`, then `/usr/local`), tests each for a real executable file by stat and mode bits rather than by PATH lookup, and executes the resulting absolute path. It already distinguishes exactly three outcomes: `Absent`, `Captured`, and `Failed`.

        Those are the three outcomes provisioning needs, under different names:

        Manager absent:
            Skip. No receipt, no error. An informational line names the manager and the locations probed — without the second half, "not installed" and "installed somewhere dodot does not look" are indistinguishable and the user has nothing to act on. Because no receipt is written, a later `dodot up` runs it for free once the manager exists.

        Probe failed:
            A real error, surfaced with the captured detail. Distinct from absence and never silently absorbed.

        Manager present:
            Proceed.

        The hook to carry this is also already present and also unused. `RunOnceCommand::validate` exists, is called during intent generation, and is documented for exactly this purpose — "for shelling out to verify the tool is invokable — e.g. `nix --version`, `brew --version`". No shipped handler implements it. Its signature has to change: `Result<()>` expresses two outcomes and its `Err` aborts intent generation, where absence must produce neither a receipt nor an error.

        Widening that signature is necessary and not sufficient. See [#4.2].

    4.2. One Probe, Two Callers

        `validate` is consumed inside `RunOnceHandler::to_intents`. If it reports absence there and the handler emits no intent, the finding dies with the intent: `commands/status.rs::run_once_health` derives a row's health independently — source file present, checksum, datastore receipt — and never calls `validate` at all. An absent manager would render as _never run_, indistinguishable from one nobody has run yet, with nothing naming the locations probed. The [#6.3] row this proposal promises has no data path to reach.

        So availability is not a handler method. It is one shared module — the descriptor's `availability` field plus a single probe over it — that planning and status both reach through the same interface, and whose outcome travels as an ephemeral diagnostic on the plan and on the status row rather than as anything persisted. A receipt records that a file ran; absence is a fact about this machine right now, and re-probing is cheap.

        Two callers agreeing is the requirement, and it is testable rather than a note: for each of absent, failed, and present, `dodot up`, `dodot status`, and a dry run must report the same outcome for the same machine.

    4.3. Probing Is Scoped By What The Run Selected

        No new mechanism is needed to avoid probing managers nobody uses. `validate` is reached only for files a rule matched, in packs the current command selected, so a machine with no `Brewfile` never probes for brew.

    4.4. Ordering Between Packs Is Not dodot's Problem

        Because an absent manager skips without a receipt, a pack whose `Brewfile` installs a tool may run after a pack that needs it; the second pack skips and succeeds on the next run. dodot builds no dependency graph and pre-installs nothing.

    4.5. dodot Never Installs A Package Manager

        Both Homebrew and Nix ship pipe-to-shell as their official and only supported install path. dodot will not execute such an installer and will not offer to; doing so would make dodot worth compromising as a way to intercept one.

        What dodot does is name the manager and print its canonical project URL, held as a compile-time constant. An attacker who can alter that constant can already alter everything else dodot does, and withholding it only sends the user to a search engine. What dodot must never do is take install instructions from configuration, from a pack, or from the network.


5. Receipts

    Unchanged. The sentinel lives at `<data_dir>/packs/<pack>/<handler>/<filename>-<hash>`, with a `.snapshot` sibling holding the raw bytes of the file that ran; the hash is the first 8 bytes of a SHA-256 over the file's raw content, rendered as 16 hex characters.

    Two corrections to the record rather than changes to behavior:

    - [./shipped/nix.lex] §7 states the hash is Blake3. It is SHA-256. The user documentation is correct and the shipped proposal is not.
    - Three documents state that an edited script re-runs automatically, which is the inverse of shipped behavior. See [#8].

    What a receipt asserts stays narrow and deliberate: _this exact file content was applied successfully_. It asserts nothing about which packages are currently present. A package the user removed by hand, or a floating version that moved upstream, does not invalidate it.


6. Errors, Status, And Skipping

    6.1. A Failure Must Not Take The Pack With It

        Today a provisioning failure propagates out of the intent loop, the pack is recorded as failed, and every operation that already succeeded in that pack is discarded. Because provisioning runs before `PathExport`, `ShellInit`, and `Link`, a failing `Brewfile` costs that pack its symlinks, its PATH entries, and its shell initialization.

        A failing provisioning command must produce a per-file failure row and leave the rest of the pack's work intact.

    6.2. A Failure Must Reach The Exit Code

        `dodot up` returns success regardless of whether any operation failed. The failure is discoverable only by a human reading the rendered table; a bootstrap script, an image build, or a chained `dodot up && …` sees success.

    6.3. Absence Annotates, It Does Not Fail

        The precedent exists: `Health::RecentFailures` attaches a warning-kind footnote to a row without changing that row's verdict. An absent manager takes the same shape — a row that reports the skip and a note naming the locations probed. The probe result reaches that row through [#4.2]'s shared module; `run_once_health` builds the row and never sees `validate`.

        Adding a `Health` variant is compiler-checked; its `style`, `label`, and `footnote` arms are exhaustive matches. Adding a new _status string_ is not: the aggregation has a catch-all arm that silently drops unknown values from the summary counts, and the theme is defined twice — once in YAML in the library, once in CSS in the CLI — with no mechanism keeping them in step. Prefer the enum.

    6.4. A Deliberate Skip Should Say So

        `--no-provision` drops code-execution handlers before intent generation, so no rows are produced at all and nothing marks the files as deliberately skipped; status shows their true datastore state with no indication that the user asked to skip them. It also suppresses the `external` handler's fetches, which its help text does not mention.

        Both are worth correcting here: a deliberate skip should be visible as a skip, and the help should describe what the flag does.

    6.5. Manager Output Belongs In The Footnote

        The status row stays uniform — ran, failed, or skipped. Manager-specific detail goes into the footnote.

        The display mechanism exists; the plumbing does not. Captured stderr never reaches the display layer as data: it is written directly to the process's stderr, bypassing the renderer, and separately flattened into an error's `Display` string. dodot does not interpret, rewrite, or summarize a manager's error output — it attaches it verbatim.


7. Open Question: The Nix Pillar

    Nix earns its place by offering a guarantee Homebrew does not. dodot does not currently deliver that guarantee, and the obvious remedy is unavailable to dodot's own users.

    7.1. What Is Missing

        The current handler runs `nix profile install --expr <wrapper>`, where the wrapper is dodot-authored Nix code normalizing list, bare-derivation, and attribute-set forms. Removing a package from `packages.nix` therefore removes nothing from the machine: the file is a to-do list that ran once, not a description of profile state. [./shipped/nix.lex] §4 states this honestly as "ensure installed", not "owned by pack".

        A rebuilt machine converges on the file. An existing machine drifts from it.

    7.2. Why The Obvious Fix Is Blocked

        The one genuinely declarative idiom in Nix's CLI is `nix-env --install --remove-all --file <path>`, where the file becomes the whole truth for the profile in a single transaction, removals included. It remains valid in Nix 2.35.2, and `nix-env` carries no formal deprecation.

        It is unavailable here. Nix's own manual states that once `nix profile` has been used, `nix-env` cannot be used again without first deleting the profile directory — which destroys the installed set. dodot's shipped handler runs `nix profile install`. dodot has therefore placed its own Nix users in precisely the state where the remedy costs them their packages.

        There is also no declarative input for `nix profile` itself: its subcommands are all imperative, and the standing request [https://github.com/NixOS/nix/issues/7965] remains open, last active 2026-08-05.

    7.3. Decision: Ensure-Installed, Stated Plainly

        dodot keeps installing into the user's default profile via `nix profile`, and describes the semantics honestly rather than reaching for a guarantee it cannot deliver. Nix behaves exactly like `homebrew` and `install`: dodot ran this file, and recorded that it did.

        Two alternatives were considered and declined.

        _Migrating to `nix-env`._ Real declarative sync, at the cost of a destructive one-time migration for every existing dodot Nix user — the population [#7.2] describes — and of adopting the CLI the ecosystem is moving away from. The cost falls on precisely the people the change would serve.

        _A dodot-owned profile._ Pointing Nix at a profile dodot owns rather than the user's default. A fresh profile carries no incompatible manifest, so the obstruction in [#7.2] would not apply, `--remove-all` would become available, and `dodot down` would gain a real uninstall story.

        Declined on two grounds. The first is a rule about what dodot may cost a user who leaves: a profile held inside dodot's data directory disappears when dodot does, taking its packages off PATH immediately and leaving their store paths to be collected by the next garbage collection. A tool must not make its own removal destructive.

        Siting the profile at a durable Nix-managed location instead would avoid that specific harm — the profile would outlive dodot, still rooted, with only the PATH entry lost. It is declined on the second ground, which that variant does not escape: it makes Nix the one provisioner that does not fit the model. `homebrew` and `install` hand a file to a command and record the run. Any owned-profile variant additionally needs a profile path, a PATH contribution, and an uninstall story — descriptor fields that exist for exactly one row, and a permanent exception to the uniformity [#3] is built to obtain. Declarative removal does not buy that back.

    7.4. What The Claim Becomes

        Not "Nix is the reproducible option and Homebrew is not". The distinction that survives is narrower and truer, and it is the one users feel:

        - _Nix can pin._ A derivation resolves to exact bits, so a manifest naming an explicitly pinned `nixpkgs` yields the same versions on every machine, now and later.
        - _A `Brewfile` cannot, portably._ It yields whatever is current when it runs.

        Homebrew is not without version controls, and the distinction is worth drawing precisely rather than categorically. Two exist and neither closes the gap. A `Brewfile` can name a versioned formula — `postgresql@14` — where Homebrew publishes one, and that much travels between machines; verified by execution, `postgresql@14` resolves to 14.24, so what the manifest declares is a series and not a version. `brew pin` freezes an installed formula against `brew upgrade`, but it is machine-local state under the prefix that `brew bundle dump` does not emit, so no manifest carries it and a second machine does not inherit it. What a `Brewfile` cannot express is the thing being contrasted here: an arbitrary exact version, or a snapshot of the tap it resolves against.

        The pin is Nix's to offer and not yet dodot's to deliver, and that difference matters enough to state rather than blur. The manifest shape dodot documents and its wrapper expression relies on — `{ pkgs ? import <nixpkgs> {} }` — resolves `<nixpkgs>` from the user's mutable `NIX_PATH`. A derivation is immutable once evaluated, but the manifest that produces it is not: two machines on different channels, or the same machine after `nix-channel --update`, evaluate the same `packages.nix` to different derivations. [./shipped/nix.lex] §9.2 already records pinned-`nixpkgs` injection as an unshipped v2 mode, and the user documentation lists the same gap. So the honest form of the distinction today is that Nix can express an exact, portable pin and a `Brewfile` cannot — with that pin a manifest the user writes and a mode dodot has yet to support.

        What neither provides through dodot is convergence: removing a line from either file uninstalls nothing. That is the same "ensure installed" position [./shipped/nix.lex] §4 already states, now stated for all three provisioners rather than for Nix alone.


8. Defects In The Same Code

    Found while surveying the shipped implementation. They are listed here because this work touches the same paths, not because they depend on it. Ordered by severity.

    1. `dodot up` exits zero when a provisioning command fails. See [#6.2].
    2. A failing or absent provisioner aborts the rest of its pack and discards operations that already succeeded, before symlinks are deployed. See [#6.1]. A missing `brew` on Linux currently takes this path.
    3. The `nix` handler writes no snapshot, because the snapshot is taken from the command's last argument and nix's last argument is not the manifest. `dodot status --diff` is therefore permanently unavailable for nix, and its progress header prints the wrong name. See [#3.1].
    4. Three documents state the opposite of shipped behavior, claiming an edited script re-runs automatically and that `--provision-rerun` is only for re-running _without_ a change: the handler reference, the storage developer document, and the templates user document. An earlier sweep corrected four files of this class and missed these.
    5. The re-run advice names the wrong flag in three messages, and worse, the row a user actually sees after a real `dodot up` names no remedy at all. One of the three wrong strings is pinned by a test asserting the incorrect text.
    6. brew invocations do not suppress auto-update. The first brew invocation of a day triggers a measured multi-second network update. Needs `HOMEBREW_NO_AUTO_UPDATE`, which needs [#3.1]'s `env` field.
    7. `brew bundle install` upgrades by default, so what reads as "install this Brewfile" can be a long mutating upgrade of unrelated formulae. Needs `--no-upgrade`.
    8. Cask installs can trigger interactive `sudo` prompts written directly to `/dev/tty`. dodot pipes stdin and stdout, so such a prompt is invisible and the run hangs rather than failing. No flag disables it. Needs a timeout and a message naming the cask to run by hand.
    9. `brew shellenv` is captured without sanitizing dodot's own PATH. The consequence is milder than previously recorded — an empty capture is already treated as a failure rather than cached — so the result is a spurious warning and a retained previous capture, not silent loss. The fix is unchanged.
    10. On the passive `init-sh` path a failed capture drops the Homebrew block with no warning, because that path emits the script itself and has no warnings channel. Silent, and repeated on every shell start until the next `dodot up`.
    11. `Brewfile` fires on Linux by default, requiring a hand-written pattern to suppress it. [#4] dissolves this: an absent manager becomes a skip, which is the correct behavior once Homebrew is a supported Linux path rather than an assumed-macOS one.
    12. The Homebrew shell bootstrap is macOS-only and cannot see a Linux brew: `capture` and `cached_or_capture_at` both return early on `!host.is_macos`, and `DEFAULT_PREFIXES` omits `/home/linuxbrew/.linuxbrew`. Not a live defect today, because Linux brew is not yet a supported path — a prerequisite for making it one, without which brew-installed tools stay unreachable in later Linux shells. See [#3.2].

    The taxonomy documentation is separately and pervasively stale: the reference, developer, glossary, and README handler tables all predate the `nix`, `external`, and `gate` handlers, several still say dodot ships eight handlers, and the execution-order document claims each phase holds a single handler. The same facts are transcribed into roughly eight places by hand, and the handler-authoring guide mandates updating four of them. Generating the table from the registry would retire that class of error.


9. Scope

    9.1. Not Supported, And Why

        `cargo`:
            No tool manifest and no bulk-install-from-file; the request [https://github.com/rust-lang/cargo/issues/9527] has been open since 2021 and remains open. Partial failure is not atomic and the exit code does not distinguish it from total failure. Use a `Brewfile` `cargo` entry, or `cargo install --locked` lines in an `install.sh`.

        `uv`:
            No declarative tool installation; [https://github.com/astral-sh/uv/issues/12533] remains open. Use a `Brewfile` `uv` entry.

        `npm`:
            No global manifest. Use a `Brewfile` `npm` entry.

        `pipx`:
            Passes on its own merits — `pipx manifest sync` is a real manifest interface — and is declined anyway. Two Python installers writing to the same directory will collide over identical symlink names with no arbitration, and pipx has delegated to uv as its backend since 1.12.0 when uv is present.

        `apt` and system managers:
            See [#1.2].

    9.2. Overlap Is The User's Call

        On a machine with brew, a `Brewfile` `uv` entry and a hand-rolled `install.sh` loop can both install the same tool. dodot cannot reconcile two sources of truth and will not try. Declare a package in one place.

    9.3. Not In Scope

        Parsing manifests, reconciling versions, deciding which packages are missing, repairing partial state, or diagnosing package-level failure. dodot can truthfully say _we ran this file, and here is what the manager said_. It cannot truthfully say which packages are present, and it will not guess.
