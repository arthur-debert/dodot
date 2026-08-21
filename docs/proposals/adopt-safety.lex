Proposal: Safe Directory Adoption and Recoverable Publication

    `dodot adopt ~/.config/zed/` fails when that directory holds a `.DS_Store`, because the child matches `[pack].ignore` and copying it into the pack would put a file there that every later dodot run omits. The refusal is right about the hazard and wrong about the remedy: the matched child is precisely the noise the user never wanted managed, and refusing costs them the other twenty files in the directory. By the time the refusal prints, adopt has already created the inferred pack directory.

    The same command writes prospective pack content directly into final pack paths and only then asks whether deploying it would collide with another pack. Cleanup after a failed check is best effort, so created directories can outlive it, and under `--force` an earlier destination can be replaced before a later check refuses the run.

    This document specifies what `dodot adopt` guarantees for directory sources and for the order in which it writes: which entries are adoptable, which are left alone and reported, which are errors, and what state the filesystem is in after each failure. It specifies behavior only; decomposing it into implementation work happens after this document lands. Issue `#372`.

    :: note ::
        *Scope.* Pack-name inference and in-pack path encoding are settled by [./shipped/macos-paths.lex] §7 and are not reopened here — this document takes the in-pack path each source resolves to as an input. Filter semantics are settled by [./../user/filters.lex]; this document applies them to adopt rather than changing them. The current user contract is [./../user/commands/adopt.lex].
    ::


1. What Is Broken

    1.1. An Ignored Child Refuses the Whole Directory

        Pack-root directory expansion ([./shipped/macos-paths.lex] §7.5) enumerates a directory's children and plans one adoption per child. Each planned child is then tested against the merged root-plus-pack `[pack].ignore` list and dodot's reserved filenames; a match aborts the entire invocation with `refusing to adopt`.

        Two things go wrong at once. The first is the verdict: `.DS_Store`, `*.swp`, and `node_modules` are in the default ignore list [./../user/filters.lex] §4 because users want dodot to leave them alone, and finding one inside a directory the user asked to adopt is the expected case, not a mistake to report back. The second is the timing: expansion enumerates children after the inferred pack directory has already been created, so a refusal leaves an empty pack behind that the user did not ask for and is not told about.

    1.2. Prospective Content Is Written to Final Paths

        Adoption today copies every source into its final in-pack path, then runs the cross-pack deployment conflict analysis with those copies in place, then replaces the originals with symlinks. Everything that can refuse the run — the conflict analysis, a permission failure on a later source, a mid-copy error — refuses after content is already sitting at final paths.

        Unwinding that is best effort by construction. Removing a copied entry does not remove the intermediate directories created to hold it, so a refused `dodot adopt ~/.config/nvim/lua/plugins/init.lua` can leave `nvim/lua/plugins/` behind. Under `--force`, an earlier destination is replaced with new content before later plans are copied or the conflict analysis runs, and the replaced content is gone: a run that ultimately refuses has still destroyed a file it never adopted.


2. Principles

    2.1. Ignoring Is a User Decision, Not an Adopt Failure

        A file matches `[pack].ignore` because someone — the default list or the user's own config — decided dodot should not read it. Adopt reads that decision as an answer, not as a question to escalate. An ignored child discovered inside a directory the user pointed at stays where it is, and the run continues.

        The one place a match still means refusal is a path the user typed. `dodot adopt ~/.config/zed/.DS_Store` names a file whose configuration says dodot ignores it; adopting it would produce a pack entry that no later dodot run reads. Answering that with a report and a success would be a lie about what the command did.

    2.2. No One-Run Bypass

        The remedy for "I actually want dodot to manage this file" is to change the configuration that ignores it — remove the pattern, or override `[pack].ignore` for the pack. Adopt adds no flag that adopts an ignored path for one run, because the resulting pack entry stays invisible to every subsequent `dodot up` and `dodot status`. A flag whose success leaves the user with a file dodot will not deploy is worse than the error it replaces.

    2.3. Nothing Final Is Written Until Everything Is Decided

        Every check that can refuse a run — classification, destination conflicts, permissions, cross-pack deployment conflicts — runs before the first byte lands at a final pack path. This makes the `--force` guarantee structural instead of a matter of ordering luck, makes `--dry-run` the same code path as a real run minus its last steps, and means a refused adopt leaves the dotfiles repo exactly as it found it, including creating no pack directory.

    2.4. Claim the Atomicity That Exists

        A single `rename` within one filesystem is atomic; a sequence of them is not. Publishing a new pack is one rename and is atomic. Updating an existing pack is several and is recoverable — adopt undoes what it did on failure — but a process killed mid-sequence leaves an intermediate state that no in-process recovery can undo. This document says which of the two applies at each step rather than describing both as atomic.


