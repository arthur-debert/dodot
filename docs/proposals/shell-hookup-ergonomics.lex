Design Specification: Shell Hookup Ergonomics

    :: note ::
        *Status: proposed.* Epic RCS01. Successor to [./shipped/shell-hookup.lex] (INS01), which shipped the activation evidence this document extends. Read that first: the generation stamp, the heartbeat, the signal ladder, the probe, and `dodot install` are all defined there and assumed here.

        Two shipped deviations, where the code is the authority. The diagnosis probe landed *inside* `dodot probe shell-init`, running by default — `--no-trace` opts out, a `<PACK[/FILE]>` argument suppresses it — not as the separate `dodot probe shell-hookup` command [#3.1] proposes (epic `#288`, decision 6). And the version-skew hint points at `$PATH` resolution generically ("check which one your PATH finds first") rather than naming both binary paths as [#2.3] promises; the probe is where both paths get named.

    INS01 taught dodot to ask whether any shell loads its init script. It answers that question with one bit — a generation number — and that bit turns out to be too narrow. A hookup can be wired, sourced on every shell start, and still broken, because the *binary* that generated the init script is not the binary the user runs. dodot cannot see that today, and the message it prints instead sends the user to the one fix that cannot work.

    This proposal widens the evidence to carry a version, replaces the single activation line with a two-line footer that says what happened and when, adds an on-demand probe that reports what `dodot` resolves to *at the hook line* rather than guessing, and moves the Homebrew bootstrap inside dodot so the shell rc no longer has to contain anything at all.


1. The Problem

    1.1. A Failure INS01 Reported Correctly and Diagnosed Wrongly

        Observed on a maintainer's machine, running dodot 5.5.1. Every shell reported:

        Status footer, verbatim:

            This shell hasn't loaded dodot.
            The hook is in /Volumes/code/projects/dotfiles/zsh/zshrc, so this shell probably
            predates it — open a new shell. If that changes nothing, run `dodot up` to diagnose.

        :: text ::

        The headline was true. The hint was useless: no new shell would ever fix it. Tracing a clean login shell (`env -i … zsh -l -i -x`) found the cause:

            - The rc hook was the hand-wired form, `eval "$(dodot init-sh)"`, at `~/.zshrc:6`.
            - At that line `PATH` holds only what `path_helper` and `~/.zshenv` produced. `/opt/homebrew/bin` is absent, because dodot's *own* init script is what adds it.
            - `/usr/local/bin/dodot` is a dangling symlink, so resolution falls through to `~/.cargo/bin/dodot` — dodot *5.0.0*, which predates the evidence block entirely.
            - So the shell evaluated a 5.0.0 init script: no `DODOT_INIT_GEN` export, no heartbeat write. At the prompt, `dodot` resolved to 5.5.1 and correctly reported no stamp.

        Every layer behaved as specified. The user still had no path to the answer, and the hookup had been silently one major version behind for months.

    1.2. Three Gaps

        The story above is one instance of three separable gaps:

            - *The evidence carries no identity.* The stamp says _when_ a script was generated, never _by what_. Version skew — the single most likely cause of a hookup that is wired but dead — is invisible to every command.
            - *The evidence carries no run time.* Under the file-source hook the generation is a *write* time. Measured on the same machine: heartbeat content `1786830419` (18:46, when `up` wrote the script) against heartbeat mtime `19:31:54` (when a shell last sourced it). Nothing dodot prints can answer "when did this last actually run".
            - *Nothing can see PATH at the hook line.* `dodot status` is forbidden from spawning a shell ([./shipped/shell-hookup.lex] §9), and the probe measures only whether a stamp comes back — not what the shell resolved, or failed to resolve, to get there.

        A fourth, adjacent: the Homebrew bootstrap that caused the PATH gap is work every macOS user redoes by hand. See [#4].

    1.3. Goals

        - The evidence identifies the binary that produced it, so version skew is detectable without spawning anything.
        - Every status render ends with two lines: whether dodot is sourced in new shells, and when it last ran, by which version.
        - A user with a broken hookup can run one command that names the rc file, the line, what `dodot` resolves to *there*, and why.
        - A macOS user gets a working Homebrew environment without authoring a bootstrap pack or a line of rc.

    1.4. Non-Goals

        - Reworking PATH precedence. The final ordering of PATH entries is `#260`'s subject and stays there; see [#4.3].
        - Migrating existing hand-wired hooks. `dodot install --write` continues to leave a hook it finds in place, whatever its form.
        - New ordering surfaces for packs. [./shipped/pack-ordering.lex] settled that contract and this proposal relies on it unchanged.
        - Making `status` measure anything. It stays passive; the new diagnosis is a separate, opt-in command.


2. Evidence, Version-Stamped

    2.1. What the Init Script Records

        The generated script gains one export and one field, both on the existing "one export, one redirect" budget — no command execution, no `dodot` invocation:

        Activation evidence block:

            export DODOT_INIT_GEN=1786830419
            export DODOT_INIT_VERSION=5.6.0
            echo '1786830419 5.6.0' >| '…/probes/hookup/heartbeat' 2>/dev/null || :

        :: sh ::

        The export answers "which dodot did *this* shell load"; the heartbeat field answers "which dodot did the last shell anywhere load". Both are needed: the first is what a user's own session can act on, the second is what a detached caller sees.

        *Amended during review.* The redirect is `>|`, not `>`. Under `setopt noclobber` / `set -C` — an ordinary rc line — a plain `>` refuses to write a file that already exists, and the `2>/dev/null || :` guards hide the refusal. The heartbeat exists after the first activation, so every activation after that would fail silently and freeze "last loaded" at the first shell ever run. This epic newly rests three claims on that file — the footer's timestamp, the skew comparison, and the probe gate — so the frozen file reads as a confident wrong answer. `>|` is POSIX; verified against zsh, bash and dash.

        A heartbeat or stamp carrying no version renders as `≤5.5.1` — one rule covering both a pre-RCS01 binary and a stale heartbeat left on disk by one. The boundary version is a single constant, set to the release preceding this work.

    2.2. Run Time Is Not Generation Time

        "Last run" comes from the heartbeat file's *mtime*, never from the generation it contains. The generation is a property of the script; the mtime is a property of the last shell that sourced it. They coincide only under the `eval "$(dodot init-sh)"` hook, which regenerates on every shell start — which is precisely why the divergence was never visible before.

        No new write is required: the redirect already updates mtime on every activation. When the heartbeat is missing or unreadable, "last run" is simply unknown, and the second line says so rather than guessing.

        *Amended during review.* The run time and the version have to come from the same activation. The stamp decides the *state* — it speaks for the session the user can act on — but the mtime belongs to whichever shell wrote the heartbeat, so a stamp that outranks the heartbeat may not take the heartbeat's timestamp with it: a shell stamped 5.6.0 beside a more recently written 5.0.0 heartbeat would otherwise render "Last loaded 4 minutes ago by dodot 5.6.0", a moment at which no 5.6.0 shell loaded anything. When the two signals name different dodots, line two reports both activations ("This shell loaded dodot 5.6.0; the last shell to load ran dodot 5.0.0, 4 minutes ago."); a stamp with no heartbeat at all carries no time and says so.

    2.3. The Two-Line Footer

        The single activation notice becomes a footer of two lines, emitted by the *shared* status renderer — so `up`, `down`, and `status` all carry it. This deliberately supersedes [./shipped/shell-hookup.lex] §5's "a healthy hookup after `up` is silence": the footer is short, and a deploy that says nothing about activation is the gap INS01 set out to close.

        Line one states whether dodot is sourced in new shell sessions, colour-coded, and when it is not, the hint carries the fix. Line two is the evidence, dimmed. Proposed strings, to be pinned by the implementation and then documented once:

        Footer states, quoted verbatim:
            | State | Line one | Line two |
            | healthy | Shell hookup: dodot is sourced in new shells. | Last loaded 4 minutes ago by dodot 5.6.0. |
            | version skew | Shell hookup: your shells load a different dodot. | Last loaded 4 minutes ago by dodot 5.0.0 — you are running 5.6.0. |
            | stale shell | Shell hookup: this shell predates your last \`dodot up\`. | Last loaded 4 minutes ago by dodot 5.6.0. |
            | shell not loaded | Shell hookup: this shell did not load dodot. | Last loaded 2 days ago by dodot 5.6.0. |
            | never | Shell hookup: no shell has loaded dodot yet. | Never loaded. |
            | verified broken | Shell hookup: a new shell did not load dodot. | Last loaded 9 days ago by dodot ≤5.5.1. |

            :: table align=lll header=1 ::

        Version skew is a new state, and the one the evidence change exists to catch: a stamp or heartbeat whose version differs from the running binary's. It is a warning, not an error — a user mid-upgrade will see it transiently — and its hint names both paths, which is the whole diagnosis in the common case.

        *After `down`*, a footer claiming a healthy hookup is technically true and practically misleading: the hook is wired, but the script it sources now deploys nothing. Rather than special-casing the command, the footer reports the script's *content*: when the generated script carries no pack contributions, line one says so (`Shell hookup: wired, but no packs are deployed.`). That covers `down`, a repository with every pack ignored, and a first `up` that deployed nothing, under one rule.

        *Amended during review.* The line says "no packs", not "the init script is empty", because packs are what the measurement counts. On a Homebrew host the script still carries the `brew shellenv` block from [#4] after a `down` — twenty-odd lines that rewrite `PATH` — so a claim about the file being empty would be false exactly where this epic added the content.

    2.4. Relative Time

        Rendered by the `timeago` crate with default features off — no date-time dependency enters the tree, and the edge cases of hand-rolled "N minutes ago" arithmetic are someone else's maintained problem. dodot has no relative-time rendering today, so this is the first and only place it is defined.


3. The Hookup Diagnosis Probe

    3.1. Why This Is a Separate Command

        Answering "what does `dodot` resolve to at the hook line" requires spawning the user's shell and running their whole rc under tracing. That is heavy, and `status` may not spawn anything at all. It therefore lands as `dodot probe shell-hookup`, on demand, in the existing `probe` family ([../user/commands/probe.lex]) where the slower, heavier-handed introspection already lives.

        :: note ::
            *Superseded by epic `#288`, decision 6.* No `dodot probe shell-hookup` command exists. The diagnosis shipped folded into `dodot probe shell-init`, which now traces the rc **by default**: startup timings and the hook-line verdict answer the same "what did my shell just do" question, so the one-word-apart naming hazard this section weighed was resolved by not minting a second name at all. `--no-trace` opts out; the passive views (a `<PACK[/FILE]>` filter, `--runs`, `--history`, `--errors-only`) never trace. See [../user/commands/probe.lex] §4.

    3.2. PATH at the Hook Line

        The naive approach — spawn the shell with its rc suppressed and read PATH — reproduces PATH *before* the rc runs, so any PATH work the user does earlier in their own rc is invisible and the verdict is wrong whenever it matters most. Truncating the rc at the hook line is worse: the prefix is frequently not valid shell, because the hook can sit inside a conditional, a loop, or a function.

        The shell will report it itself. `PS4` is re-expanded for every traced line, so it can carry the location and the live `PATH`. dodot sets it in the spawned shell's environment, which both shells import before the rc runs:

        Tracing the hook line:

            # zsh — PROMPT_SUBST is required for the expansion, and can be set on the command line
            PS4='+dodot|%N|%i|$PATH> ' zsh -o promptsubst -o xtrace -i -c true

            # bash — PS4 expands by default
            PS4='+dodot|${BASH_SOURCE}|${LINENO}|${PATH}> ' bash -x -i -c true

        :: sh ::

        dodot already knows the rc file (the `shell::rc` ladder) and can find the hook's line number by scanning it, so the trace record to read is addressable rather than searched for. Nothing in the user's rc is parsed — only a location match and a field dodot itself defined — so the usual fragility of scraping xtrace does not apply.

        This is strictly more than the question asks: PATH at *every* line means the probe can also name the line that broke it. It runs the user's real rc, which the INS01 probe already does, and it mutates nothing.

        *Fallback.* An rc that overrides `PS4` or unsets `PROMPT_SUBST` costs us the marker, which is detectable. The fallback is a temporary `ZDOTDIR` holding a copy of the rc with a report line *inserted* before the hook (insertion, not truncation, so the file stays parseable inside any block), the always-read files symlinked in, and `ZDOTDIR` reassigned to the real directory on line one so the rc's own `$ZDOTDIR` references still resolve. bash gets the same effect from `--rcfile`. The user's real files are never written.

    3.3. Resolution Happens in Rust

        Given PATH at the hook line, dodot resolves `dodot` against it *itself* rather than asking the shell. `command -v` silently skips a dangling symlink; dodot reports it, which is exactly the entry that started [#1.1]. The report names each candidate it passed over and why, then compares the winner against the running binary (path and version).

        Verdicts:

            - The hook line never executed — no trace record at that location. The hook is inside a branch that did not run, or the rc `exec`s away before reaching it.
            - `dodot` is unresolvable at the hook line, with the PATH it searched and the entries it skipped.
            - `dodot` resolves to a different binary than the one running, with both paths and versions.
            - `dodot` resolves to the running binary — the hook is sound, and the fault is elsewhere.

        The whole section applies only to the `eval "$(dodot init-sh)"` hook. For the file-source hook the equivalent check is whether that path exists at that point, with no PATH involved — which is the structural reason the file-source form is the recommended one.

    3.4. What `install` Does Not Do

        `dodot install --write` finds the hand-wired `eval` form and reports it as present, wired by hand, and leaves it alone. That behaviour is *retained deliberately*: dodot does not rewrite a hook the user authored, even to a form it prefers. The diagnosis names the trade-off and the manual fix; the choice stays the user's.


4. Homebrew Bootstrap

    4.1. Ask Brew, Do Not Guess

        `brew shellenv` emits the environment block already, and it takes a shell argument. dodot captures its output *at generation time* — during `up`, baked into the init script as static text — so Homebrew remains the authority on its own bootstrap and the shell pays nothing to ask. A brew that changes its block is picked up by the next `up`; a hand-written approximation would silently rot.

        The block is emitted per shell under a guard (`brew shellenv zsh` carries `fpath[1,0]=`, which is zsh-only), or as `sh` for anything else. Absent brew at generation time, nothing is emitted; the next `up` heals it.

        A config key gates it — `auto` (emit when a prefix exists) and `off`. One mechanism, per [./shipped/pack-ordering.lex] §2.2: no second hand-rolled emitter to disagree with the first.

    4.2. Ordering, and a Correction

        The block is emitted *first*, above dodot's own PATH additions, so dodot's contributions are always the last word.

        An earlier concern — that `brew shellenv`'s `path_helper` re-exec demotes dodot's PATH entries — no longer holds. Brew now passes `PATH_HELPER_ROOT`, which changes the behaviour. Measured:

        path_helper with PATH_HELPER_ROOT:

            input:  /aaa-first:/usr/bin:/bin:/zzz-last
            output: /opt/homebrew/bin:/opt/homebrew/sbin:/aaa-first:/usr/bin:/bin:/zzz-last

        :: text ::

        Only brew's own paths are prepended; existing entries keep their order. (Plain `path_helper`, which `/etc/zprofile` runs, *does* reorder — the source of the older advice.) The cost is two process spawns per shell start, measured at ~2.3ms, which buys immunity from brew's bootstrap changing under us.

    4.3. What This Does and Does Not Close

        Issue `#260` names two independent needs: *availability during init* (brew's PATH must exist before pack scripts that call brew-installed commands) and *final precedence* (where brew ranks once init is done).

        This proposal closes the first, for Homebrew specifically, by construction — the block is emitted before anything else, so no pack ordering is required to obtain it. It closes none of the second: the tiered-PATH design `#260` sketches remains open, and a user who needs their personal bins above brew still orders that by hand. The issue should be updated, not closed.

        It also removes the reason a user writes a `001-homebrew` pack at all. The numeric-prefix grammar stays exactly as [./shipped/pack-ordering.lex] specified it; this simply retires the most common occasion for reaching for it.


5. User Stories

    1. As a user whose shells load an old dodot, I want status to tell me the versions differ, so that I stop being told to open a new shell.
    2. As a user, I want to know when dodot last loaded in a shell, so that "it is wired" and "it is working" stop being the same claim.
    3. As a user with a dead hookup, I want one command that names my rc file, the line, and what `dodot` resolves to there, so that I do not have to trace a login shell by hand.
    4. As a macOS user, I want Homebrew's environment set up by dodot, so that my rc file can be empty and my packs can call brew-installed commands.
    5. As a user reading `dodot down`'s output, I want the footer to say the init script now deploys nothing, so that a healthy hookup line does not read as a healthy deployment.
    6. As an operator, I want the diagnosis to run only when I ask, so that `status` stays as cheap and passive as it is today.


6. Cross-Cutting Concerns

    - *Hot path.* One added export. The Homebrew block adds two process spawns per shell (~2.3ms), the first time dodot has put a command on the startup path; the cost is stated in the docs, and `off` is one config key away.
    - *Safety.* The new probe inherits the INS01 envelope unchanged: announced before it runs, stdin from `/dev/null`, output captured, a hard timeout with a process-group kill, `DODOT_INIT_*` scrubbed. It never writes the user's rc; the fallback works on a copy.
    - *Privacy.* The probe prints PATH and binary paths — local, and no more than the user's own shell would show — but it is output the user may paste into an issue. It should print nothing beyond what the diagnosis needs.
    - *Dependencies.* One new crate, `timeago`, default features off.
    - *Migration.* None. A pre-RCS01 heartbeat reads as an unknown version and renders `≤5.5.1`; an existing hook of either form keeps working untouched.

7. Testing and Verification

    The interesting logic is pure and should be reachable without a shell:

        - Trace parsing — locate a record, extract the PATH field, and handle the marker's absence — against captured fixtures from real zsh and bash traces.
        - PATH resolution — first match wins, non-executable and dangling entries skipped and reported — against a fake filesystem.
        - Footer rendering — every state, present and absent versions, present and absent heartbeat — as string assertions, since these are the strings the docs quote.
        - Homebrew block emission — against a fake command runner, so no test needs brew installed.

    Live evidence belongs in `tests/e2e/bats/test_shell_hookup.bats`, which INS01 built for exactly this and which runs in the `ci / e2e` leg: a real bash whose rc mangles PATH before the hook, asserting the probe names the right line and the right binary.

8. Work Stream Hints

    Four streams. WS01 lands first — it makes the epic's motivating failure detectable with no probe at all; the rest are independent of each other.

    1. *Activation evidence v2* — the version export and heartbeat field, mtime as run time, the version-skew state, the two-line footer in the shared renderer, `timeago`. Touches `shell::activation`, `shell::mod`, and the status render.
    2. *The diagnosis probe* — `dodot probe shell-hookup`: trace spawn and parse, PATH resolution in Rust, verdicts, the `ZDOTDIR` fallback. Depends on WS01 only for vocabulary.
    3. *Homebrew bootstrap* — prefix detection, generation-time capture, guarded emission first in the script, the config key.
    4. *Docs* — the activation states and their strings documented once, the new probe's reference page, the Homebrew behaviour and its cost, and the bootstrap guidance in getting-started that a built-in brew block makes obsolete.

9. Out of Scope

    - PATH tiering and final precedence (`#260`).
    - Any rewriting of a user's existing hook ([#3.4]).
    - `status` reporting a measured verdict, which `#279` covers and INS01 §9 forbids.
    - Package managers other than Homebrew. nix, asdf, and mise raise the same bootstrap question; none is answered here.

10. Further Notes

    Related work: [./shipped/shell-hookup.lex] (INS01, the evidence and probe this extends), [./shipped/pack-ordering.lex] (the numeric-prefix contract, unchanged by this), issue `#260` (PATH precedence, partly relieved and not closed), issue `#279` (`status` cannot report a measured verdict), issue `#281` (the tutorial resolves its own rc path).

    The failure in [#1.1] was found by hand-tracing a clean login shell. That every layer of INS01 behaved to spec while the user still had no route to the answer is the argument for this epic: evidence that cannot identify itself is evidence that can only report, never diagnose.
