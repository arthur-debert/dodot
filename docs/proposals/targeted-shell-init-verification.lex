Design Specification: Targeted Shell-Init Verification

    :: note ::
        *Status: proposed.* Epic code unassigned. This is a focused successor to [./shipped/shell-hookup.lex] and [./shell-hookup-ergonomics.lex]. It retains their activation evidence, process-group timeout, and PATH-at-the-hook diagnosis. It replaces only the requirement that routine verification and a bare `dodot probe shell-init` run the user's complete rc file.

        Until this proposal ships, the code and current user documentation remain authoritative: `dodot up` and `dodot install --write` may run the full rc to verify activation, and a bare `dodot probe shell-init` runs the full rc under tracing by default.

    dodot needs to answer one small question after creating or inspecting a shell hookup: *did a new interactive shell reach the dodot init script?* The current implementation answers a larger question. It waits for the entire rc file to finish, and `probe shell-init` additionally traces every command so it can reconstruct `PATH` at the hook line. That diagnosis is valuable when PATH resolution is broken, but it is unnecessary work for routine activation verification and can time out after dodot's hook has already run.

    This proposal gives routine verification its own narrow protocol. A child shell carries a one-use nonce; the generated init script recognizes it before any pack contribution runs, reports its generation and dodot version, and exits the child shell immediately. The result says whether the hook was reached, which init script answered, and how long reaching it took. Full hook-resolution tracing remains available only when explicitly requested.


1. The Problem

    1.1. Three Questions Share Two Implementations

        The current shell-hookup code answers three different questions:

        Current questions and mechanisms:
            | Question | Current mechanism | Required work |
            | Is this shell, or any recent shell, using dodot? | Environment stamp and heartbeat | Environment and file reads only |
            | Would a new interactive shell reach dodot's init script? | Start `$SHELL -ic`, run the complete rc, print the resulting stamp | Every command in the rc and every deployed pack contribution |
            | What does `dodot` resolve to at the hook line? | Start the shell with PS4 tracing; retry against a temporary rc copy when necessary | The complete rc once or twice, trace capture and parsing, PATH resolution, sometimes another binary's `--version` |

            :: table align=lll header=1 ::

        The first question belongs to passive status. The second is the routine verification needed after `up`, after `install --write`, and in the ordinary `probe shell-init` report. The third is diagnosis for a PATH or hook-location failure. Treating the second and third as the same operation makes the common check inherit the diagnostic operation's cost and failure modes.

    1.2. The Timeout Reports the Wrong Requirement

        The shell process currently gets five seconds to *finish starting*. A timeout is rendered as:

        Current timeout:

            Hook resolution ~/.zshrc:6 (zsh)
              could not trace — your shell did not finish starting up in time

        :: text ::

        Finishing startup is not the requirement. If line 6 sourced `dodot-init.sh`, activation succeeded even when a completion framework, prompt, network call, multiplexer, or command later in the rc takes longer than five seconds. Waiting for those operations adds cost without increasing confidence in the hookup.

        The inverse matters too. A shell that finishes quickly but never reaches the hook is broken. Verification should report the hook event itself, not use process completion as a substitute for it.

    1.3. The Verification Cost Is Missing from the Report

        `probe shell-init` reads the most recent profile before it starts the live hook trace. The displayed profile therefore describes an earlier shell. The trace can create a newer profile, but the command does not display it, and the trace result carries no elapsed duration.

        This produces four misleading outcomes:

        - The displayed grand total excludes the live verification step the command just performed.
        - A successful report does not say whether hook verification took 8 ms, 200 ms, or 4.9 seconds.
        - A timeout reports the five-second limit but not where time was spent.
        - A shell killed during an instrumented source can leave an incomplete profile that renders without an honest completion state.

    1.4. Goals

        - Verify that a new interactive shell reached the dodot init script, without sourcing pack contributions or waiting for the rest of the rc.
        - Preserve generation and dodot-version identity, so a response from an older or different dodot is not reported as healthy.
        - Measure and report the targeted verification's wall-clock duration.
        - Keep `dodot status` passive and keep `up`'s existing evidence-first decision about whether a measurement is needed.
        - Keep full PATH-at-the-hook diagnosis available as an explicit operation.
        - Use one verification implementation for `up`, `install --write`, and `probe shell-init`.

    1.5. Non-Goals

        - Proving that every pack contribution completed. Shell-init profiles and per-file exit statuses answer that question.
        - Making the user's real shell startup faster. This proposal removes unnecessary work from dodot's verification process; it does not optimize Atuin, completion, prompts, or other pack code.
        - Running a complete login-shell sequence. Dodot verifies the interactive rc that contains the hook, matching the existing `-ic` behavior.
        - Removing hook-resolution tracing. The trace remains the right tool when the targeted verifier reports no marker or a different dodot.
        - Adding a user-facing configuration option. The verification protocol is internal behavior, not a shell customization feature.


