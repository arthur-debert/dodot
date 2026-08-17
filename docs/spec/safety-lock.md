# Safety Lock

## Context

Dodot treats the dotfiles root as the source of truth for every pack-based
operation. When `DOTFILES_ROOT` is not set, the CLI currently infers that root
from the enclosing Git repository and then from the current working directory.
This convenience lets users invoke Dodot from anywhere inside their dotfiles
repository, but it also lets an unrelated repository become a plausible
dotfiles root without an explicit act of selection.

The existing processing pipeline makes that mistake consequential. Every
eligible top-level directory becomes a pack; rules assign pack entries to
handlers; handlers produce intents; and mutating commands dispatch the
resulting operations into the datastore and the user's environment. Shell and
PATH state is then included in the generated init script loaded by future shell
sessions. Code-execution handlers may also run install scripts, package
manifests, and other provisioning inputs.

The repository's storage contract keeps Dodot bookkeeping out of the dotfiles
root. Durable operational state belongs under the Dodot data directory, while
the dotfiles root contains user-authored source. Safety Lock must preserve that
separation.

The current behavior and vocabulary are governed by:

- [Terms and Concepts](../reference/terms-and-concepts.md), especially
  dotfiles root, pack, handler, intent, operation, datastore directory, and
  shell integration;
- [Architecture](../reference/architecture.md), including the unified,
  previewable pack pipeline;
- [Storage](../dev/storage.md), including the datastore ownership invariants;
- [CLI Output](../dev/cli-output.md), including structured stdout and
  presentation-free command results; and
- the command documentation for [`up`](../user/commands/up.md),
  [`down`](../user/commands/down.md), and
  [`status`](../user/commands/status.md).

This Spec exists because an accidental `dodot up` from the Dodot source
repository selected that repository's Git top-level as the dotfiles root. Its
`bin` directory became a pack, and `bin/setup-dev-env.sh` matched the default
shell rule. The generated init script subsequently sourced a valid shell
program that changes shell options and exits, preventing new shells from
remaining open. An already-running shell was the only convenient recovery
path.

## Problem

Dodot currently treats an inferred root as sufficient authorization for
mutation. Filesystem shape is mistaken for user intent: an unrelated Git
repository containing ordinary directories, shell programs, install scripts,
or configuration files looks enough like a dotfiles repository to deploy.

The failure is broader than shell startup:

- shell files can be added to every future shell session;
- PATH directories can alter command resolution;
- provisioning handlers can execute user-authored programs or package
  manifests;
- symlink and external handlers can alter user-visible paths; and
- a wrong-root `down` can treat legitimate datastore packs as orphans and
  remove their state.

Existing safeguards do not establish root intent. Cross-pack conflict checks
protect competing destinations, dry runs preview operations only when the user
asks for them, and shell validation checks syntax rather than runtime meaning.
None distinguishes an intended dotfiles root from an accidentally selected
repository.

Root selection is also implemented in more than one place and does not expose
its provenance as a stable domain result. In particular, a set but unusable
`DOTFILES_ROOT` can fall through to implicit discovery, turning an explicit
configuration error into a different root selection.

## Goals

- Require an explicit act of trust before an implicitly discovered dotfiles
  root can drive root-sensitive mutation.
- Preserve the convenient Git-top-level and current-directory discovery paths
  for read-only inspection and interactive use.
- Treat a valid, explicitly set `DOTFILES_ROOT` as deliberate selection, while
  failing clearly when the explicit value is unusable instead of falling back
  to another root.
- Make the exact resolved root and how it was selected visible before the user
  approves it.
- Give the user concise orientation about the selected root: counts and a
  bounded list of files assigned to high-impact handlers such as shell and
  code execution. This inventory helps the user recognize a wrong directory;
  it is not a deployment preview or a security analysis.
- Fail closed when an untrusted implicit root reaches a root-sensitive
  mutation without an interactive terminal.
