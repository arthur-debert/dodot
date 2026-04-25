Design Specification: Pack Ordering

    This document specifies how dodot orders the application of packs across a dotfiles repository. The current behaviour — strict lexicographic order by directory name — is a de-facto contract that has never been written down, and users have no idiomatic way to express "this pack must run before that one" when bootstrap dependencies require it.

    The proposal is small on purpose. It documents the existing contract, adds a recognised numeric prefix grammar for pack directories, and explicitly defers richer ordering surfaces (priority fields, dependency graphs, phase buckets) until real usage shows the prefix is insufficient. The handlers spec [./preprocessing-pipeline.lex] already covers within-pack handler phases; this document covers the gap above that — order across packs.


1. Motivation

    1.1. Bootstrap Is the Common Case

        Three real situations have surfaced from early adoption, all variations of the same shape:

            - The Homebrew shell environment must be evaluated before any pack that uses brew. Today this lives in raw shell rc above the dodot eval, because dodot has no way to express "this pack first".
            - `compinit` must run after all completion-providing plugins are loaded but before `fzf-tab`. The cross-pack ordering between a `completion` pack and an `fzf-tab` pack is undefined unless the user knows the lex-order trick.
            - On a fresh macOS install, baseline setup (xcode-select, license acceptance, etc.) is a precondition for anything that compiles. Today users either run `dodot up <bootstrap-pack>` manually first or push the work outside dodot entirely.

        These are not requests for a dependency graph. They are requests for an ordering primitive. The distinction matters; see §6.

    1.2. The Undocumented Contract

        dodot already orders packs lexicographically by directory name [../crates/dodot-lib/src/packs/mod.rs]. Shell init files are emitted across packs in that same order [../crates/dodot-lib/src/shell/mod.rs]. Pack name validation accepts digits, hyphens, underscores, and dots, so users can already prefix a directory with `001-` and observe the ordering effect. What is missing is:

            - Documentation that this contract exists and can be relied on.
            - A recognised, opinionated grammar so the prefix is treated as ordering metadata rather than as part of the pack's logical name.
            - A getting-started note explaining what belongs in raw shell rc (the bootstrap-before-init zone) versus what belongs in a pack.

        The first item is free. The second is small. The third is documentation only. None of this requires a new ordering engine.


2. Principles

    2.1. The Filesystem Should Tell the Truth

        A user listing their dotfiles directory should be able to see the order packs will apply. This is dodot's house style: explicit over magical, visible over configured. A `priority = 30` field buried in a `.dodot.toml` is invisible to anyone scanning the directory; a directory named `030-nvim` is not. The cost is that the order is encoded in the name, which couples identity to position. We accept that cost.

    2.2. One Mechanism, Not Three

        Numeric prefixes, an `_` bootstrap convention, a `priority` field, and a `requires:` dependency list are all candidates. Shipping any two of them in the same release creates a conflict-of-rules problem (which wins when they disagree?) and forces every user to learn both. This proposal ships exactly one mechanism — the numeric prefix — and reserves the others as future work, gated on real demand. See §6.

    2.3. Ordering Is Not Dependency

        systemd's split between `Before=`/`After=` (ordering) and `Requires=`/`Wants=` (dependency strength) is the right primitive, and conflating them is the documented failure mode of every system that has tried. dodot's prefix is purely an ordering primitive: it says "A applies before B", not "A is required for B to make sense". A pack with a missing dependency is the user's problem, not the framework's. We do not detect, validate, or react to it.


3. The Current State

    3.1. What dodot Does Today

        - `scan_packs()` reads the dotfiles directory, builds a `Pack` per subdirectory, and sorts by directory name [../crates/dodot-lib/src/packs/mod.rs].
        - The shell handler iterates packs in scan order and emits one source line per shell file per pack [../crates/dodot-lib/src/shell/mod.rs]. Across packs, the lex order is the source order.
        - `dodot up <name>` resolves the argument by exact directory-name match [../crates/dodot-lib/src/packs/orchestration.rs]. There is no fuzzy match, no alias system, no prefix stripping.
        - Pack name validation [../crates/dodot-lib/src/packs/mod.rs] accepts `[A-Za-z0-9._-]+`, so `001-brew`, `010_nvim`, and `99.late` are all currently legal pack names.

    3.2. What Breaks Without This Proposal

        Nothing breaks. A user can rename their packs to `001-brew`, `010-nvim`, `100-starship` today and ordering will work as expected. The cost they pay is that:

            - Every CLI invocation uses the prefixed name: `dodot up 010-nvim`, not `dodot up nvim`.
            - Every status output, error message, and log line shows the prefix.
            - The convention is theirs, not dodot's; another user reading their repo has to infer it.

        This proposal pays those costs once, in the framework, so users don't pay them on every interaction.