2. Separate Answers, Separate Interfaces

    The shell-hookup design has three modules. Each module returns the evidence for its own question and does not silently perform another module's work.

    Module responsibilities:
        | Module | Answer | May start a shell? | Typical callers |
        | Activation evidence | What this shell and the most recent observed shell loaded | No | `status`, `up`, `down`, `install` |
        | Targeted verification | Whether a new interactive shell reached a specific dodot init script, with identity and elapsed time | Yes, at most once | `up`, `install --write`, bare `probe shell-init` |
        | Hook-resolution trace | Whether the hook line ran and, for the eval form, which `dodot` PATH selected there | Yes, explicitly requested; fallback may start it again | `probe shell-init --trace-hook` |

        :: table align=llll header=1 ::

    Activation evidence remains the default source for shared status. Targeted verification is the small dynamic check. Hook-resolution tracing is diagnosis after the small check cannot explain a failure.


3. The Targeted Verification Protocol

    3.1. One-Use Challenge

        The parent creates an unpredictable nonce and passes it to the child shell in an internal environment variable. Working name: `DODOT_INTERNAL_SHELL_INIT_PROBE`.

        A fixed boolean such as `DODOT_VERIFY_SH_INIT_PROBE=1` would work mechanically, but an inherited or accidentally exported value could make a real user shell exit during startup. A fresh nonce narrows the special behavior to one child process and lets the parent reject unrelated output that happens to resemble a probe response.

        The child still starts through the existing defensive process runner:

        - `$SHELL -ic`, because the hook lives in the interactive rc.
        - stdin from `/dev/null`; stdout and stderr captured concurrently.
        - `DODOT_INIT_*` removed before launch so inherited evidence cannot pass the check.
        - A dedicated process group, killed on timeout.
        - The probe announced before the shell starts.

    3.2. Response at the Start of the Init Script

        A generated file-source init script checks for the challenge before it writes the heartbeat, opens a profiling file, adds PATH entries, or sources a pack contribution. Generation and version values are literals already known when dodot writes the script.

        Illustrative emitted branch:

            if [ -n "${DODOT_INTERNAL_SHELL_INIT_PROBE-}" ]; then
                printf 'dodot-shell-init-probe:%s|1786998535|5.7.0\n' \
                    "$DODOT_INTERNAL_SHELL_INIT_PROBE"
                exit 0
            fi

        :: sh ::

        `exit`, rather than `return`, is deliberate. The verification child has answered the question, so it should not continue through the rest of the init script or the rest of the rc. This branch is safe in normal shells because only the child receives the nonce.

        The response contains:

        - The nonce, proving the line belongs to this invocation.
        - The init-script generation, compared with the generation on disk.
        - The dodot version that generated the script, compared with the running dodot.

        The branch performs no external command. `printf` and `exit` are shell builtins in the supported shells.

    3.3. The Eval Hook

        The hand-written `eval "$(dodot init-sh)"` form has to invoke a dodot binary before any generated shell code exists. When `dodot init-sh` sees the one-use challenge, it emits only the response-and-exit branch for its own generation and version. It does not load pack metadata, capture Homebrew state, or generate the ordinary init body.

        This preserves the important version-skew answer. If PATH at the hook line selects a current dodot that understands the protocol, that binary identifies itself in the response. If it selects an older dodot that does not understand the protocol, the old command emits its normal script; the existing post-rc stamp command remains as a compatibility fallback and reports any `DODOT_INIT_GEN` and `DODOT_INIT_VERSION` that script exported. A pre-version script reports no usable identity and is not certified as current.

    3.4. Outcomes

        Targeted verification returns a typed result with the shell, resolved rc, hook line when statically known, elapsed duration, and one of these outcomes:

        Verification outcomes:
            | Outcome | Meaning | Next action |
            | Reached current init | Matching nonce, a generation current relative to the reference, and the running dodot version | Report success and elapsed time |
            | Reached different init | Matching nonce or fallback stamp, but generation or version differs | Report version or generation skew; offer full trace when PATH diagnosis can help |
            | Shell completed without evidence | The rc finished without a matching response or usable fallback stamp | Static scan distinguishes a missing hook from a hook that did not run |
            | Timed out before evidence | No matching response arrived before the timeout | Report that the hook was not reached within the limit; offer full trace |
            | Spawn failed | The shell could not start | Report the operating-system error and retain the passive evidence result |

            :: table align=lll header=1 ::

        A matching response ends the child immediately, so there is no successful state in which dodot waits for shell completion after reaching the hook.


