Proposal: The Magical Git Experience with Pre Processed Files

    :: note ::
        *Status: implemented and shipped.* Phases 1, 3, and 4 of this proposal landed in PRs #100 (R1, baseline cache + tracker swap), #101 (R2, conflict markers + safety gate), #102 (R3, `dodot transform check`), #103 (R4, install-hook), #104 (R5, `dodot refresh`), #105 (R6, template clean filter + filter installer + hook upgrade), #106 (R7, `transform status` + `git-show-alias` + `git-install-alias`), and #107 (R8, full-stack e2e). Phase 2 (plist clean/smudge) shipped earlier on its own track per [./plists.lex]. The user-facing reference for the resulting feature lives in [./../reference/template-magic.lex] and [./../reference/pre-processors.lex] §6. This proposal is preserved as historical design context — *not* a maintained spec. Where this document and the reference docs disagree about behavior, the reference docs are authoritative; where this document and the source disagree, the source is authoritative. See "Implementation Notes vs. Spec" at the bottom for the deviations that were accepted during implementation.

This file is not a feature proposal per se, but the user-facing conceptual document that captures what we see as the ideal design for future dodot — one that lets us uphold our goals and principles while imposing minimal cost on users to make it all work.
See @../proposals/template-expansion.lex and @../proposals/preprocessing-pipeline.lex for context.

This is a revised version of an earlier draft. Two things changed our thinking. First, we pulled encrypted files out of this story entirely — users already grok that you can't round-trip plaintext through a ciphertext, and the reference spec already says dodot does not own encryption. Second, we realized that the baseline cache the preprocessing pipeline already plans to keep (for divergence detection) turns out to be the structural trick that makes everything else cheap, honest, and compatible with the secrets design. Both changes are walked through in the sections below.


1. Source Control and dodot: git always wins

    Dotfile managers are *primarily* about collecting files from many locations and centralizing them for proper source control. That is the primary point, and everything else flows from it: reproducibility, recovery, history tracking. These are what source control gives you — not what the dotfile manager gives you.
    So it's a sensible way to define such a program as software that facilitates getting your configuration under source control.

2. The Overbearing Solutions

    The more advanced dotfile managers, in a combination of complicated designs and an ambition to handle things themselves, often overstep and take over parts of source control. Sometimes it's just a specific use case; sometimes they want you to execute git calls through them, or through a special sub-shell. The point is that in some scenario the answer to "how do I do task X (a source-control task)" changes from your vanilla git answer.
    This is a core principle of dodot: thou shalt not fuck around with source control. git does its job very well, and for whatever shortcomings one may see, there is an ecosystem of tools to augment it. Dotfile management is something you want for the long haul — probably the kind of thing you'll carry with you for decades. Coupling your data, workflows, and tooling to one particular solution is bound to cause you more work. Sure, git is itself such a coupling, but it's reasonable to expect git's shelf life to be much longer than any given dotfile manager's, and any newcomer is likely to be git-compatible anyway. That's why in dodot, nothing in source control is done outside git, ever. `git diff` tells you what has changed, as do history, checkout, and everything else.
    But what about the things that aren't really in git, or aren't entirely?

3. When Source Files and Deploy Files Differ

    In some situations, the thing you keep under source control is not the thing that gets deployed. In these cases you may still keep the source snugly under git, but `git diff` tells you what has changed in the *source*, not in the *deployed* file (nor easily could it).
    There are a handful of common use cases: template-expanded files; cases like our plist solution, where you transform the plain-text version of the format into the binary one for deployment, keeping both source control and the application happy; and secrets injected at render time (to avoid keeping sensitive data in source control).
    The question is: if the source file is fine in git and you've made a change to the deployed file, how does that change propagate back to the git source?
    Many tools solve this by forcing a workflow change on how you edit your configs, usually with further restrictions on which tool you use, and/or requiring an "apply" step — a change in your workflow, really. These were strict no-gos for dodot: no apply step, no workflow change, no forcing of config-editing tools. That works as long as you can keep source → deployed one-to-one, because then the deployed file can be a symlink, which has the nice properties we want. But for transformed files that breaks down. You can't have your cake and eat it too: if the two don't match, something has to mediate changes — hence the workflow imposition, the apply step, the tooling requirements.
    One scoping clarification before we go further: this document is specifically about files where a *useful* reverse path exists. That covers representational transforms (plist XML ↔ binary) and generative transforms (templates with variables and injected secrets). It does *not* cover encrypted files. The ciphertext you store in git and the plaintext you deploy are deliberately unrelated from git's point of view, and the whole point of encryption is that the write-back direction should be an explicit, authenticated operation — not a git hook firing silently. For those, the existing story stands: edit the plaintext manually, re-encrypt, commit the new ciphertext. That's what the reference spec already says, and we're not walking it back.