4. Survey of Prior Art

    The decision space has been explored extensively by adjacent tools. We summarise rather than enumerate; the full survey informs the trade-offs but the conclusion is what matters.

    4.1. Dotfiles Managers

        chezmoi:
            Filename grammar: `run_once_before_NN-name.sh`. Phase encoded in the name (`before`, `after`, `once`); within a phase, lexical order. The closest analog to what we're proposing. Bootstrap is solved cleanly. Cost: filename grammar is dense and easy to typo, and the `once` hash key is the filename, so renaming breaks idempotency.

        dotbot:
            YAML list order is the order. No deps, no priorities. Refactoring means rewriting the giant ordered list. Works, but the ordering surface is one global file rather than per-pack metadata.

        yadm:
            One user-authored bootstrap script. Punts the problem to the user.

        rcm:
            `pre-up.d/` and `post-up.d/` directories with run-parts-style lex order. Two phases plus lex within. Coarse but adequate for the bootstrap case.

        home-manager (Nix):
            Full DAG. Solves everything. Requires buying into Nix.

        Ansible roles:
            `meta/main.yml` `dependencies:`. Named, explicit, transitively resolved. The gold standard for declared deps, also the most expensive in machinery.

    4.2. Adjacent Ordering Systems

        SysV `S01foo`, `S99bar`:
            The original NN-prefix pattern. Works in practice; the documented pain is renumbering cascades and the implicit-meaning problem (`S30` doesn't say *why* it's before `S40`). The 10/20/30 gap convention exists precisely to mitigate the renumbering pain.

        run-parts (`/etc/cron.d`, `/etc/logrotate.d`):
            Lexical order with conventional prefixes `00-`, `50-`, `99-`. Pitfall: filenames with dots are silently skipped by run-parts itself. Universal gap convention.

        systemd:
            `Before=` / `After=` for ordering, `Requires=` / `Wants=` for dependency. Decoupling the two is the lesson worth stealing.

        zsh plugin managers (zinit, antigen):
            `.zshrc` line order. `compinit` → `fzf-tab` is the canonical "ordering matters" example in the entire shell ecosystem.

        Vim plugin/after, Emacs use-package :after:
            Phase buckets and named "load after" deps respectively. The vim two-phase model (plugin/, after/plugin/) is a reminder that two buckets cover surprisingly many cases.

    4.3. What the Survey Says

        Three families:

            - Numeric prefix + lex sort (SysV, run-parts, chezmoi-in-practice). Zero learning, visible, painful to renumber.
            - Named deps + topo-sort (Ansible, use-package, Nix). Refactor-safe, requires config, edge cases in the graph.
            - Phase buckets (vim plugin/after, chezmoi before/after, rcm pre/post). Coarse, adequate for ~90% of bootstrap cases, almost no machinery.

        The chezmoi precedent is the strongest argument for prefix-based ordering specifically in dotfiles tooling: the model is proven on the same problem we're solving. The systemd precedent is the strongest argument for *not* conflating ordering with dependency.