- Keep read-only commands and dry runs available on untrusted roots so users
  and agents can diagnose what Dodot sees safely.
- Preserve terminal, text, JSON, YAML, and term-debug output contracts by
  keeping interactive safety communication off structured stdout.
- Support multiple independently trusted roots without requiring one to be
  selected as globally preferred.
- Keep trust state outside the dotfiles root and make that state inspectable
  and revocable.
- Apply the protection consistently to every command whose mutation depends on
  the resolved dotfiles root, rather than relying on individual handlers to
  remember the policy.

## Non-Goals

- Evaluating whether repository contents are safe.
- Statically proving that a shell source is semantically safe. Programs such as
  `exit`, `exec`, traps, or shell-option changes remain the pack author's
  responsibility after the root is deliberately trusted.
- Making `up` or `down` transactional, or rolling back partial effects from an
  operational failure.
- Isolating the datastore by root or arbitrating ownership between multiple
  roots that contain packs with the same identity.
- Removing Git-top-level or current-directory root inference.
- Replacing `status` or `up --dry-run` as the detailed deployment inspection
  surfaces.
- Rendering templates, reading file contents, resolving secrets, or proving
  that the prompt inventory describes every eventual deployment operation.
- Turning `--force` into a root-trust or noninteractive-consent flag.
- Requiring an interactive confirmation when a valid `DOTFILES_ROOT` already
  expresses deliberate selection.
- Repairing a shell environment damaged by a deployment that predates Safety
  Lock.

## Proposed Shape

Safety Lock introduces one pre-mutation flow with three cooperating modules.

The **root selection module** resolves one canonical dotfiles root together
with its provenance. All consumers use the same result. Explicit environment
selection is authoritative and validated; implicit selection retains the
existing Git-top-level and current-directory behavior. Resolution happens once
per invocation. Trust lookup, the confirmation prompt, and execution carry
that same immutable path-and-provenance value rather than consulting the
environment, Git, or current directory again.

The **root trust module** answers whether an implicitly selected canonical root
has been approved by the current user. It owns durable trust state outside the
dotfiles root, supports more than one root, and provides the operations needed
to establish, inspect, and revoke trust. Trust represents deliberate root
selection, not the success of any particular deployment. Approval does not
expire with time, Dodot upgrades, or command failures; it remains until the
user explicitly revokes it with `dodot roots forget <path>` or removes
Dodot-owned state through `dodot reset`. `dodot roots list` is the inspection
surface. Approval itself occurs only through the normal safety gate; there is
no separate trust verb. Forgetting an existing path canonicalizes it before
matching, while an absent path can match its exact stored absolute spelling so
approvals remain removable after a root is moved or deleted.

A trust record identifies its root by the canonical path's native
operating-system form, stored losslessly. A path that is valid UTF-8 is stored
as a plain string; one that is not is stored in a tagged, reversible encoding
of its native units, so trust state and JSON or YAML output remain valid
documents without discarding bytes. A lossy rendering is never the stored
identity nor the match key: two roots whose lossy renderings collide stay two
records, `roots list` prints the exact reversible spelling of each, and `roots
forget` accepts either an existing path, which it canonicalizes, or that exact
spelling for a root that no longer exists. Prompts and diagnostics name the
root in the same reversible spelling, so the identity the user approves is the
one they can later inspect and revoke.

Corrupt, unreadable, or incompatible trust state never counts as approval and
never silently becomes an empty registry. Root-sensitive mutation fails with a
diagnostic naming the affected state and recovery choices. `roots list`
surfaces the problem, `roots forget` removes an identifiable affected record,
and factory `reset` remains the recovery route when narrower revocation is not
possible.

