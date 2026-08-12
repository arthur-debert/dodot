Design Specification: Shell Hookup — dodot's Own Install

    :: note ::
        *Status: proposal.* Epic INS01. Written from the 2026-08-12 planning session; decomposition into work streams happens in a later issue-planning session.

    dodot's deployment model is two-layered: `dodot up` materializes pack state into the datastore, and *shells activate it* by sourcing the generated init script. Everything dodot verifies and reports today lives in the first layer. Nothing verifies the second — and the second is the only one users experience. This proposal closes that gap with three cooperating pieces: an init script that leaves *evidence* it ran, a gated empirical *probe* that measures activation when no evidence exists yet, and a `dodot install` command that wires the shell hook and confirms its own work. Where earlier discussion of this problem reached for heuristics ("guess the rc file, grep it"), the design principle throughout is *evidence over guessing*: the static scan survives only as a diagnostic aid and a write-target selector, never as the verdict.

1. The Problem

    1.1. Deployment Is Not Activation

        A successful `dodot up` proves the datastore is correct: symlinks point at sources, Brewfiles ran, the init script was regenerated. It proves nothing about whether any shell will ever *source* that init script. The two layers fail independently, and the second layer — the `eval "$(dodot init-sh)"` (or equivalent) hook in the user's shell rc — is wired manually, outside dodot's own machinery.

    1.2. The New-User Failure Story

        The canonical failure is silent and lands on exactly the people least equipped to diagnose it:

        1. A new user installs dodot, builds their packs, runs `dodot up`.
        2. Everything reports green. As far as dodot's own output is concerned, they are done.
        3. They open a new terminal. Nothing happens: no PATH additions, no profile scripts, no aliases.

        Nothing failed, so nothing was reported. The missing step — adding the shell hook — is documented, but the tool's own success report actively suggests it is unnecessary. The cost is a first-run experience that undermines trust at the exact moment the user is evaluating the tool.

    1.3. Adjacent Confusions

        Two neighboring failure modes share the same root (the user cannot see activation state) and are solved by the same evidence:

        - *Stale shells*: the user runs `dodot up`, then keeps working in a terminal opened before the run. That shell sourced an older generation of the init script; newly deployed packs are silently absent until they open a new one.
        - *Broken hooks*: a previously working hookup breaks — the rc file was rewritten, the hook line commented out, the shell changed. Deployments keep reporting green while every new shell silently loads nothing (observed in practice after a dotfiles-repo move; see [./../../CHANGELOG/5.3.0.md]).

    1.4. Goals

        - `dodot up` and `dodot status` know, and say, whether shells are actually activating dodot — with distinct messages for "never activated", "activated but stale", and "healthy".
        - A first-run `dodot up` ends with a *measured* verdict on shell integration, not silence.
        - `dodot install` prints the hook and the file it belongs in; `dodot install --write` wires it idempotently and verifies the result.
        - The steady-state cost of all of this, once a hookup is healthy, is effectively zero.

    1.5. Non-Goals

        - Managing the user's rc file beyond the dodot hook block. dodot writes one marked block and touches nothing else.
        - Supporting every shell. The supported set is the set dodot generates init for — POSIX sh/bash/zsh today, fish if/when init generation grows fish syntax. Exotic `$SHELL` values get an honest "can't help, here is the line" rather than a guess.
        - A general-purpose `dodot doctor`. The probe designed here is scoped to shell hookup; a broader diagnostic command may reuse it later but is not specified here.