5. Proposal

    5.1. The Contract (Document What Already Exists)

        Add to `docs/reference/handlers.lex`, a new "Cross-pack ordering" section with this contract:

            dodot processes packs in lexicographic order of their on-disk directory names. Within a pack, handlers run in their fixed phase order. Across packs, this lex order determines the relative order of every cross-pack effect — shell init source order, PATH entry order, install/provision execution order.

        This sentence resolves the original feedback at zero implementation cost. Ship it before any code change.

    5.2. The Prefix Grammar

        A pack directory matching the regex `^(\d+)[-_](.+)$` has its prefix recognised as ordering metadata:

            On disk:
                The directory is named with the prefix: `010-nvim`, `020_zsh`, `100-starship`.

            Logical pack name:
                The portion after the separator: `nvim`, `zsh`, `starship`. This is what every user-facing surface uses — `dodot up <name>`, status output, error messages, generated shell-init comments, log lines.

            Sort key:
                The full on-disk directory name. Lexicographic sort over the full name produces the intended numeric order *as long as users follow the digit-width convention* (see §5.4).

            Unprefixed packs:
                Sort lexically among themselves and interleave naturally with prefixed ones. `010-brew` < `020-zsh` < `nvim` < `starship`, so prefixed packs run before unprefixed ones in this example.

    5.3. Collision Rules

        Three classes of collision are possible. All are scan-time errors with both offending paths reported:

            Logical-name collision:
                Both `nvim` and `010-nvim` exist. The logical name `nvim` is ambiguous. Hard error.

            Multi-prefix collision:
                Both `010-nvim` and `020-nvim` exist. The logical name `nvim` resolves to two packs. Hard error.

            Empty stem:
                `010-` or `010_` (no name after the separator). Hard error: a pack must have a name.

        The non-collision case — two packs with the same prefix and different stems (`010-brew` and `010-zsh`) — is permitted; lex order on the stem decides between them. We do not enforce the gap convention; it is documented but not validated.

    5.4. Recommended Convention

        Document, do not enforce:

            - Three-digit prefixes with leading zeros: `010`, `020`, `050`, `100`. Two digits work but break sort once you cross 99 to 100.
            - Gap convention: 10/20/30/.../100, leaving room to insert without renumbering. This is the universal lesson from SysV and run-parts.
            - Reserved range by convention: `000–099` for bootstrap-y things (PATH setup, package manager install, OS-level prereqs). `100–899` for normal packs. `900–999` for late-loading things (prompt, autosuggestions, anything that wants to see the fully-populated environment).
            - Hyphen separator preferred over underscore. Both work; hyphen reads better and matches existing dodot conventions.

        These are conventions for the docs and the canonical example, not validation rules. A user who wants `5-foo` and `99-bar` is not wrong; they have just bought themselves a renumbering hazard, and that is their business.


6. What This Proposal Does Not Build

    Each of these is a reasonable extension. None ships in this proposal.

    6.1. No `priority` Field in `.dodot.toml`

        A `priority = 30` field per pack would decouple order from directory name and make refactoring easier (move a pack between order positions without renaming). The cost is that order becomes invisible: you can no longer answer "what runs first?" by listing the directory. Per principle 2.1, the filesystem should tell the truth. Defer until a concrete case shows the prefix can't handle it.

    6.2. No Dependency Graph (`requires:`, `after:`)

        An explicit `after: [pack-name]` field, with topo-sort and cycle detection, is the Ansible/use-package model. It is refactor-safe and expresses intent precisely. It is also a multi-week project (graph construction, cycle detection, error UX, doc churn) for a problem that the prefix solves for the 90% case. If real usage produces a case where two packs need to be relatively ordered without caring about their global position, revisit. The prefix and an `after:` field compose without contradiction; we are not painting ourselves into a corner.

    6.3. No Phase Buckets (`bootstrap/`, `main/`, `late/`)

        Either as parent directories or as a frontmatter field. The prefix already provides finer-grained control than three buckets, and the convention in §5.4 (000–099 / 100–899 / 900–999) gives the same coarse-grained intuition without a second mechanism. Per principle 2.2.

    6.4. No `_` Prefix as a Bootstrap Signal

        "Anything prefixed with `_` runs first" was considered. It is a one-bit phase bucket layered on top of the lex-order rule, which means two ordering rules that interact. Two rules > one rule. The `0NN-` prefix range covers the same intent.

    6.5. No Validation of Bootstrap Hygiene

        dodot does not detect "you put a brew-using pack before the brew-installing pack". That is a runtime failure of the user's pack scripts, surfaced through normal exit codes. Validating it would require dodot to understand what each pack actually does, which is out of scope by a wide margin.