The **CLI safety gate** classifies commands by whether they perform a
root-sensitive mutation: a requested mutation of repository, deployment, or
user-visible state whose target is selected from the resolved dotfiles root.
Deployment and repository-writing commands cross this gate. Incidental
diagnostic logs or caches do not make an otherwise diagnostic command
root-sensitive. Mutations based only on other coordinates, such as the user's
home directory or Dodot's global data directory, retain their own safeguards
but do not require root trust merely because they also construct an execution
context. Before a root-sensitive command crosses into mutation, the gate
accepts explicit root selection, recognizes prior trust, or asks an interactive
user to approve the resolved root. Refusal, missing interactivity, invalid
configuration, or failure to persist approval prevents the requested command
from mutating. Read-only commands and dry runs bypass the gate without
establishing trust.

The initial root-sensitive taxonomy includes mutating `up` and `down`; `init`,
`fill`, `adopt`, and `addignore`; configuration actions whose persistence target
is the selected root; mutating `refresh` and `transform check`; repository-local
`git-install-filters`, `template install-filter`, and `transform install-hook`;
and the tutorial's real deployment step. Their read-only and dry-run modes do
not cross the gate. Read-only inspection, filters that only transform standard
input, `install --write`, `git-install-alias`, dismissed-prompt management, and
factory `reset` are not root-sensitive merely because they mutate some other
user or Dodot-owned location. Post-`up` repository installers inherit the
already-authorized root from the protected `up` flow.

Two entries in that taxonomy do not derive their write targets from the
resolved root today. `refresh` and `transform check` walk the shared
preprocessor baseline cache and write each baseline's stored absolute
`source_path`, which can name a source file under a different root. Because
per-root cache and datastore namespaces are out of scope, the gate's
target-is-the-selected-root invariant is met by scoping the mutation set rather
than the cache: these commands write only baselines whose canonical
`source_path` lies inside the authorized canonical root, and report every other
baseline as out-of-root instead of writing it. Baselines whose source path is
stale, absent, or uncanonicalizable are reported, never written. Trusting one
root therefore cannot authorize writes into another root's source tree, and the
root the user approved is the root actually mutated. This is distinct from
shared datastore state, where cross-root pack-identity collisions remain an
acknowledged out-of-scope condition; the concern here is writes into a second
repository's user-authored source.

Only `y` or `yes` approves the root. `n`, an empty answer, unrecognized input,
EOF, and a non-TTY implicit-root attempt all refuse the command, emit a clear
diagnostic on stderr, leave stdout empty, make no trust or requested-command
writes, and exit with status 1. Interactivity requires a terminal on both sides
of the exchange: the answer is read from a terminal-backed stdin and the
warning is written to a terminal-backed stderr. A terminal stdin whose stderr
is redirected is non-interactive and refuses, because approval must never be
accepted for a root and inventory the user was never shown. Ctrl-C retains the conventional status 130 and
also leaves the root unapproved and the command unexecuted.

The prompt is conservative: it identifies the canonical root, explains whether
Git or the current directory selected it, and defaults to refusal. It also
shows root-wide handler counts and at most ten file paths across the entire
prompt, prioritizing shell and code-execution handlers; omitted paths are
counted. This is a recognition aid, not a dry run: Dodot does not render,
resolve, or inspect candidate pack-entry contents for the prompt. It does read
configuration and routing metadata needed to classify entries. Approval is
recorded before the requested mutation starts because it records the user's
path decision, not the repository contents or the mutation's outcome.

Each recognized file contributes to one configured-handler category. Detailed
paths are ordered first by category priority—shell, code execution, PATH,
external, link, then other—and then by pack-relative path. The category counts
remain visible even when the ten-path detail cap omits entries.

Configuration is validated before approval. A bad configuration names the
offending file and problem, records no approval, and prevents the requested
command from running. A present `DOTFILES_ROOT` is authoritative: empty,
nonexistent, non-directory, unreadable, or uncanonicalizable values hard-fail
without Git/cwd fallback. Relative and non-Unicode operating-system paths are
accepted when they resolve to a readable directory and are then canonicalized.
A non-Unicode explicit value keeps its native form through resolution, and
diagnostics that name it use the same reversible spelling as trust state.

