# Availability is three outcomes, read from one probe

Asking whether a provisioner's manager is on this machine has three answers, not
two, and every caller reads them from the same function.

**Present** proceeds, and the run spawns the path that answered. **Absent**
skips: no receipt, no error, no effect on the exit code, and a row naming the
manager and every location probed. **Probe failed** — a candidate that could not
be examined at all — is a real error, surfaced with its detail and never
absorbed into absence: a real `dodot up` reports it in its exit code, while
still deploying the rest of the pack. Two outcomes would force a permission
error on `/opt/homebrew` to report as "Homebrew is not installed", which is a
different machine, a different remedy, and a lie.

Absence writing no receipt is what makes the skip self-healing. A receipt asserts
*this exact file content ran successfully*; absence is a fact about this machine
right now. Conflating them would mean installing brew later required clearing
state first. Because nothing is written, installing the manager and re-running
`dodot up` runs the file — no flag, no cleanup.

## Not a handler method

`RunOnceCommand::validate` existed for exactly this and had no implementor. It
could not have worked. `validate` is consumed inside `RunOnceHandler::to_intents`,
so a finding raised there dies with the intent, while
`commands/status.rs::run_once_health` derives a row from the source file and the
datastore without ever calling a handler. An absent manager would have rendered
as `never ran` — indistinguishable from one nobody has run yet, with nothing
naming where dodot looked. Widening its `Result<()>` signature would have fixed
the arity and left the row unreachable. The hook is therefore deleted rather than
widened, and availability is a shared module (`provisioners::availability`) that
the pack planner and status both call with the same inputs.

Agreement between those two callers is the requirement, so it is a test and not
a note: for each of present, absent, and probe-failed, `dodot up`, `dodot
status`, and `dodot up --dry-run` must report the same thing about the same
machine. The planner is also the only place a skip can be *reported* — an
absent manager's rows come from the plan, because `to_intents` returns intents
and nothing else.

## Presence is stat-only; fitness is what spawns

The probe stats candidates and reads mode bits. It spawns nothing, and that is a
contract rather than an optimization: `dodot status` carries a real
`ShellCommandRunner` in production like every other command — what keeps it
passive is that its call graph never reaches one that spawns — it is pinned to
leave the datastore byte-identical, and status reaches this module on every
provisioning row. Asking a manager whether
it is well enough to use — a version floor, a working `brew --version` — is a
separate fitness probe that spawns and is therefore `up`-only. An implementer
who reaches for `brew --version` from the presence module has broken the
passive-command contract, not just this module.

Nothing is cached within a run. Re-probing three packs' `Brewfile`s is three
`stat` calls, and staying a pure function of `(fs, host, handler)` is what makes
the planner and status agree trivially. A cache belongs with the fitness probe,
where the cost is a subprocess.

## Ranking, when a receipt and a machine disagree

An availability travels as an ephemeral diagnostic on a plan and on a status
row, never as anything persisted, and it meets the datastore's three run-once
states at the row. Where they conflict:

- A **probe failure** wins every verdict. dodot could not answer the question,
  and a row reporting anything else would be inventing an answer.
- **Absence with a receipt** annotates without changing the verdict: brew ran
  this `Brewfile`, and brew leaving afterwards does not undo that. The
  precedent is `Health::RecentFailures`, which attaches a warning footnote to a
  row that stays deployed.
- **Absence with no receipt** is the skip.

These are `Health` variants rather than new status strings, deliberately.
`Health`'s `style`, `label`, and `footnote` arms are exhaustive matches the
compiler checks; an unrecognized status string falls into `aggregate_status`'s
catch-all and silently contributes to no summary bucket, and the theme is
defined twice — YAML in `dodot-lib`, CSS in `dodot-cli` — with nothing keeping
the two in step.

Reasoning: `docs/proposals/provisioning-handlers.lex` §4.1, §4.2, §6.3.