2. Evidence Over Guessing

    The design's spine is a ladder of signals ordered by reliability and cost. Each rung answers a different question, and the expensive rungs are consulted only when the cheap ones are inconclusive.

    Signal ladder:
        | # | Signal            | Question answered                          | Cost               |
        | 1 | Environment stamp | Is *this* shell live, and on which gen?    | Free (env read)    |
        | 2 | Heartbeat file    | Has *any* shell activated, and when?       | One stat           |
        | 3 | Empirical probe   | Would a *new* shell activate?              | One shell spawn    |
        | 4 | Static rc scan    | Is the hook *configured*, and where?       | File reads         |

        :: table align=clll ::

    2.1. The Init Script Leaves Evidence

        dodot generates the init script, so the init script can prove it ran. Two additions to the emitted script, both unconditional (unlike the opt-in profiling instrumentation that already exists):

        - *Environment stamp*: export a generation marker (working name `DODOT_INIT_GEN`) whose value is stamped into the script at generation time — a monotonic counter or the generation timestamp. Any dodot command inherits the calling shell's environment, so it knows with certainty whether the invoking shell sourced init, and whether that init is current or predates the last `up`.
        - *Heartbeat*: touch a marker file under the existing probes directory (alongside the shell-init profiling probes that `dodot status` already reads) on every source. This is one redirect per shell start; the full profiling TSVs remain opt-in. The heartbeat distinguishes "never activated anywhere" from "activated, just not in this context" — the difference between a loud warning and silence.

        The generation marker also resolves the stale-shell confusion ([#1.3]) for free: env stamp present but older than the current init script means "this terminal predates your last `up`; open a new shell".

    2.2. Why Not Heuristics First

        Rc-file scanning cannot be the verdict because the hookup surface is not enumerable: rc files chain, live behind symlinks (commonly *into the dotfiles repo dodot itself manages*), get sourced from unexpected places, and differ per platform. A scan can be wrong in both directions. Evidence cannot: if the marker is in the environment, init ran. The scan earns its keep in exactly two places — choosing where `--write` writes ([#4.3]), and turning a failed probe into an actionable diagnosis ([#3.3]).

3. The Verification Probe

    When no evidence exists, dodot measures instead of guessing: spawn the user's actual shell, interactively, and check for the environment stamp.

    3.1. Gating — When the Probe Runs

        The probe runs from `dodot up` (and on demand from `dodot install`) only when the cheap signals are inconclusive:

        - Env stamp present and current → this shell is live; no probe.
        - Heartbeat at or after the current init generation → some shell activated since the last regeneration; no probe.
        - Neither → this is precisely the fresh-install or broken-hook case → probe.

        This gate makes the probe self-limiting: it fires on the first `up` after install, and again only if the hookup actually breaks. The first real shell activation writes the heartbeat and the probe retires. Steady-state cost: zero. `dodot status` reports from signals 1–2 (and the last probe verdict) without spawning anything; only `up` and `install` may probe.

    3.2. Mechanics — How the Probe Runs

        The probe must be safe against arbitrary user rc files, which prompt, hang, `exec` into multiplexers, and fail in creative ways:

        - Spawn `$SHELL` as interactive non-login (`-ic`), the mode that reads the rc file the hook lives in, with a command that prints the environment stamp.
        - stdin from `/dev/null`, no tty allocation, stdout/stderr captured.
        - Hard timeout (order of 5 seconds); on expiry kill the whole process group.
        - Scrub `DODOT_INIT_*` from the child environment before spawning — the child inherits the parent's exports, and an inherited stamp is a false positive whenever the probe runs from an already-live shell.
        - Announced, never covert: the probe prints what it is doing (`verifying shell integration (zsh)…`). Every terminal open runs the user's rc anyway; the only unacceptable version of this is a silent one.
        - The probe reads exactly one bit (stamp present and current?) from captured output; rc noise and nonzero exits from unrelated rc breakage do not fail the probe by themselves.

    3.3. Verdicts and Degradation

        Three outcomes, kept distinct because they demand different user action:

        - *Verified*: stamp present and current — report the measured ✓.
        - *Verified broken*: shell completed, no stamp. Run the static scan to split the diagnosis: hook absent from the expected rc ("run `dodot install --write`", with the file named) versus hook present but not reached ("something earlier in `~/.zshrc` is failing before the dodot block").
        - *Couldn't verify*: timeout or spawn failure. Degrade to the static scan's answer, clearly labeled as configuration state, not activation state. Never wedge `up` on a probe.

4. The Install Command

    4.1. `dodot install`

        A new command in the MISC help group ("Wires dodot into your shell startup; --write applies it"). The bare invocation is read-only and prints: the detected shell, the rc file `--write` would target (with symlink resolution shown), whether the hook is already present, and the exact hook line for manual addition. The dry default means the user can always see the decision before delegating it.

    4.2. `--write` and the Marked Block

        `dodot install --write` appends a marked block to the target rc:

        Hook block:

            # >>> dodot shell hookup >>>
            [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh"
            # <<< dodot shell hookup <<<

        :: sh ::

        - *Idempotent*: an existing block is replaced in place, never duplicated. Removal is exact.
        - *Direct source, not eval*: sourcing the generated file beats `eval "$(dodot init-sh)"` on two counts — no binary invocation on every shell start, and no bootstrap cycle when the dodot binary itself is not on PATH until init runs (true whenever dodot is installed via a package manager whose PATH entry dodot's own init provides). The guard makes a missing or reset datastore a silent no-op instead of an error. The emitted path honors the resolved data dir at write time; `dodot install` re-run heals a moved data dir.
        - After writing, the probe runs ([#3]) with the just-written state, so `install --write` ends on a measured verdict, not a promise. `dodot init-sh` remains supported for users who prefer to wire manually.

    4.3. Choosing the Target Rc

        The write target is a guess; the probe is the truth; the marked block makes the guess reversible. The guess only has to be right most of the time, because wrongness becomes a failed verification with a named file, never a silent lie. Resolution ladder:

        1. Shell from `$SHELL` basename (fallback: the login shell from the user database). Unsupported shell → refuse to guess; print the hook line and ask for `--rc`.
        2. Canonical interactive rc per shell:
            - zsh → `${ZDOTDIR:-$HOME}/.zshrc`; when `ZDOTDIR` is not in the environment, peek at `~/.zshenv` for an assignment, since that is where it legally gets set.
            - bash → `~/.bashrc`; on macOS additionally ensure `~/.bash_profile` (or `~/.profile`) chains to `.bashrc`, since Terminal spawns login shells — write the standard chain if absent.
            - fish (when supported) → drop a file into `~/.config/fish/conf.d/`; no rc editing at all.
        3. A missing rc file is created — a fresh machine has no `.zshrc`, and that is still the right target.
        4. A symlinked rc is written *through*, with the resolved target announced ("`~/.zshrc` → `dotfiles/zsh/zshrc` — writing there"). For dotfiles users, landing the hook in their repo is the desired outcome; doing it silently is how trust is lost.
        5. `--rc <file>` overrides the whole ladder.

5. Surfacing in `up` and `status`

    The activation states and their messages, derived from signals 1–2 (plus the probe verdict when `up` just ran one):

    Activation states:
        | State                | Evidence                                   | Surface                                                                 |
        | Healthy              | Stamp/heartbeat current                    | Silence (status may show a quiet "shell hookup: ok")                    |
        | Stale shell          | Env stamp present but older than init gen  | Info: "this shell started before your last `up` — open a new shell"     |
        | Never activated      | No heartbeat ever                          | Prominent: "deployed, but no shell has loaded dodot yet — run `dodot install`" (or the probe's measured verdict when `up` probed) |
        | Verified broken      | Probe ran, no stamp                        | Error with split diagnosis per [#3.3]                                   |

        :: table align=lll ::

    The "never activated" warning fires at the exact moment the new-user story ([#1.2]) goes wrong today: the end of a green first `up`.

6. Cross-Cutting Concerns

    - *Per-shell-start cost*: the evidence additions are one export and one redirect; they must stay that cheap. No command execution, no dodot invocation, on the hot path.
    - *Portability*: stamp and heartbeat must be valid POSIX sh, bash, and zsh — same constraint the emitted init script already carries. Probe process-group handling is Unix; dodot has no Windows target.
    - *No secrets, no telemetry*: the heartbeat records a timestamp/generation locally, nothing else, in the existing dodot data dir.
    - *Concurrency*: heartbeat writes from parallel shell startups must not corrupt (single truncating redirect of static content; last writer wins is fine).
    - *Compatibility*: existing `eval "$(dodot init-sh)"` hookups keep working untouched — the evidence rides inside the emitted script either way. No migration required; `install --write` is additive.

7. Testing and Verification

    - *Init emission*: unit tests over the generated script asserting the stamp export, the heartbeat write, and their unconditionality (existing shell/write_init_script tests are the prior art).
    - *Evidence evaluation*: table-driven tests over the signal ladder — stamp current/stale/absent × heartbeat fresh/old/absent → expected state and message. Pure logic; test exhaustively.
    - *Probe*: integration tests with fabricated `$SHELL` scripts simulating: clean activation, missing hook, rc that hangs (assert timeout kill), rc that exits nonzero but activates, inherited-stamp scrubbing.
    - *Install*: filesystem tests over the rc ladder — fresh file creation, idempotent block replacement, symlink write-through and announcement, `ZDOTDIR` peek, macOS bash chain, `--rc` override.
    - *Acceptance*: the new-user story end-to-end in a container/sandbox — install dodot, `up`, assert the warning; `install --write`, assert the measured ✓ and that a real new shell activates.

8. Work Stream Hints

    Natural decomposition (final slicing belongs to issue planning):

    1. Init-script evidence + state surfacing: stamp, heartbeat, signal-ladder evaluation, `up`/`status` messages. No new command; immediately useful.
    2. The probe + `dodot install [--write] [--rc]`: gating, safe spawn, verdicts, rc ladder, marked block. Depends on 1's evidence definitions.
    3. Docs and onboarding: user docs for `install`, first-run guidance, README/tutorial touchpoints; retire any docs presenting the manual eval as the only path.

9. Out of Scope

    - Auto-writing the rc without `--write` (dodot never modifies shell startup unasked).
    - Probing from `dodot status` (status reports evidence; only `up` and `install` spawn shells).
    - Fish/nushell/etc. init generation (tracked separately; the design leaves room via the rc ladder and conf.d path).
    - Uninstall tooling beyond removing the marked block (`install` owns its block; a broader uninstall story is separate).

10. Further Notes

    - Grew out of the 2026-08-12 incident and planning session that also produced the orphaned-pack sweep and `dodot reset` (released in [./../../CHANGELOG/5.3.0.md]): all three are children of the same lesson — dodot's state model is only as trustworthy as its visibility into what shells actually do.
    - Prior art consulted: starship/zoxide (print-the-eval convention), conda init (managed rc block, and its reputation — hence dry-by-default and the marked block's strict idempotence), rustup (bash_profile chaining on macOS), fzf install (write-then-verify loop), fish conf.d (drop-in directory as the no-editing ideal).
    - Terminology for the grill/issue leg: "hookup" = the rc wiring; "activation" = a shell actually sourcing init; "generation" = one regeneration of the init script; "evidence" = signals 1–2; "probe" = signal 3.