## User / Agent Stories

1. As a user who runs `dodot up` in an unrelated repository, I want Dodot to
   stop before changing anything, so that an ordinary navigation mistake cannot
   alter my next shell or deployed configuration.
2. As a user working inside a subdirectory of my dotfiles repository, I want to
   see that Git selected the repository top-level, so that I approve the root
   Dodot will actually use rather than the directory shown in my prompt.
3. As a user approving a root for the first time, I want concise handler counts
   and a bounded file list that prioritizes shell startup and code execution,
   so that I can recognize an obviously wrong directory without the
   confirmation question scrolling out of view.
4. As a user who declines or presses Enter at the prompt, I want the command to
   report cancellation and exit unsuccessfully without writing trust or
   deployment state, so that scripts cannot mistake refusal for deployment.
5. As a user with more than one dotfiles repository, I want to trust each root
   independently, so that Safety Lock does not impose an unrelated preference
   or arbitration model.
6. As a user who moves a dotfiles root, I want Dodot to reassess the resolved
   path, so that approval for the old location is not silently applied to the
   new one.
7. As a user who no longer trusts a root, I want to inspect and revoke its
   approval, so that future root-sensitive mutations require confirmation
   again. `dodot roots list` shows approvals and `dodot roots forget <path>`
   removes one.
8. As an automation author, I want a valid `DOTFILES_ROOT` to remain the clear
   noninteractive selection mechanism, so that intentional jobs stay
   deterministic without teaching a generic confirmation bypass.
9. As an automation author with a broken `DOTFILES_ROOT`, I want a hard error
   naming the invalid path, so that Dodot never mutates a fallback repository.
10. As a user or agent investigating an unfamiliar repository, I want `status`,
    `list`, probes, and dry runs to remain safe and available before trust, so
    that I can understand the repository without authorizing it.
11. As a user invoking a root-sensitive command with JSON or YAML output, I
    want stdout to remain valid structured data, so that the safety interaction
    does not corrupt downstream consumers.
12. As a user about to run `down` from the wrong repository, I want the same
    root protection as `up`, so that legitimate datastore state is not removed
    as apparent orphaned state.
13. As a maintainer adding a new mutating command, I want root-sensitivity to be
    explicit and testable at one seam, so that new commands do not silently
    escape the safety policy.
14. As a user whose dotfiles configuration is invalid, I want Dodot to identify
    the bad file and problem and refuse to approve or run the root-sensitive
    command, because Dodot must not operate from configuration it cannot load.

## Risks And Rabbit Holes

- A prompt that dumps every matched file becomes unreadable. The orientation
  inventory reports counts by configured handler and limits detail to ten paths
  across the whole prompt. Its deterministic sample prioritizes shell and
  code-execution handlers and states how many paths were omitted. It does not
  claim to be an operation plan or content analysis.
- A filtered command and a root-wide approval have different scopes. The prompt
  identifies that the path itself is being approved, not only the filtered
  packs in the current command.
- Root selection, trust lookup, confirmation, and execution happen over time.
  One resolved-root value is shared across the invocation, preventing later
  environment, Git, cwd, or symlink resolution from silently selecting another
  root. Dodot does not lock or snapshot repository contents: same-path changes
  remain covered by canonical-path trust, while a root that disappears or
  becomes unreadable causes normal command validation to fail before mutation.
- The command taxonomy can drift as new commands appear. Naming a context
  builder "read-only" is not sufficient evidence because several current
  commands modify pack source while using read-only execution context flags.
- Existing deployments from multiple roots share datastore pack identities.
  Root trust must not imply deployment isolation or claim to resolve collisions
  that already exist.
- Persisting approval only after a supposedly successful `up` is unreliable:
  current command results can describe conflicts or per-operation failures
  without a single unambiguous success signal. Trust and deployment outcome
  must remain separate concepts.
