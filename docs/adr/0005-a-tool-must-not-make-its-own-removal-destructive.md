# A tool must not make its own removal destructive

Uninstalling dodot, or deleting its data directory, must cost the user nothing
beyond dodot itself. State that dodot creates on the user's behalf either lives
where the owning tool keeps it, or is recoverable by re-running dodot. Nothing
the user acquired through dodot may become unreachable because dodot is gone.

The decision this rule settled: dodot installs Nix packages into the user's
default profile via `nix profile`, and does not point Nix at a profile of its
own. An owned profile is attractive — a fresh profile carries no `nix-env`
manifest to obstruct it, `--remove-all` becomes available, and `dodot down`
would gain a real uninstall story. Sited inside dodot's data directory, it also
disappears when dodot does: the installed packages leave `PATH` immediately, and
their store paths are left for the next garbage collection to reclaim. The user
who removes a dotfiles manager loses a set of programs they never asked dodot to
own.

The rule generalizes past Nix, which is why it is recorded separately from the
decision that produced it. Any design where dodot becomes the sole path back to
something the user has — a package set, a fetched artifact, a captured
credential, a rewritten file with no source — inherits the same objection. The
question to ask of such a design is what a user loses by deleting dodot, and the
answer has to be *dodot*.

It does not forbid dodot from holding state. The datastore holds sentinels,
snapshots, and intermediate symlinks, and deleting it costs a re-run of
`dodot up`, not a reinstall of anything. That is the line: recoverable
bookkeeping on one side, sole custody of the user's things on the other.

Siting an owned profile at a durable Nix-managed location would escape this
rule — the profile would outlive dodot, still rooted, with only the `PATH` entry
lost. It was declined on a separate ground: it makes Nix the one provisioner
that needs a profile path, a `PATH` contribution, and an uninstall story,
which is a permanent exception to the uniform descriptor model built for
exactly one row.

Reasoning: `docs/proposals/provisioning-handlers.lex` §7.3.