4. dodot: have your cake and eat it

    We wanted to support the two remaining transformation use cases but not give up on our principles: no workflow changes, no tool requirements, git is the source of truth.
    Other dotfile managers are correctly framing the situation: you can't have all of these at once. What dodot does isn't about breaking logical facts — it's about making a few trade-offs that, we hope, feel like the magical keepable-and-eatable cake. The answer does require a bit of reality-bending, but it works and is true *in spirit* to both our principles and the user's needs. The solution takes some clever bits and a willingness to add a little tooling to git.
    The answer is git itself. By using clean and smudge filters, plus some clever diffing, we can make this work.

    4.1. Representational Transformations

        For perfectly revertible transforms, clean and smudge filters are all you need. They can deterministically describe their files, and hence produce correct `git diff` and `git status` output with no ambiguity.
        Plists are the canonical example: XML and binary round-trip losslessly via the `plist` Rust crate (with recursive dict-key sort applied for byte-stable output). Git stores the canonical XML (diffable); the working tree holds the binary (what the application actually reads). The clean filter converts binary → XML on `git add`; the smudge filter converts XML → binary on `git checkout`. The user never sees the transform happen; `git diff` shows a sensible XML diff of what is otherwise a binary file. The full design lives in [./plists.lex].
        This case is clean, low-risk, and high-value. It's also logically independent of the rest of this proposal, so we plan to ship it on its own track.

    4.2. Generative Transformations

        For non-revertible transformations, such as template expansions — which include secret injection — there is no deterministically correct way to reverse them, because more than one input change can produce the same expanded output. However, when we think about what dodot actually does and what the user actually needs, it's possible to lean on simple heuristics that let users have the benefit without effort. And when a genuinely ambiguous situation arises, we ask the user to confirm the right call.
        This stems from an observation: if, at generation time, you can tell which parts of the rendered output came from expansion versus static template content, then you can ignore the "expansion" parts when diffing, and still produce a meaningful diff of the interesting bits that should go back into source control.
        For lines that don't touch injected content, we can very reliably detect changes and state the reverse diff assertively. For lines that *do* touch injected content, we can't. In those cases, we pre-fill the file with both the original line and the changed line, and let the user decide which is right. It's a sort of merge conflict, except nothing is being merged, because the changes aren't committed yet.
        We believe that's a good compromise: it does the expected thing almost all the time — producing the magical outcome — with only a small cost at the genuinely ambiguous corners.