- A repository-local marker would dirty the user's source tree, could travel in
  Git, and would violate the existing storage contract. Safety Lock state must
  not become user-authored dotfile content by accident.
- Existing E2E setup exports `DOTFILES_ROOT`, so the deliberate bypass would
  make the new behavior invisible unless tests explicitly exercise implicit
  discovery.
- Prompt tests that pipe `yes` are not TTY tests. The suite needs a real
  pseudo-terminal lane for affirmative interaction while separately proving
  that piped stdin cannot bypass the guard.

## Cross-Cutting Concerns

**Safety boundary.** Safety Lock confirms path intent before an implicitly
selected root can drive mutation. It does not evaluate repository contents and
never broadens the meaning of existing flags such as `--force`.

**State ownership and migration.** Approval is per user and per canonical root,
stored within Dodot-owned state rather than the source repository. Existing
users have no approval records and will be asked once on their next implicit,
root-sensitive mutation. Explicit-root automation remains compatible. Approval
has no time-to-live or version expiry. Explicit revocation and `dodot reset`
remove it, after which the next implicit root-sensitive mutation asks again.

**CLI output and exit behavior.** Prompts and refusal diagnostics use stderr;
structured results use stdout. Interactive refusal and noninteractive refusal
exit 1 with empty stdout. `n`, Enter, invalid input, EOF, and non-TTY execution
all count as refusal. Ctrl-C exits 130. Every such path leaves trust unchanged
and does not run the requested command.

**Observability.** Debug logs should record root provenance, trust decisions,
and gate outcomes without recording sensitive contents. User-facing errors
must name the selected root and explain the safe next action.

**Performance.** Already-trusted roots should pay only a small trust-check cost.
First approval performs only the discovery and classification needed for the
bounded orientation inventory; it does not render or inspect content.

**Compatibility.** Scripts that rely on implicit Git/cwd discovery for mutation
without a terminal will begin failing by design. Documentation must make a
valid `DOTFILES_ROOT` the supported deliberate automation path. Read-only and
dry-run workflows remain compatible.

**Release and documentation.** Help, getting-started material, the dotfiles-root
glossary, command references, troubleshooting, reset behavior, and agent-facing
guidance must describe the trust model consistently. They must distinguish the
pre-mutation Safety Lock prompt from the post-`up` install ladder, and
distinguish `roots forget`, dismissed-prompt reset, and factory reset from one
another.

## Testing / Verification

The root selection, root trust, and CLI safety gate modules are all tested
through their interfaces.

Root-selection tests cover valid explicit selection, invalid explicit values,
Git-subdirectory discovery and provenance, current-directory fallback,
canonical identity, and path aliases. Explicit-value cases include empty,
relative, non-Unicode, nonexistent, non-directory, unreadable, and
uncanonicalizable paths, and assert that no invalid present value falls back to
Git or cwd.

Root-trust tests cover absent, valid, corrupt, incompatible, and revoked state;
multiple roots; moves to a new canonical path; replacement at an already
trusted path; safe persistence; concurrent attempts; unwritable storage; and
cleanup semantics. Tests assert that same-path replacement retains trust,
existing path aliases can be forgotten through canonical matching, stale paths
can be forgotten without existing on disk, and failure to record approval
occurs before the protected mutation. Corrupt, unreadable, and incompatible
state fails closed without appearing empty; inspection reports it, narrow
revocation recovers an identifiable record, and reset recovers the remaining
cases. Path-identity tests cover non-Unicode roots end to end: two roots whose
lossy UTF-8 renderings collide persist, list, and revoke as two distinct
records; a stored record survives a persistence and reload cycle byte for byte;
and a deleted non-Unicode root remains revocable by passing back exactly what
`roots list` printed.