4. Command Behavior

    4.1. Shared Status

        `dodot status` continues to read the environment stamp, heartbeat, and static rc state. It never starts a shell. The shared footer remains evidence-based and keeps its current activation states and wording.

    4.2. `dodot up`

        `up` retains its evidence-first decision:

        - A current environment stamp or fresh heartbeat is sufficient; no shell starts.
        - Missing or stale evidence permits one targeted verification after the init script is written.
        - A successful response refreshes the same measured activation verdict `up` reports today.
        - A timeout no longer means “the shell did not finish”; it means “the hook was not reached within the verification limit.”

        The probe-mode branch does not write the heartbeat. A verification process should not masquerade as an ordinary user shell activation; the measured result belongs to the command that requested it, while the heartbeat remains evidence from real shell use.

    4.3. `dodot install --write`

        `install --write` starts one targeted verification after changing the rc, as it starts one full activation probe today. This checks the exact state it just wrote without running any pack contribution. A bare `dodot install` remains passive.

    4.4. `dodot probe shell-init`

        The bare command returns two adjacent but independently timed answers:

        - The latest recorded complete startup profile: what a real shell previously spent inside the generated init script.
        - A fresh targeted verification: whether a new interactive shell reaches the hook now, and how long that took.

        Example:

            Shell-init profile
              run: profile-1786998735-77927-18437.tsv
              grand total          131.3 ms

            Hook verification ~/.zshrc:6 (zsh)
              reached dodot-init.sh in 8 ms
              generation 1786998535, dodot 5.7.0

        :: text ::

        The live result carries numeric elapsed microseconds in structured output and a human duration in terminal output. Timing begins immediately before process creation and ends when the matching response is parsed, on timeout, or on spawn failure.

        `--no-trace` remains accepted as a compatibility spelling for a passive report during migration. Help marks it deprecated in favor of `--no-verify`, whose name describes the new behavior.

    4.5. Explicit Hook-Resolution Trace

        `dodot probe shell-init --trace-hook` performs the existing PS4/PATH diagnosis. It replaces the targeted verification for that invocation rather than running both. It is the operation to run when targeted verification reports a different dodot, no evidence, or a timeout and the user needs to know what happened at the hook line.

        The trace report includes its own wall-clock duration and whether it used the temporary-copy fallback. It does not present its cost as part of the recorded init profile. A fallback produces a second attempt and the report says so.

        Passive historical views remain passive: `<PACK[/FILE]>`, `--runs`, `--history`, and `--errors-only` never start a shell and reject `--trace-hook` when the combination would have no coherent meaning.


5. Performance and Profile Truth

    5.1. Cost Scales with Work Before the Hook

        Targeted verification runs only shell startup work that precedes the dodot hook. For an rc whose first executable line is the managed hook, no pack, prompt, completion, or command after the hook contributes to its duration.

        A slow command before the hook still delays or times out verification. That is a correct result: the new shell did not reach dodot within the limit. Full hook tracing can then name the hook location and help the user inspect the preceding rc work.

    5.2. Verification Does Not Create a Startup Profile

        The probe response occurs before profiling initialization, so targeted verification does not add a synthetic short run to shell-init history. Profiles remain observations of ordinary activation, not records of dodot's own checks.

        Full tracing still runs the ordinary init path and may create one or two profiles. Those profiles remain ordinary startup observations, while the trace's separate elapsed field reports the diagnostic operation's actual wall time.

    5.3. Incomplete Profiles Are Explicit

        A profile without its closing `end_t` record is incomplete. Readers must not render it as a zero-duration completed run or select it as the latest complete profile for the default report. History may list it as `incomplete`, preserving the rows that finished before interruption, but totals that require `end_t` remain unknown.

        Naming the source that was active at interruption would require a new begin/end record format and is outside this proposal. The honest result with the current format is “incomplete after <last completed source>”, not a guess about the next one.


