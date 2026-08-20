# A fitness probe spawns, so it is `up`-only

Asking whether a package manager *exists* is a `stat`. Asking whether it is well
enough to use — a version floor, a working `--version` — needs the manager to
answer, which means spawning it. Those are two modules, not two arms of one
function, and only `dodot up` may reach the second.

`provisioners::availability` is the presence probe of ADR-0008: three outcomes,
read from mode bits, called by both the pack planner and `dodot status`.
`provisioners::fitness` is this one. Its single caller is `commands::up`, behind
`!ctx.dry_run`.

The boundary is what makes `dodot status` a passive command rather than a
command that happens not to write anything today. Status builds its context with
a `NoopCommandRunner` and is pinned byte-identical against the datastore, and it
reaches the presence probe on *every* provisioning row. If the version question
lived in the same module, every `dodot status` on a machine with three
`Brewfile`s would spawn brew three times, and the pin that catches it would be a
test nobody thought to write. Keeping the spawning question in a module status
does not import means the mistake is a compile-time absence rather than a
review-time catch. A dry run is excluded for the parallel reason: a command whose
whole promise is to report what it *would* do cannot start the user's package
manager to find out.

## What it asks, and why it has to

The first — and so far only — fitness question is Homebrew's version floor. A
`Brewfile` may declare `go`, `cargo`, `uv`, `npm`, and `krew` entries, and dodot's
position that the language-bound package managers are Homebrew's job rather than
dodot's rests on exactly that. Those five entry types arrived across five
releases, the last of them (`npm`, `krew`) in Homebrew 5.1.2 on 2026-03-30. Below
that, a line the user was entitled to write comes back as a parse error out of
`brew bundle` that names neither the version nor the fix.

This would normally self-correct: brew auto-updates before most commands, which
carries a stale installation over the floor on its own. dodot sets
`HOMEBREW_NO_AUTO_UPDATE` for provisioning commands, so on a dodot-managed run it
does not — an old brew stays old. Having taken the self-correction away, dodot
owes the user the check.

The probe therefore spawns the same executable the run is about to spawn — the
absolute path the presence probe found, carried on the intent — with the same
environment rows. `HOMEBREW_NO_AUTO_UPDATE` on the version question is not
incidental: without it, reading brew's version would trigger the very update this
project turned off, and the answer would describe a brew that did not exist a
moment earlier.

## Checked whenever the manifest is there, not when it needs it

Only a `Brewfile` that actually uses one of the newer entry types is affected, so
checking only those would be friendlier. dodot does not.

Knowing would mean reading and parsing the user's `Brewfile`, and a run-once
manifest is opaque to dodot by design: its bytes are hashed, snapshotted, and
handed to the manager, never interpreted. `RunOnceCommand` builds a command from
a path; nothing in that lifecycle reads content, and adding a `Brewfile` parser to
avoid a subprocess would buy a small kindness with a dependency on Homebrew's
manifest grammar that dodot would then own forever.

A version floor is a fact about the machine, and the machine is what this module
is allowed to look at.

## It reports; it does not refuse

Because the check is not conditioned on content, a below-floor brew is not
evidence that anything will fail. A `Brewfile` of plain `brew` and `cask` lines
runs perfectly on a brew from two years ago. Refusing the run would convert a
warning dodot is not sure about into a deployment this machine cannot perform —
and dodot would be wrong most of the times it fired.

So the condition is named, with `brew update` as its remedy, *before* the file
runs, and then the file runs. If a `go` line does fail afterwards, the
explanation is already on screen. A manager at or above its floor produces no
output at all.

`ProbeFailed` — a `--version` that exits non-zero, prints nothing, or answers
`>=4.1.0 (shallow or no git repository)`, which is a lower bound rather than a
version — is its own outcome, for the same reason ADR-0008 keeps probe failure
out of absence. Reading a lower bound as its floor would tell a user on a current
brew to update; folding it into "fit" would claim dodot checked something it did
not.

## Ephemeral, and paid for once

A fitness answer is never written to the datastore and never becomes part of a
receipt. A receipt asserts *this exact file content ran successfully*; the
manager's version at that moment is not part of that claim, and recording it
would mean a `brew update` invalidated receipts it has nothing to do with.

Where ADR-0008 says nothing is cached — three `Brewfile`s are three `stat` calls
— this module is the place that ADR pointed at for a cache, because here the cost
is a subprocess. `up` asks once per (manager, executable) pair per run, and does
not ask at all about a file whose receipt is already current: on the second `up`
of the day the `Brewfile` is not going to run, and a subprocess bought to warn
about a run that is not happening is a subprocess wasted.

Reasoning: `docs/proposals/provisioning-handlers.lex` §1.1, §4.2.