Prompt-inventory tests exercise default and custom handler mappings, handler
precedence, pack and entry ignores, host gates, template source names, nested
files, PATH entries, provisioning handlers, externals, links, and filtered
commands. They assert deterministic handler counts and a maximum of ten paths
across the prompt, with shell and code-execution priority and explicit
omitted-path counts. They also prove that a first-ever run needs no rendered
baseline and that prompt construction reads only configuration and routing
metadata, never rendering, decrypting, resolving, or inspecting candidate
pack-entry contents.

CLI-gate tests use injected input and output where appropriate to cover
affirmative answers, default and explicit refusal, invalid input, EOF,
noninteractive execution, exit status 1 for every refusal form, exit status 130
for Ctrl-C, empty stdout on refusal, stderr diagnostics, command classification,
prior trust, explicit-root bypass, dry-run bypass, invalid configuration before
prompt, trust persistence, or requested mutation. Cache-derived mutation is
covered by two roots sharing one data and cache directory with only one root
trusted: `refresh` and `transform check` write only the trusted root's sources
and report the other root's baselines as out-of-root, so trust in one root
never permits a write under the other.
The management surface is covered through `dodot roots list` and `dodot roots
forget <path>`; tests assert that no separate trust verb exists.

Real-process verification includes:

- a dedicated Bats safety suite with `DOTFILES_ROOT` explicitly unset;
- non-TTY refusal with no deployment, repository, user-configuration, or trust
  changes; ordinary diagnostic logging remains permitted;
- proof that piped affirmative input cannot bypass TTY requirements;
- refusal when stdin is a terminal but stderr is redirected, proving approval
  is impossible on a channel that never displayed the root;
- invalid explicit-root failure without Git/cwd fallback;
- a real pseudo-terminal acceptance path for approve, refuse, Enter, interrupt,
  and second-run behavior;
- a first-run root orientation prompt containing representative shell, PATH,
  provisioning, external, and link handler assignments without preprocessing;
- an untrusted wrong-root `down` that leaves existing deployment state intact;
- dry runs that neither prompt nor establish trust;
- JSON and YAML output that remain parseable; and
- terminal-debug inspection of semantic styling and output-channel placement.

The required verification lanes are `pixi run lint`, `pixi run test`, the
focused Safety Lock Bats/PTY suites, and the repository's full real-process E2E
suite. Existing tests that export `DOTFILES_ROOT` must continue proving the
deliberate-selection path rather than being mass-rewritten to bypass prompts in
a less explicit way.

Acceptance evidence must be collected in an isolated home, data directory,
cache, and repository. No test may exercise the developer's live Dodot
deployment.

## Workstream Hints

- Establish canonical root selection and trusted-root state behind small,
  independently testable interfaces.
- Add the central CLI gate, bounded handler inventory, and root-sensitive
  command classification while preserving read-only, dry-run, passthrough, and
  structured-output behavior.
- Complete real-process coverage and reconcile all user, contributor, and
  agent-facing documentation.

These hints identify coherent areas of work only. The later issue-planning leg
owns the independently mergeable workstream topology and dependencies.

## Out Of Scope

- Implementation code, tracker issues, release execution, or migration tooling
  in this planning artifact.
- A general policy framework for arbitrary dangerous commands unrelated to
  dotfiles-root selection.
- Content linting for shell programs, install scripts, Brewfiles, Nix
  manifests, or fetched resources.
- Per-root datastore namespaces, cross-root locking, or pack ownership leases.
- Automatic recovery of overwritten links, removed datastore state, or broken
  shell startup from historical accidental deployments.
- Remote trust synchronization or repository-carried approval.

## Further Notes

Durable decisions:

- [Trust dotfiles roots by canonical path](../adr/0001-trust-dotfiles-roots-by-canonical-path.md)
- [Guard root-derived mutations, not every mutation](../adr/0002-guard-root-derived-mutations.md)
- [Inspect and revoke roots without a trust command](../adr/0003-inspect-and-revoke-roots-without-a-trust-command.md)