The Magical BurgerToCow

    The formal truth holds: the reverse-templating problem is unsolvable in a deterministic and general way. But we don't need to cover every possible form or be fully generalizable. We can leverage the information we already have — the source text and the values used in expansion — along with knowledge of the templating engine (minijinja) to produce diffs against the original template for the common cases, and be explicit about the ambiguous ones, asking the user to resolve them.
    This is done by our burgertocow crate [https://github.com/arthur-debert/burgertocow], using a shadow-map approach: every variable emission is wrapped in invisible marker bytes during rendering, and the markers let us later tell which chars in the output came from which part of the template. The crate produces either an honest template-space diff or a conflict block flagging the lines it couldn't reliably reverse.
    Its assertiveness can be greatly enhanced with a simple best practice: hoist each expansion into a dedicated variable assignment at the top of the file. In a shell file:

    Hoisted expansion in shell:

        GH_TOKEN="{{ secret('op://Personal/GitHub/token') }}"

    :: shell ::

    Or equivalently, using minijinja's `set` tag:

    Hoisted expansion in minijinja:

        {% set GH_TOKEN = secret('op://Personal/GitHub/token') %}

    :: jinja ::

    This isolates expansions into dedicated lines — which is good practice regardless, because it makes the template easier to understand and change. After doing so, very few cases remain that can't be reliably diffed.

The Cache That Makes It Cheap

    When we first sketched this architecture, we had a real worry: if a clean filter runs on every `git status`, every `git diff`, every editor gitgutter poll, every shell prompt refresh — and it has to re-render a template that contains `secret()` calls — then every one of those now prompts for Touch ID or a 1Password unlock. That is severe auth fatigue, and it contradicts exactly the passive/active split that the secrets design carefully laid out. It was close to a dealbreaker.
    The way out came from noticing we already planned to keep a baseline cache for divergence detection (see preprocessing-pipeline.lex §5.2 and §6.1). The cache holds, per deployed file: a hash of the source, a hash of the render context, a hash of the rendered output, and the rendered output itself. Add one more field — the *tracked render*, which is the rendered output with burgertocow's one-byte markers around each variable emission — and the clean filter becomes nearly free.

    Clean filter, fast path (the common case):

        1. hash the current deployed file
        2. compare to baseline.rendered_hash
        3. if equal: emit the template unchanged and exit
           — no burgertocow, no provider calls, microseconds

    :: text ::

    The vast majority of `git status` invocations fire while the deployed file is still in sync with its last `dodot up`. Nothing needs to be recomputed; the clean filter returns immediately.
    When the deployed file *has* been edited — the interesting case — we still don't need to re-render:

    Clean filter, slow path (the user edited their deployed file):

        1. load baseline.tracked_render (cached, with markers)
        2. run burgertocow with (template, cached_tracked_render, current_deployed)
        3. emit the resulting patched template

    :: text ::

    No provider calls. The tracked render was computed and cached on the last `dodot up`, when the user authenticated once for the whole run. `git status` never prompts.
    The security posture of caching the tracked render is the same as caching the rendered output, which the reference spec already does: the plaintext is already on disk, visible to the user; adding a marker-annotated twin in the same directory, under the same permissions, doesn't meaningfully expand the attack surface.
    This is the small structural insight that makes the whole vision viable. Without it, clean filters are hostile to the secrets design. With it, they slot in cleanly.

The User Experience

    We want to be honest that the magical outcome does have user-visible costs — they're small, one-time, and per machine, but they're real. We think the best way to introduce them is incrementally, so that each step ships value on its own and the user opts into more magic only if they want it.

    1. The Git Data Bit

        There are two things to wire into git: the filters themselves, and the commit-time safety net.
        Filters live in `.git/config` and aren't carried by the repository, so they have to be installed once per clone, per machine. `dodot git-install-filters` installs them; `dodot git-show-filters` prints the configuration so you can inspect or install it by hand. On the first `dodot up` of a pack that uses a preprocessor, dodot checks whether the filters are installed. If they aren't, it asks you whether to install them, print the script for you to run yourself, or skip (and nag next time). One Y/n per machine.

    2. The Update Trigger Bit

        The last hurdle is getting git to actually *use* the filters. git doesn't scan every file's content on every check — instead, it only re-reads content when the file's modification time is newer than the cached version. This is a sensible optimization that normally keeps git snappy. But in our case, it poses a problem: when you update the deployed file, the deployed file's mtime changes, but the *source* file's mtime (the template) doesn't. git looks at the source, sees an unchanged mtime, and skips the filter — so the reverse-diff never runs.
        The fix for mtime itself is trivial: we copy the deployed file's mtime onto the source, so git will re-read it on the next invocation. With the cache in place, deciding *which* sources need a touch is cheap — we hash the deployed files and compare against the baseline, and only touch sources that are genuinely out of sync. A full `dodot refresh` over a realistic dotfile repo runs in well under a second.
        The interesting question is *when* to trigger that refresh. We see this as a ladder the user can climb as far as they want. Everyone gets tier 1 for free; tier 2 is opt-in for folks who want the fullest magic; tier 3 is there if you already have a file watcher and want to plug dodot into it.

        2.1. The Commit Tier (default)

            The always-available answer is a pre-commit hook. `dodot git-install-hook` installs one; it runs `dodot refresh` just before the commit, so the commit snapshot always contains the right template-space changes. `git diff --cached` right before committing shows the right thing. `git log -p` shows the right thing. PR reviews show the right thing.
            The trade-off is that `git status` and `git diff HEAD` *before* you try to commit will show the template as unchanged, even if you have edits sitting in the deployed file. We think this is an acceptable default: the moment template-space diffs really matter is at commit time, and that's when the hook fires. Everyone gets this tier out of the box.

        2.2. The Interactive-Shell Tier (opt-in)

            For users who want `git status` and `git diff` in their interactive shell to always reflect the latest template state, we offer a shell alias or function that runs `dodot refresh` before delegating to git. Something like:

            The interactive alias:

                alias git='dodot refresh --quiet && command git'

            :: shell ::

            Because the refresh is cache-backed, the added latency is small enough not to feel intrusive. dodot will print the alias for your shell on request (`dodot git-show-alias`), or add it to your shell's rc file with your consent. This tier only affects your interactive shell — scripts, editors, and CI that shell out to `git` directly are unaffected, which is exactly what we want: non-interactive callers get predictable behavior, interactive use gets the magic.
            If this tier rocks your boat, great. If it doesn't, skip it — you still get the commit-time magic from tier 1.

        2.3. The External-Watcher Tier (out of scope, but supported)

            If you already have a file watcher set up — a direnv hook, a Nix rebuild watcher, your editor's onSave handler — you can plug `dodot refresh` into it yourself. `dodot refresh --list-paths` enumerates the source templates that need to be watched, so the integration is straightforward. We don't ship a daemon of our own: it's out of scope, more invasive than we'd like, and you probably already have opinions about what watcher you use.

    3. Conflicts, Honestly

        When burgertocow can't reliably reverse a change — typically because the edited line touches an expansion — it emits a conflict block into the reversed template. The block contains the original template line, the user's deployed-file edit, and clear markers around both.
        In the clean-filter fast/slow path these markers surface in `git diff` and `git status` exactly as you'd expect — you see them, you resolve them. The one thing we *don't* do is let them get committed silently: the pre-commit hook (tier 1 above) detects the markers and refuses the commit until they're resolved. So even if you're deep in a rebase and the clean filter is emitting conflict-marker-bearing templates into intermediate git operations, the only way they land in a real commit is if you yourself edit them out.

    4. What This Costs the User

        Being honest about the price tag, per machine:

        - one Y/n to install the clean/smudge filters and the pre-commit hook (tier 1)
        - optionally, one Y/n to install the interactive-shell alias (tier 2)
        - re-running the install on each new clone, because filters aren't stored in the repo (git doesn't carry them)

        Being honest about what this does *not* cost:

        - no new CLI commands the user has to remember in daily use
        - no workflow changes — you commit, diff, and status with vanilla git
        - no auth prompts from passive git commands, ever
        - no dodot-owned shell, editor, or daemon

        We think that's a good deal. The user's effort is bounded, the solution isn't too intrusive, and once installed, the magic works every time.