6. Compatibility and Safety

    - Existing file-source and eval hooks remain valid; users do not edit their rc.
    - The first verification against an older generated script may run the complete rc because that script does not recognize the challenge. Its exported generation/version still feeds the compatibility fallback. The next `up` writes the short response branch.
    - The one-use nonce is internal, unguessable for practical purposes, and scoped to the child environment. User shells never retain it.
    - Probe mode writes neither heartbeat nor profile, so a diagnostic process cannot become evidence of ordinary shell use.
    - The current timeout and process-group kill remain the upper bound for shells that never reach the hook.
    - The response contains only a nonce, generation, and dodot version. It does not expose PATH, cwd, pack names, or user-authored shell output.
    - The response branch uses POSIX-compatible syntax and is exercised under every shell supported by init generation.


7. Testing and Verification

    7.1. Protocol Tests

        - Generated file-source scripts carry the response branch before heartbeat, profiling, PATH, Homebrew, and pack contributions.
        - `dodot init-sh` emits only the response-and-exit script when the challenge is present.
        - A matching nonce is required; stale, missing, and malformed responses are rejected.
        - Generation and version skew produce distinct typed outcomes.
        - Probe mode writes no heartbeat or profile.

    7.2. Real-Shell Acceptance Tests

        - Bash and zsh rc files with the hook followed by `sleep 10` verify promptly; the sleep never runs.
        - A deployed pack contribution containing `sleep 10` never runs during targeted verification.
        - A `sleep 10` before the hook reaches the timeout and reports that the hook was not reached in time.
        - File-source and eval hooks both return the current generation/version.
        - An eval hook resolving an older fabricated dodot reports version skew rather than success.
        - `status`, passive probe views, and `--no-verify` start no shell.

    7.3. Timing and Rendering Tests

        - Terminal output prints the targeted verification duration on success, timeout, and spawn failure.
        - Structured output carries numeric elapsed microseconds and a stable outcome discriminator.
        - `--trace-hook` reports trace duration and fallback use separately from startup-profile totals.
        - Incomplete profiles render as incomplete and are not selected as the default latest complete run.
        - E2E assertions cover real process behavior; Rust tests alone are insufficient for shell exit, timeout, and process-group behavior.


8. User Stories

    1. As a new user, I want `up` to confirm that a new shell reaches dodot without waiting for my entire shell setup, so a slow prompt or completion system does not turn successful activation into an inconclusive warning.
    2. As a user diagnosing startup performance, I want `probe shell-init` to distinguish recorded startup cost from the live verification cost, so its numbers describe the work they claim to measure.
    3. As a user with a broken PATH at the hook line, I want full hook tracing when I explicitly request it, so the detailed diagnosis remains available without becoming every probe's default cost.
    4. As an operator, I want `status` to remain passive and verification processes to leave no heartbeat or profile, so observation does not change the evidence being observed.
    5. As a maintainer, I want `up`, `install --write`, and `probe shell-init` to use one verification module, so their verdicts, timeouts, and version comparisons cannot drift apart.


9. Work Stream Hints

    Natural vertical slices for issue planning:

    1. Targeted verification end to end: challenge/response emission for file-source and eval hooks, one shared typed verifier with elapsed time, adoption by `up`, `install --write`, and bare `probe shell-init`, plus real-shell acceptance coverage.
    2. Explicit hook diagnosis: move the existing PS4/PATH trace behind `--trace-hook`, retain the passive compatibility flags, report trace duration and fallback attempts, and update the command documentation.
    3. Honest interrupted profiles: classify incomplete records, exclude them from the default latest-complete selection, render them accurately in history, and add parser/rendering regression coverage.


10. Further Notes

    - [./shipped/shell-hookup.lex] remains authoritative for activation evidence, the evidence-first decision in `up`, rc selection, and `install --write` ownership.
    - [./shell-hookup-ergonomics.lex] remains authoritative for version-stamped evidence and PATH-at-the-hook diagnosis. This proposal supersedes its epic decision 6 only: full tracing is no longer the default behavior of bare `probe shell-init`.
    - [./shipped/profiling.lex] remains authoritative for ordinary shell-init profile records and historical views, with [#5.3] adding the missing incomplete-record rule.
    - The observed performance problem came from a minimal zsh rc whose first executable line sources the managed init script. That shape makes the distinction especially clear: reaching the hook is cheap; running every deployed contribution and the rest of the rc is a separate operation.