3. Classifying Entries

    3.1. One Predicate

        An entry is *adoptable* when a later pack scan would read it at the in-pack position adopt would give it. Three discovery-layer rules decide that, and only those three [./../user/filters.lex] §§2, 4:

        - `.dodotignore` in the destination pack — the pack itself is invisible, so the whole invocation is refused, as it is today.
        - `[pack].ignore` — the merged root-level and pack-level pattern list.
        - dodot's reserved filenames — `.dodot.toml` and `.dodotignore`.

        Dispatch-layer filters do not participate. A file matching `[mappings].ignore`, `[mappings].skip`, or a gate label is discovered by the scan and then routed — it is a live pack entry whose routing the user can change by editing config, without moving files. Adopt classifies on discovery, where an omission means no dodot run can see the file at all.

        The predicate applies to every path component of the prospective in-pack path, not to the leaf alone. `dodot adopt ~/.config/nvim/lua/plugins/init.lua` produces the in-pack path `nvim/lua/plugins/init.lua`; if `lua` matched an ignore pattern the scan would never descend, so the adoption is refused even though `init.lua` matches nothing. Routing prefixes (`_home/`, `_xdg/`, `_app/`, `_lib/`) and the gate directory `--only-os` prepends are components like any other and are tested the same way.

    3.2. Explicitly Named Sources

        A source path the user typed on the command line that is not adoptable is an error. The message names the matched pattern and the configuration layer it came from — the built-in default list, the root `.dodot.toml`, or the pack's own — so the user can act on it.

        Error text:

            error: ~/.config/zed/.DS_Store
              this path matches `.DS_Store` in [pack].ignore (dodot's default
              list), so no dodot run would read it inside a pack. To manage it,
              override [pack].ignore for this pack in .dodot.toml.

        :: text ::

        Nothing is written, and no pack is created.

    3.3. Discovered Children

        A child found by directory expansion that is not adoptable stays at its original path, is not copied into the pack, and is reported once (§4). The run continues with the adoptable children. `~/.config/zed/` adopts its config files; `~/.config/zed/.DS_Store` remains a real file inside a directory whose other entries are now symlinks, which is what a user who has both a `.DS_Store` and an ignore rule for it already expects.

        Reserved filenames are the exception: a discovered `.dodot.toml` or `.dodotignore` is an error, not a report. Copying either into the pack would change the pack's own configuration or hide the pack entirely, an outcome different in kind from leaving noise behind, and it is not something to do as a side effect of adopting a directory.

    3.4. Zero Adoptable Children

        An expanded directory whose children are all unadoptable is an error. Reporting every child as left in place and exiting successfully would claim an adoption that did not happen. The message lists the children and the rule each matched, so the user can see that the directory holds nothing dodot would manage.

        Error text:

            error: ~/.config/cache-only/
              expanding this directory found no adoptable entries. All 3 children
              match [pack].ignore:
                .DS_Store   matches `.DS_Store`
                index.swp   matches `*.swp`
                node_modules  matches `node_modules`

        :: text ::

        An empty directory is the same error with an empty list — there is nothing to adopt either way.

    3.5. What Adopt Does Not Classify

        Classification covers the paths the user names and the immediate children expansion discovers. It does not descend into an adopted directory. `~/.config/helix/themes/` is copied whole, ignored files inside it included, and later scans omit those files silently — the behavior a user gets today from any directory they adopt. Refusing or reporting on subtree contents would make adopting a real config directory an exercise in clearing its noise first, for a payoff the ignore rules already deliver.

    3.6. `--force` Changes None of This

        `--force` answers exactly one question — may adopt replace an existing destination inside the pack — and answers it after classification has already decided which entries exist. It does not adopt an ignored path, does not adopt a reserved filename, and does not turn the zero-adoptable-children error into a warning.


4. The Ignored-Entry Report

    Entries left in place under §3.3 are listed in the adopt result, once, as command-level notes naming each original path and the rule that matched it. The run's exit status is success; these are not failures.

    Reporting at adopt time is the only opportunity there is. A file matching `[pack].ignore` is invisible in `dodot status` by design — that is the difference between `[pack].ignore` and `[mappings].skip` [./../user/filters.lex] §2 — so no later command tells the user that `~/.config/zed/.DS_Store` is still a real file among symlinks. Adopt is the single moment where dodot both knows the fact and is entitled to say it, because the user just asked about that directory.

    The report changes nothing about later runs. Adopt persists no record of what it left behind, adds no status row, and marks nothing for follow-up. `dodot status` stays silent about ignored paths afterwards, exactly as it is for a file that was ignored all along, and the intentional silence documented for `[pack].ignore` remains intact.