5. Implementation, Phased

    The phased path falls out naturally from the architecture. Each phase ships value on its own; later phases build on the cache that earlier phases already require.

    Phase 1: the foundation.
        Ship the preprocessing pipeline, the baseline cache (including the tracked-render field), burgertocow integration, `dodot transform check`, and the pre-commit hook (tier 1 above). At this point, users who adopt preprocessors get correct template-space diffs at commit time with no filter installed. This is the minimum viable magic.

    Phase 2: plist clean/smudge.
        Ship plist support as an independent track — clean/smudge filters for binary↔canonical-XML conversion (via the `plist` crate, with recursive key sorting), with their own install flow shared with phase 3. Doesn't depend on anything in phase 1; can ship in parallel. Full spec in [./plists.lex].

    Phase 3: the template clean filter.
        Layer the clean filter on top of the phase-1 cache. This is the thin wrapper: hash-compare, fast-path, burgertocow on the slow path. The commit-time gate from phase 1 handles conflict markers. Opt-in filter install (the Y/n prompt on first deploy).

    Phase 4: the interactive-shell alias.
        Ship `dodot git-show-alias` and the install flow for the opt-in tier-2 alias. Purely additive over phase 3.

    Re-opening phase 3 or 4 is always possible if the commit-tier magic from phase 1 turns out to be sufficient in practice. We don't think it will be — the pull of `git status` telling the truth is strong — but it's good to know the earlier phases stand on their own if the later ones never ship.