7. Implementation Sketch

    Approximate scope: one struct field, one regex, one collision check, one display swap, plus tests and docs.

    7.1. Pack Struct Changes

        The `Pack` struct gains a `display_name: String` alongside the existing `name: String`. The `name` field continues to hold the raw on-disk directory name and continues to be the sort key; `display_name` holds the stripped form.

        For unprefixed packs, `display_name == name`. For prefixed packs, `display_name` is the stem.

    7.2. Scan-Time Changes

        `scan_packs()` [../crates/dodot-lib/src/packs/mod.rs] gains:

            - Regex match for the prefix grammar; populate `display_name`.
            - Collision detection (§5.3) implemented as a pass over the scanned packs after sort: hash by `display_name`, error on duplicates with both source paths in the message.

        Sort behaviour is unchanged: still by `name`, still lex.

    7.3. Lookup Changes

        `dodot up <arg>` [../crates/dodot-lib/src/packs/orchestration.rs] resolves `<arg>` against `display_name` first. Exact match on `display_name` wins. As a fallback, exact match on `name` (raw) is also accepted, so users with muscle memory or scripts referencing the prefixed form are not broken.

        If `<arg>` matches multiple packs (only possible if the user disabled collision detection, which they cannot — see §5.3), the same scan-time error fires.

    7.4. Display Surfaces

        Every user-facing surface switches to `display_name`:

            - `dodot status` table rows
            - `dodot list` output
            - Error messages ("pack `nvim` not found")
            - Generated shell-init comments (`# pack: nvim`)
            - Log lines

        Internal surfaces (datastore paths, sentinel keys, anything keyed by directory identity) continue to use `name`. The directory on disk is still `010-nvim`; the datastore subtree is still `010-nvim/`. Only display changes.

    7.5. Tests

        - Round-trip: scan a fixture with mixed prefixed/unprefixed packs, assert sort order and `display_name` values.
        - Collision detection: each of the three collision classes produces the expected error.
        - Lookup: `dodot up <stripped>` and `dodot up <raw>` both find the right pack.
        - Display: fixture with `010-nvim` produces output containing `nvim`, not `010-nvim`, in the rendered template.


8. Documentation Deliverables

    The proposal's user value is roughly half code and half docs. The doc work is:

    8.1. Cross-Pack Ordering Section in handlers.lex

        Per §5.1. One paragraph stating the lex-order contract, with a concrete example.

    8.2. Bootstrap Zone Note in getting-started.lex

        One sentence: anything that must exist before dodot can run (Homebrew shellenv that puts dodot on PATH, OS-level prereqs that block any pack from succeeding) belongs in raw shell rc above the dodot eval line, not in a pack. Resolves the third user feedback item.

    8.3. Pack Ordering How-To

        New short doc under `docs/reference/` (or wherever how-tos live) covering:

            - The prefix grammar
            - The 10/20/30 gap convention and why it matters
            - The 000–099 / 100–899 / 900–999 reserved-range convention
            - The canonical example: `010-brew`, `020-zsh`, `030-compinit`, `040-fzf-tab`, `100-starship`, `900-zsh-autosuggestions`
            - A note that prefixes are optional — unprefixed packs work fine and most users will never need them

    8.4. Migration Note

        Existing users with prefixed packs (today, manually, paying the CLI ergonomics cost) should be told their pack names are about to change in CLI output. This is a behaviour change, not a breaking one (the raw-name fallback in §7.3 keeps scripts working), but worth a CHANGELOG line.


9. Phased Implementation

    Phase 1: Documentation. **(Independent of all code.)**
        Land §8.1 and §8.2 immediately. Resolves the original feedback's items 2 and 3 with zero code risk. Optional: §8.3 with the canonical example, even before the prefix grammar is recognised by the code — the example works today via plain lex order.

    Phase 2: Prefix grammar.
        Implement §7.1 through §7.4. Tests per §7.5. CHANGELOG note per §8.4. Full §8.3 documenting the recognised grammar.

    Phase 3: Deferred.
        Revisit only on concrete user demand. Candidates, in rough order of likelihood:

            - `priority` field as an alternative to prefix (if users complain about renames).
            - `after:` field for relative ordering without global position (if compinit/fzf-tab style cases multiply).
            - Validation hooks that warn when a known-bootstrap pack is positioned after a known-consumer pack (low priority; almost certainly never).


10. What This Costs the User

    Being honest about the price tag:

        - One new convention to learn (the prefix grammar). Optional — unprefixed packs continue to work.
        - A small renumbering hazard if the gap convention is ignored. The convention is documented; the user owns the consequences.
        - One behaviour change in CLI output: a pack named `010-nvim` on disk now displays as `nvim`. Pre-existing scripts referencing the raw name still work via the fallback in §7.3.

    Being honest about what this does not cost:

        - No change to existing packs that don't use prefixes.
        - No new config file, no new schema, no new CLI flags.
        - No background processing, no cache invalidation, no migration script.
        - No dependency resolution, no graph, no cycle detection, no failure modes that didn't already exist.

    The proposal is small because the problem is small. dodot already orders packs; we are giving the existing order a name and a recognised grammar, and writing down what was previously folklore. That is the shape this feature should have.