5. Order of Operations

    Five steps, in this order. The first three write nothing outside the preparation directory.

    5.1. Plan

        Resolve the destination pack, infer each source's in-pack path, classify every named source and every discovered child (§3), and check destination conflicts against the pack's current content — an existing destination is an error without `--force` and a displacement with it. Check that the sources are readable and that the dotfiles root is writable. Refusing here writes nothing at all; in particular, an inferred pack that does not exist yet is still not created.

    5.2. Prepare

        Copy the prospective content into a preparation directory inside the dotfiles root, laid out at the in-pack paths the plan assigned. The directory carries a fixed `.dodot-adopt-` name prefix and a per-run suffix.

        Inside the dotfiles root is the requirement, not a convenience: publication is a `rename` from the preparation directory to the final path, and `rename` does not cross filesystems. Copying from the source into the preparation directory may cross one — the user's `$HOME` and their dotfiles repo can live on different filesystems, and that copy is the same work adopt already does today.

        The preparation directory sits in the dotfiles root, where pack discovery would otherwise read it as a pack. Discovery has to skip it — by the name prefix, or by a `.dodotignore` written inside it — so that a `dodot status` running while an adopt is in progress reports the user's packs and not a half-copied one.

    5.3. Validate

        Run the cross-pack deployment conflict analysis against the *prospective* pack tree — the pack's current entries composed with the prepared entries at their final in-pack paths — and refuse if deploying it would collide with another pack. Whether the prospective tree is assembled in memory or read from the preparation directory with paths rewritten is an implementation choice; the contract is that final pack paths are not written to for the analysis to run. `--force` does not bypass this analysis, as it does not today.

        `--dry-run` reports here and stops: the preparation directory is removed, and no final path was ever touched.

    5.4. Publish

        Publishing a new pack is one `rename` of the prepared pack directory onto the pack path, and is atomic: the pack appears complete or does not appear. If the pack path came into existence after planning, adopt refuses instead of merging into it — nothing is published yet, so refusing is free.

        Publishing into an existing pack is a sequence: create the intermediate directories the plan needs, then, per entry, displace any existing destination into the preparation directory and `rename` the prepared entry into place. Each individual entry's appearance is atomic. The sequence is not: it is *recoverable*, meaning adopt undoes its own renames on failure — displaced content renamed back, published entries removed, intermediate directories adopt created and left empty removed — and it reports what it restored. Adopt does not promise portable whole-directory atomic replacement for an existing pack, and does not promise crash-atomicity: a process killed mid-sequence leaves the pack in an intermediate state with the preparation directory still on disk. The fixed name prefix makes that leftover identifiable, and no later adopt run publishes from a leftover or deletes one.

        Displaced content is kept until §5.6, not discarded here.

    5.5. Replace Sources

        Per adopted source, replace the original with a symlink to its published pack path.

        A file source, or a symlink source under `--no-follow`, is replaced by creating the symlink at an adjacent temporary name and renaming it over the original: one atomic replacement, with the original readable at its own path until the instant it is the symlink.

        A directory source is two steps — rename the original to an adjacent backup name, create the symlink — plus removing the backup once the symlink exists. If the symlink fails, the backup is renamed back and the source is as it was. A process killed between the two steps leaves the directory at the backup path next to its original location, named clearly enough to restore by hand. This is recoverable, not atomic, and adopt says so rather than claiming otherwise.

        Sources are independent. A failure on one replaces nothing for that source, restores that entry's pack state — the displaced content under `--force`, or removal of the newly published entry otherwise — reports the failure in the result, and leaves the sources already replaced alone. Adopt does not promise one transaction across unrelated source directories, and cannot: they may live on different filesystems from each other and from the pack.

    5.6. Finish

        With every source replaced or reported, remove the preparation directory, discarding the content displaced in §5.4. Until this step, every `--force` displacement is recoverable; after it, the user's `--force` has taken effect as asked.


6. Failure Guarantees

    What the filesystem holds after a failure at each step:
        | Failure point | Pack | Sources |
        | Classification (§5.1) | untouched; inferred pack not created | untouched |
        | Destination conflict, permissions (§5.1) | untouched; inferred pack not created | untouched |
        | Copy into preparation (§5.2) | untouched; preparation directory removed | untouched |
        | Deployment conflict analysis (§5.3) | untouched; preparation directory removed | untouched |
        | `--dry-run` stop (§5.3) | untouched; preparation directory removed | untouched |
        | New-pack publication (§5.4) | pack does not exist | untouched |
        | Existing-pack publication (§5.4) | pre-adopt content restored; report names what was restored | untouched |
        | One source replacement (§5.5) | that entry's pre-adopt content restored | that source untouched; earlier sources adopted |
        | Process killed during §5.4 | intermediate; preparation directory left with displaced content | untouched |
        | Process killed during §5.5 | published | at most one source at an adjacent backup path |

        :: table align=lll ::

    Two entries in that table are the defects §1.2 describes, and both are closed by §2.3: a refused run never leaves a partially created pack, and `--force` never destroys a destination for a run that then refuses.