6. Implementation Notes vs. Spec

    The implementation deviates from the spec above in a few places. Listed here so future readers don't mistake the spec for the source of truth:

    6.1. Phased rollout was R0–R8, not Phases 1/3/4

        The phased path that actually shipped was finer-grained: R0 (burgertocow `from_tracked_string` constructor, in the burgertocow repo), R1 (baseline cache + Tracker swap), R2 (conflict markers + safety gate), R3 (`dodot transform check`), R4 (`dodot transform install-hook` + post-up prompt), R5 (`dodot refresh`), R6 (clean filter + filter installer + hook upgrade), R7 (`transform status` + `git-show-alias` + `git-install-alias`), R8 (full-stack e2e). The spec's Phase 1 corresponds roughly to R1–R4, Phase 3 to R5–R6, Phase 4 to R7. R8 was a pure-tests phase that didn't appear in the original spec; R9 was a docs-and-config phase (this update).

    6.2. Hook command shape

        Spec says the pre-commit hook calls `dodot refresh` (Tier 1). The shipped form is two lines: `dodot refresh --quiet || exit 1` followed by `dodot transform check --strict || exit 1`. Splitting refresh and check across two shell statements rather than a single chained command lets users diagnose failure in either step independently from the hook output. The R4 install initially shipped just the strict-check line; R6 added the refresh line and the upgrade path that rewrites stale R4-shape blocks when `install-hook` is re-run.

    6.3. Tier 2 alias supports bash and zsh, errors on others

        Spec proposes "your shell" generally. The shipped form supports bash and zsh; fish/nu/etc. get a clear error from `dodot git-show-alias --shell <unsupported>` pointing at the supported shells, and `Shell::detect()` returns `None` for unsupported `$SHELL` values rather than silently falling back to bash. Users on other shells run `dodot git-show-alias --shell bash` and adapt the snippet manually.

    6.4. `dodot git-install-alias` shipped in R7, not deferred

        Spec §"Interactive-Shell Tier" says `dodot git-show-alias` (print only). R7 shipped both `git-show-alias` and `git-install-alias` (writes to ~/.bashrc or ~/.zshrc with idempotent guard block, mirroring the hook installer's pattern). The user-confirmation step happens through the standard install Y/n/show flow on the next post-`up` prompt rather than a separate consent screen.

    6.5. Per-file `no_reverse` opt-out

        Not in the original spec. R9 added `[preprocessor.template] no_reverse = ["pattern", ...]` for templates whose content is mostly dynamic — burgertocow's heuristic produces more conflict markers than usable diffs on those, so the user opts the file out of reverse-merge while keeping divergence detection. Per-file glob patterns; pack-level `.dodot.toml` overrides root.

    6.6. Consolidated post-`up` install ladder

        The original R4 / R6 install paths shipped as three sequential post-`up` prompts (plist filter, hook, template filter), each with its own Y/n. That violated the §4 promise of "one Y/n to install the clean/smudge filters and the pre-commit hook." Issue #112 tracked the reconciliation; the shipped form is a single Y/n covering whichever rungs apply, with `show` walking each component's preview block individually and `no` dismissing every applicable component at once. Each rung still has its own catalog dismissal key (`template.install_hook`, `plist.install_filters`, `template.install_filter`) so users can resurface a single rung via `dodot prompts reset <key>` after a global "no". The umbrella prompt is `magic.install_ladder`.

    6.7. cfprefsd cache-invalidation prompt

        Not in the original spec. Issue #109 added a separate post-`up` prompt that fires (macOS only) when `dodot up` detects a plist file in any active pack with mtime newer than the previous successful `up`. Offers `killall cfprefsd` so running GUI apps re-read fresh plist values. The detection is mtime-based via a marker file (`<data_dir>/cfprefsd-needs-invalidation`) written by `up` and consumed by the prompt. Catalog key: `plist.cfprefsd_invalidate`. cfprefsd respawns immediately so there's no data-loss window.