7. Non-Goals

    - *No Git integration.* Adopt does not read `.gitignore`, does not write one, and does not treat Git's view of a file as a reason to adopt or refuse it. `[pack].ignore` is dodot's list and the only one consulted.
    - *No continue-on-ignores flag.* §2.2. An explicitly named ignored path is an error with a configuration remedy, not a prompt with an override.
    - *No cross-filesystem all-or-nothing transaction.* §5.5. Sources are independent, and adopt says which unit is atomic instead of implying the invocation is.
    - *No crash-atomic existing-pack update.* §5.4. Recovery is in-process; a killed process leaves identifiable leftovers rather than a rolled-back pack.
    - *No subtree classification.* §3.5. Ignored files inside an adopted directory are copied with it.
    - *No handler execution and no datastore write.* Adopt continues to move files and create symlinks only; the next `dodot up` deploys what was adopted, unchanged from the current contract.
    - *No persisted record of ignored entries.* §4. The report exists for one run.


8. Verification Scenarios

    Enough cases to decompose this document into implementation work and to know when each piece is done.

    Classification:
        - Expansion with one ignored child: `~/.config/zed/` with a `.DS_Store` adopts every other child, leaves `.DS_Store` a real file at its original path, and reports it once with the matched pattern.
        - Expansion with every child ignored: errors, lists each child and its rule, creates no pack, writes nothing.
        - Expansion of an empty directory: same error, empty list.
        - Explicitly named ignored path: errors, names the pattern and the configuration layer it came from (default list, root `.dodot.toml`, pack `.dodot.toml`).
        - Ignored component above the leaf: adopting `nvim/lua/plugins/init.lua` while `lua` matches a pattern errors, naming the component.
        - Pack-level `[pack].ignore` override: a pattern removed in the pack's `.dodot.toml` makes the matching child adoptable; a pattern added there makes it left-in-place.
        - Reserved filename, named explicitly and discovered by expansion: error in both cases.
        - Dispatch-layer filters do not refuse: a child matching `[mappings].skip` or `[mappings].ignore`, or carrying a gate label, is adopted normally.
        - `--force` with an ignored child, a reserved filename, and a zero-adoptable directory: identical outcomes to the runs without it.

    Report:
        - The left-in-place list appears in the adopt result with a success exit status.
        - `dodot status` after that run says nothing about the left-in-place paths.
        - No file under the dotfiles root or the data directory records them.

    Ordering and publication:
        - A run refused by the deployment conflict analysis leaves no pack directory when the pack was inferred and absent, and leaves an existing pack byte-identical.
        - A run refused by any §5.1 check leaves no preparation directory and no pack directory.
        - `--dry-run` writes nothing at any final path, removes its preparation directory, and reports the same plan a real run then executes.
        - New-pack publication is a single rename: an injected failure immediately before it leaves no pack; immediately after it leaves the pack complete.
        - Existing-pack publication with an injected failure at entry N of M restores the pre-adopt content of entries 1..N and leaves the sources untouched.
        - `--force` with an injected failure after the first destination is displaced and before the last is published leaves every displaced destination holding its original content.
        - The preparation directory lives inside the dotfiles root, and publication performs no cross-filesystem copy.
        - A pack scan running against a dotfiles root that contains a leftover preparation directory does not read it as a pack, and a subsequent adopt neither publishes from it nor deletes it.

    Source replacement:
        - A file source is replaced atomically: the original path is a readable file or the finished symlink at every observable moment.
        - A directory source whose symlink creation fails is restored to a real directory at its original path with its content intact.
        - With several sources, an injected failure on the second leaves the first adopted, the second untouched, the second's pack entry restored to its pre-adopt state, and the failure reported.
        - Sources on different filesystems from the dotfiles root replace correctly and independently.

    Unchanged behavior:
        - `--into` naming a missing pack still errors; an inferred missing pack is still created, now at publication.
        - A destination pack marked with `.dodotignore` still errors.
        - Already-adopted sources are still detected and skipped with the existing messages.
        - Adopt still runs no handler and writes no datastore entry.


9. See Also

    - [./shipped/macos-paths.lex] §7 — source roots, in-pack path encoding, pack-root directory expansion, auto-created packs.
    - [./../user/filters.lex] §§2, 4, 5 — the discovery and dispatch layers, and why `[pack].ignore` is silent in status.
    - [./../user/commands/adopt.lex] — the current user contract this document revises.
