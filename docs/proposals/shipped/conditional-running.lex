Design Specification: Conditional Running

    :: note ::
        *Status: implemented and shipped.* Phases C1–C5 landed in PR #135 (with follow-up doc PR #136 merged into it). The user-facing reference lives in [./../../user/conditional-running.lex] (full guide), [./../../user/configuration.lex] §2.1 / §4.1 / §5 (config schema), and [./../../user/handlers/controlling-activation.lex] §1.3 (the `gate` filter handler). This proposal is preserved as historical design context — *not* a maintained spec. Where this document and the user/reference docs disagree about behavior, those docs are authoritative; where this document and the source disagree, the source is authoritative. See "Implementation Notes vs. Spec" at the bottom for the deviations that were accepted during implementation.

    This document specifies how dodot routes pack files based on runtime properties of the host — primarily operating system, with the architecture left open for arch and user-defined predicates. The current state has one mechanism for this case (templates with `{% if dodot.os == "macos" %}`), and that mechanism is the wrong shape for the file-level and pack-level needs that prompted this proposal.

    The proposal is small on purpose. It introduces a single grammar — `_<label>` as a filename infix or directory segment — that gates whether dodot deploys/runs an entry, plus a `[pack] os` config key for whole-pack gating, plus a small extension to the existing Filter-phase handler family. The grammar is generic over a label table; OS labels (`darwin`, `linux`, etc.) ship as built-ins, and the architecture accommodates user-defined labels as a future extension without changing the parser.

    :: note :: See [./../../reference/terms-and-concepts.lex] for terminology used throughout.


1. Motivation

    1.1. The Real Cases

        Three concrete situations have surfaced that share the same shape:

            - A `Brewfile` should only run on macOS. Today, the user either accepts that `brew bundle` fails on Linux (Homebrew exists on Linux too, but the brewfile is mac-specific), or wraps the entry in a templated pack with `{% if dodot.os == "macos" %}` and renders an empty file on Linux — which still gets deployed as an empty file.

            - An `install.sh` script that does Linux-specific package work via `apt-get` should not execute on macOS. The same template trick produces a script that runs `# nothing on darwin`, which is at best a no-op and at worst confusing.

            - A whole pack of macOS-specific GUI app configs (`_app/Code/...`, `_lib/LaunchAgents/...`) is dead weight on Linux. The user wants the pack to *not be considered* on the wrong OS, not deployed as a pile of skipped entries.

        These are not requests for a predicate language. They are requests for a gating primitive that sits at the right granularity — file, subtree, or pack — without forcing every gated entry to become a template.

    1.2. Why Templates Are the Wrong Tool For This

        dodot already supports templates with full access to `dodot.os`, `dodot.arch`, `dodot.hostname`. For *content* that varies between hosts, templates are correct: line-level branching, value substitution, computed paths.

        Templates are the wrong tool when the question is "should this file exist on this host at all?" Reasons:

            - Templates always produce an output. An empty render still gets deployed; a Linux-rendered Brewfile.tmpl produces an empty Brewfile that `brew bundle` happily runs on. The semantics are "transform content," not "decide existence."
            - Templates require turning a regular file into a `.tmpl` file. That changes everything downstream — the file enters the preprocessing pipeline, gets a baseline cache entry, may participate in reverse-merge for `git status` purposes, and incurs the divergence-tracking machinery. For a 5-line shell script the user wants to run only on macOS, that's an enormous tax.
            - Templates put the OS test inside the file content, where it's invisible from `ls`. The filesystem stops telling the truth about what dodot will do.

        A separate gating mechanism keeps templates focused on what they're for (content) and gives gating its own surface (existence/dispatch).

    1.3. What's Already Partly There

        Two existing mechanisms do part of the job:

            - `.dodotignore` skips a whole pack — but unconditionally. There's no way to say "skip on Linux only."
            - `[mappings] ignore` and `[mappings] skip` drop matched files inside a pack — also unconditional.

        Both are pre-handler filters. Conditional gating is the same primitive plus a predicate. The architecture chapter (§5) shows how the new mechanism extends rather than parallels these.


2. Principles

    2.1. The Filesystem Should Tell the Truth

        A user listing their pack should see which files are darwin-only and which are linux-only without grepping for `{% if %}` blocks. This is the same principle that underwrites pack-ordering's numeric prefix and routing's `_home/` / `_app/` directories. A `_darwin` token in a filename is visible from `ls`; a templated `{% if %}` inside a `.sh.tmpl` is not.

        Cost: gating is encoded in the name, which couples identity to predicate. We accept that cost for the same reason pack-ordering accepted it for ordering.

    2.2. One Grammar, Generic Over Labels

        We resist the temptation to ship "OS gating" as a one-off. The grammar is `_<label>`, where `<label>` resolves through a hardcoded match table to a set of `(dimension, value)` equality checks, AND-ed together. OS labels (`darwin`, `linux`, `macos`) are the seed table; arch follows trivially; user-defined labels (`laptop`, `arm-mac`) are a small additive extension.

        This is *not* a predicate language. There are no operators, no expressions, no negation, no precedence. A label is an opaque token that resolves to "all these dimensions equal these values, simultaneously." The user-facing pitch is "label your file with the host trait it needs"; the implementation is a hashmap lookup.

    2.3. No Predicate Composition in the Filename

        Filename gates do not AND-stack. `install._darwin._arm64.sh` is *not* the path to "darwin AND arm64"; AND-stacking lives inside the label table:

            [gates]
            arm-mac = { os = "darwin", arch = "aarch64" }

        Then the file is `install._arm-mac.sh`. One name → one label → one resolved predicate. The grammar stays a single slot; combinatorial cases route through user-defined labels.

        Implementation note: the parser scans for the rightmost `._<token>` boundary in the basename. If a filename happens to contain multiple `._<token>` patterns (`install._bar._baz.sh`), only the rightmost (`_baz`) is treated as a gate; everything to its left is part of the stem. This is rightmost-wins, not "hard error on more than one." That keeps filenames like `foo._bar._darwin.sh` workable when `_bar` is just a name component the user wants in the file's stem.

        This is exactly the pack-ordering posture: "one mechanism, not three" — and combinatorics live in user-controlled config rather than in filename parsers.

    2.4. Gating Is Existence, Not Content

        Templates handle content (`{% if %}`); gates handle existence (drop or pass). The two are orthogonal and compose: `aliases._darwin.sh.tmpl` is "a darwin-only file that is also a template." The gate fires first (drop on Linux); the template renders on the survivors.

    2.5. Reuse Existing Machinery

        Gates are not a new pipeline phase. They live in the scanner's lexical-normalization step — the same step that already strips ordering prefixes from pack names (`010-nvim` → `nvim`). Failed-gate entries are tagged for the existing Filter-phase handler family; passing-gate entries surface under their stripped names.

        Concretely: a `GateHandler` sits in the registry beside `IgnoreHandler` and `SkipHandler`, with the same shape (claim-and-drop, no intent emitted). Its rules are dynamically generated from the resolved gate table at scan time. The handler/category/phase model stays uniform; the gate decision happens at scan, the same way `SkipHandler`'s decision is positional priority.


3. The Current State

    3.1. What Dodot Does Today

        - The scanner walks each pack directory and presents file/dir entries to the rule matcher under their literal names. The only normalization at scan is `display_name_for(pack)` for pack directory names (strips numeric ordering prefix). File names are passed through unchanged.

        - Filter-phase handlers (`ignore`, `skip`) drop matched files via positional priority (100 / 50, above precise mappings at 10). They claim matches and emit no intents.

        - The template preprocessor renders `*.tmpl` files into `<datastore>/packs/<pack>/preprocessed/<stripped-name>` and feeds the rendered output into normal handler dispatch.

        - `dodot.os`, `dodot.arch`, `dodot.hostname`, etc. are exposed to templates via the `dodot.*` namespace; they are evaluated live at render time.

    3.2. What Breaks Without This Proposal

        Nothing breaks. A user can rename `Brewfile` to `Brewfile.tmpl` with `{% if dodot.os != "macos" %}{# empty #}{% else %}brew "ripgrep"{% endif %}` and get the right behavior on darwin-only via the Brewfile reference matching. Pack-level gating can be approximated by `.dodotignore` plus a manual `git checkout` ritual.

        The cost is that:

            - Every gate is a template, with the full preprocessing-pipeline tax.
            - Empty files still get deployed; not every handler treats "empty" as "absent."
            - Pack-level gating is unavailable without manual git rituals.
            - The filesystem stops answering "what does dodot do here?" without reading file contents.

        This proposal pays those costs once, in the framework, with a small, additive grammar.


4. Survey of Prior Art

    The decision space has been explored extensively. The full survey informs the trade-offs but the conclusion is what matters.

    4.1. Dotfiles Managers

        chezmoi:
            Filename suffixes (`run_once_`, `_darwin`, `executable_`) plus Go-template content gating; an empty render results in a skipped target. The closest analog to this proposal in spirit. Cost: filename grammar is dense and easy to typo, and the OS-suffix and template-empty mechanisms overlap awkwardly.

        yadm:
            `name##os.Linux,arch.x86_64` filename suffix system with multi-condition AND, scoring, and negation. The most expressive filename-grammar in any tool surveyed; also the ugliest. The `##` delimiter is hostile to editors, tab-completion, and visual scanning.

        dotbot:
            YAML `if:` clauses with arbitrary shell expressions per link directive. Maximum power, zero structure; every condition is a quoting exercise.

        rcm:
            `host-<hostname>/` and `tag-<name>/` directory prefixes. Filesystem-visible, simple. Activation is per-invocation (`rcup -t work`). No OS/arch primitives — those have to be encoded as tags.

        toml-bombadil:
            TOML profiles with stacking. `bombadil link -p sway,work` activates multiple. Cleaner than dotdrop's include graph; no built-in OS detection.

        nix-home-manager:
            Programmatic. `pkgs.stdenv.isDarwin`, `lib.mkIf`, anywhere. Maximum expressiveness; requires writing Nix.

        mackup, dotdrop, GNU Stow, bare git+scripts:
            See research notes — none contributes a primitive this proposal builds on.

    4.2. Adjacent Tools

        Ansible:
            `host_vars/` and `group_vars/` directory conventions plus `when:` Jinja2 expressions on tasks. Two complementary mechanisms — directory-membership for variables, predicate clause for execution.

        CMake:
            `if(APPLE)`, `if(UNIX)`, with rich predicates and `option()` for user tags. `cmake -DENABLE_FOO=ON` is the canonical user-tag mechanism.

    4.3. Synthesis

        Three patterns dominate, at three granularities:

            L1 (whole pack/profile):    rcm tag-dirs, bombadil profiles, ansible groups
            L2 (whole file/dir):        yadm `##cond.value`, chezmoi empty-render
            L3 (lines within a file):   templates everywhere

        dodot already has L3. dodot's philosophy (§7 in [./../../reference/philosophy.lex]) explicitly rules out L1's "profile" framing. The unaddressed gap is L2: file/subtree/pack-level gating.

        The cleanest L2 mechanisms in the survey share a property: a recognised filename or directory grammar with a config counterpart. That is exactly the pattern dodot already uses for routing prefixes (`home.X` ↔ `force_home`, `_app/` ↔ `app_aliases`). This proposal mirrors that pattern.


5. Proposal

    5.1. The Filename Grammar

        Per-file gates use a `._<label>` infix immediately before the extension, or as the final segment for extensionless files:

        File-level examples:
            | Source                       | Gate  | Stripped name | Behaviour                          |
            | `install._darwin.sh`         | darwin | `install.sh`  | runs install only on macOS         |
            | `install._linux.sh`          | linux  | `install.sh`  | runs install only on Linux         |
            | `Brewfile._darwin`           | darwin | `Brewfile`    | brew bundle only on macOS          |
            | `aliases._linux.sh`          | linux  | `aliases.sh`  | sourced into shell only on Linux   |
            | `home.bashrc._darwin`        | darwin | `home.bashrc` | composes with routing prefix       |
            | `gitconfig.tmpl._darwin`     | darwin | `gitconfig.tmpl` | composes with template extension |
        :: table align=lllll ::

        Per-subtree gates use a `_<label>` directory segment whose contents are gated as a unit. The segment itself is removed from the deploy path:

        Directory-level examples:
            | Source                                  | Gate  | Deploys to (on match)        | Behaviour              |
            | `mac-tools/_darwin/_home/.hammerspoon/` | darwin | `~/.hammerspoon/`            | only on macOS          |
            | `linux-only/_linux/_xdg/i3/config`      | linux  | `~/.config/i3/config`        | only on Linux          |
            | `cross/_darwin/_app/Code/User/x.json`   | darwin | `<app_support>/Code/...`     | only on macOS          |
        :: table align=llll ::

        Gate dirs live at the pack root; routing-prefix subtrees (`_home/`, `_xdg/`, `_app/`, `_lib/`) live *inside* a passing-gate dir, not the other way around. The scanner expands gate dirs at scan time, so the contents surface at pack-root level on a matching host. Nesting in the opposite order (routing prefix at top, gate inside) is not supported in this iteration — the symlink handler owns recursion inside routing-prefix dirs and is intentionally gate-unaware. Putting the gate at the outer level reads more naturally too ("the darwin slice of my pack contains a `_home/` subtree").

        A standalone `_<label>/` directory at the pack root gates everything beneath it without changing the deploy root:

            cross-platform-pack/
                _darwin/
                    macos-only.sh
                _linux/
                    linux-only.sh
                shared.sh

        :: text ::

        The grammar is positional and unambiguous because filename slots don't overlap:
            - Routing prefix lives at the start of the basename (`home.X`) or as a top-level directory (`_home/`).
            - Gate token lives at the end of the basename, just before the extension (`._<label>.ext`), or as a directory segment (`_<label>/`).
            - The two slots never collide; their order in the filename is fixed.

    5.2. Label Resolution: Built-in and User-Defined

        A label is an opaque token. dodot resolves it through a `[gates]` table that maps each label to a set of `(dimension, expected_value)` pairs, AND-ed together. Dimensions are exactly the existing `dodot.*` namespace.

        Built-in labels (compiled defaults; user does not write these):

        Built-in labels:
            | Label    | Resolves to                          | Notes                  |
            | darwin   | `{ os = "darwin" }`                  | macOS                  |
            | linux    | `{ os = "linux" }`                   | Linux                  |
            | macos    | `{ os = "darwin" }`                  | alias for `darwin`     |
            | arm64    | `{ arch = "aarch64" }`               | Apple Silicon, ARM     |
            | aarch64  | `{ arch = "aarch64" }`               | alias for `arm64`      |
            | x86_64   | `{ arch = "x86_64" }`                | Intel/AMD 64-bit       |
        :: table align=lll ::

        User-defined labels in `.dodot.toml`:

        User-defined labels:

            [gates]
            laptop  = { hostname = "mbp-arthur" }
            work    = { hostname = "work-laptop" }
            arm-mac = { os = "darwin", arch = "aarch64" }   # implicit AND
            mbp     = { os = "darwin", arch = "aarch64" }   # alias for arm-mac

        :: toml ::

        Resolution rules:

            - Equality only. No operators, no negation, no expressions.
            - Multiple `(dim, val)` pairs in a label entry are implicitly AND-ed.
            - User-defined labels deep-merge with built-ins per the standard config layering. User config can shadow a built-in (e.g., redefine `darwin` to also require a hostname); the built-in's mapping is replaced wholesale by the user entry. Doing this is unusual; documented but not promoted.
            - Unknown label tokens are a hard error at scan time. Typo guard. Fail message names the file and the unknown label.

        Dimensions accepted in the value side: `os`, `arch`, `hostname`, `username`. Mirrors the `dodot.*` template namespace. Unknown dimensions are a hard error at config-load time.

        Promotion editorial: v1 docs lead with the OS use case, mention arch, and footnote user-defined labels. The machinery is generic; the documentation is curated.

    5.3. Pack-Level Gating: `[pack] os`

        For whole-pack gating, a separate config key — concrete, not generalized:

        Pack-level gating:

            # cross/.dodot.toml
            [pack]
            os = ["darwin"]              # whole pack runs only on macOS
            # os = ["darwin", "linux"]   # explicit allowlist; default is "all"

        :: toml ::

        Semantics:

            - List of OS values. Empty list or absence = "all OSes" (today's behaviour).
            - Match is OR within the list (`["darwin", "linux"]` runs on either).
            - Values are compared against `dodot.os` exactly; the same alias map as labels applies (`macos` → `darwin`).
            - When the current OS isn't in the list, the pack short-circuits at scan time. No file is matched, no handler runs.
            - The pack surfaces in `dodot status` as `inactive (os=darwin, current=linux)` rather than disappearing silently. Users debugging "why didn't my mac pack deploy on linux" get an answer.

        Why a concrete `os` key and not a generalized `gates`:

            - The whole-pack case is dominated by OS gating. Hostname-level pack gating is rare; arch-level pack gating is rarer.
            - `[pack] os = ["darwin"]` is more discoverable than `[pack] gates = ["darwin"]` for a feature most users want at the pack level.
            - The filename grammar handles the generic case. Pack-level can stay narrow.

        Future work may add `[pack] gates = [...]` if the demand surfaces; the two coexist if/when that happens.

    5.4. Composition with Existing Mechanisms

        Routing prefixes (per [./../../reference/symlink-paths.lex]):

            home.bashrc._darwin              →  ~/.bashrc on darwin only
            _darwin/_home/.bashrc            →  ~/.bashrc on darwin only (subtree form, gate outside)
            _home/.bashrc._darwin            →  ~/.bashrc on darwin only (per-file inside subtree)
            _darwin/_app/Code/User/x.json    →  <app_support>/Code/User/x.json on darwin only
            home.bashrc._darwin + [symlink.targets] entry → routing-override conflict (per §6.6 of symlink-paths)

        :: text ::

        Templates:

            aliases._darwin.sh.tmpl
              gate fires first (drop on Linux)
              if pass: template renders aliases.sh.tmpl into aliases.sh
              shell handler picks up aliases.sh

        :: text ::

        Pack ordering:

            010-mac-tools/.dodot.toml: [pack] os = ["darwin"]
              entire pack inactive on Linux; ordering prefix is preserved on darwin

            010-shared-tools/_darwin/foo.sh
              shared pack; foo.sh gates on darwin only

        :: text ::

        Mappings:

            install._darwin.sh
              filename gate strips to `install.sh`
              `[mappings] install = ["install.sh", ...]` matches the stripped name
              install handler runs the original (gated) source path

        :: text ::

    5.5. Resolution: When Multiple Gates Apply

        A single file can be gated by both filename grammar AND `[mappings.os]` (a future addition; see §6). When that happens: hard error, refuse to deploy, point at both, ask user to pick one. Same posture as `[symlink.targets]` vs routing prefix conflict.

        A file under both a directory gate and a filename gate: AND. `_darwin/.bashrc._linux` is impossible (the file would have to be on darwin AND linux). Surfaces as the file never matching its own gate; deployed nowhere; visible in status as "gated out (label conflict)."

        A pack with `[pack] os` set AND filename gates inside: the pack-level gate is checked first. If the pack is inactive on this OS, no file inside is even scanned. If the pack is active, file gates apply normally.


6. Configuration Surface

    6.1. New Schema

        Schema additions to `.dodot.toml`:

            # Pack-level gating
            [pack]
            os = ["darwin"]              # list of OS values; empty/absent = all

            # Label table (extends built-ins)
            [gates]
            laptop  = { hostname = "mbp-arthur" }
            arm-mac = { os = "darwin", arch = "aarch64" }

            # Glob-based filename gating without renaming
            [mappings.gates]
            "install-mac.sh" = "darwin"
            "Brewfile"       = "darwin"

        :: toml ::

        `[mappings.gates]` patterns match the same top-level pack entries the scanner surfaces — the symlink handler's nested per-file recursion is gate-unaware, so globs containing path separators only match if a top-level entry has that shape. This matches the posture from §8.8 (gates don't reach inside routing-prefix subtrees). For per-file gating of nested files, use the filename grammar (`install._darwin.sh`) instead. Invalid glob patterns are a hard error at scan time, and matches are deterministic (lexicographic pattern order, first-match-wins).

    6.2. Inheritance

        Standard three-layer model: compiled defaults < root `.dodot.toml` < pack `.dodot.toml`. `[gates]` deep-merges across layers (per the maps-deep-merge rule); a pack can introduce a label without restating root labels. `[pack] os` is pack-scoped only — it has no meaning at root level (a root config setting `[pack] os` would gate every pack, which is silly; root-level `[pack] os` is a config error and rejected).

    6.3. Defaults

        Defaults table:
            | Setting         | Default                                                    |
            | `[gates]`       | built-in seed (darwin, linux, macos, arm64, x86_64, …)     |
            | `[pack] os`     | `[]` (all)                                                 |
        :: table align=ll ::


7. Architecture

    7.1. Where Gates Live in the Pipeline

        Gates are evaluated at scan time, in the same lexical-normalization pass that strips ordering prefixes from pack names. The scanner already does this for pack-display-name resolution (`display_name_for("010-nvim") → "nvim"`); gate-stripping is the file/dir-level analog.

        Pipeline stages (highest to lowest in execution):

            1. Pack discovery: walk dotfiles root, find pack directories, apply `.dodotignore` filter.
            2. Pack gating: read each pack's `.dodot.toml`, evaluate `[pack] os` against current host. Drop inactive packs early.
            3. File scan + lexical normalization: walk each active pack's tree.
                 a. Strip routing prefixes (`_home/`, `_xdg/`, ...) — this is configured by symlink rules, not done by the scanner; documented here for context.
                 b. Strip ordering prefix from pack names (existing behaviour).
                 c. NEW: parse gate tokens (`._<label>.`, `_<label>/`). Look up label in resolved gate table.
                 d. NEW: drop entries whose gate evaluates false. Surface them in status as "gated out."
                 e. NEW: pass surviving entries to rule matching with stripped names.
            4. Rule matching: existing behaviour, on the cleaned names.
            5. Preprocessing (templates etc.): existing behaviour, on the gate-survivors.
            6. Handler dispatch and intent generation: existing behaviour.

        The gate decision happens in stage 3c–3e. The matcher and handlers see a "virtual" pack tree with gate suffixes stripped and failed entries removed.

    7.2. The `GateHandler`

        A new filter handler is registered alongside `IgnoreHandler` and `SkipHandler`. Its purpose is twofold:

            - Provide a registered handler-name that the scanner tags failed-gate entries with, so they surface in `dodot status` under a meaningful label.
            - Carry the per-gate metadata needed for the status renderer (which label, which dimension, what the host has, what the gate expected).

        Like `IgnoreHandler`/`SkipHandler`, it is `Filter` phase, `Configuration` category, `Precise` match, `Exclusive` scope. Its `to_intents` returns `Ok(vec![])` — gate handling produces no executable intent. Its rules are *dynamically generated* from the resolved gate table at scan time.

        Status surface:

            $ dodot status
              cross-platform/
                ...
                aliases.sh                    (sourced)
                aliases._linux.sh             gated out (label=linux, current os=darwin)
                Brewfile                      (provisioned)
                install._linux.sh             gated out (label=linux, current os=darwin)

        :: text ::

        The "gated out" rendering is distinct from `skip`'s rendering — gates are not "we chose to skip"; they're "the host doesn't match." The label and the host fact are both shown.

        Pack-level inactive:

            $ dodot status
              ...
              010-mac-tools                   inactive (os=darwin, current=linux)
              011-shared-tools                ...
              ...

        :: text ::

    7.3. Resolver Pseudocode

        Scanner's gate evaluation, called once per file/dir entry:

        Pseudocode:

            fn evaluate_gates(rel_path: &Path, gates: &GateTable, host: &HostFacts)
                -> GateResult {
                let segments = rel_path.components().collect::<Vec<_>>();

                // Check directory segments for `_<label>` form
                let mut active_dir_gates = vec![];
                for seg in &segments[..segments.len()-1] {
                    if let Some(label) = strip_underscore_label(seg) {
                        active_dir_gates.push(label);
                    }
                }

                // Check basename for `._<label>.ext` form
                let basename = segments.last()?.as_str();
                let file_gate = parse_basename_gate(basename); // returns Option<label>

                // All gates must pass (AND)
                let all_labels: Vec<&str> = active_dir_gates.iter()
                    .chain(file_gate.iter())
                    .collect();

                for label in &all_labels {
                    let pred = gates.lookup(label).ok_or_else(|| {
                        Error::UnknownGateLabel { label, file: rel_path }
                    })?;
                    if !pred.matches(host) {
                        return GateResult::Failed { label, expected: pred, actual: host };
                    }
                }

                let stripped_path = strip_gate_tokens(rel_path);
                GateResult::Passed { stripped_path }
            }

        :: rust ::

        `parse_basename_gate` recognises `<stem>._<label>.<ext>` where `<label>` is a single segment after `_` containing only `[A-Za-z0-9_-]`. Extensionless variants (`Brewfile._darwin`) treat the trailing `._<label>` as the gate.

        `strip_underscore_label` matches a directory segment of the form `_<label>` (entire segment starts with `_`). Note: this is the same prefix as `_home/`, `_xdg/`, etc. (routing prefixes). Disambiguation: routing prefixes are configured tokens (a small fixed set: `home`, `xdg`, `app`, `lib`); a `_<token>` directory segment is a routing prefix if the token is in that set, otherwise it's a gate label. Unknown tokens after `_` in a directory segment are gate labels and resolve through the gate table; if the table doesn't know the label either, hard error.

    7.4. Composition with Preprocessing

        Templates are still preprocessors. The gate strips its suffix at scan time, *before* preprocessing matches `.tmpl`. So:

            aliases._darwin.sh.tmpl  →  scan strips ._darwin  →  aliases.sh.tmpl  →  template preprocesses  →  aliases.sh  →  shell handler

        :: text ::

        On Linux, the gate fails first and the file never reaches the preprocessor. No render, no cache entry, no divergence tracking.

        Plists, secrets, and any future preprocessor compose the same way: gate first (existence), preprocessor second (content). No preprocessor needs to know about gates.


8. What This Proposal Does NOT Build

    Listed explicitly so future readers don't mistake omission for oversight.

    8.1. No Predicate DSL

        No `[pack] when = "os == 'darwin' && hostname == 'work'"`. No filename `name#os==darwin.sh`. No regex matching. The grammar is "label → AND of equalities," and the user's expressivity flows through user-defined labels.

    8.2. No Stacking Filename Gates

        `install._darwin._arm64.sh` is not parsed as "darwin AND arm64." Compound predicates use a user-defined label (`arm-mac`).

        The parser uses rightmost-wins semantics — given multiple `._<token>` segments, only the rightmost is treated as a gate, the rest stay in the stem (see §2.3). It does not hard-error on multiple segments because that would forbid filenames like `foo._bar._darwin.sh` where `_bar` is a legitimate name component (e.g. a tool's pack-internal naming convention).

        Reason: AND-stacking of dimensions is the same expression already supported by user-defined labels. One way to do it.

    8.3. No Negation

        No `_!darwin` or `_not-darwin`. If a user wants "everywhere except darwin," they list the positive labels (`._linux.sh`) for each target OS, or rely on the absence of a gate (no suffix means "all hosts"). Negation is an open door we don't need to walk through.

    8.4. No Profiles

        Per [./../../reference/philosophy.lex] §7. dodot is single-config-per-machine. No `dodot up --profile work`. Hostname-based gates and user-defined labels cover the practical "work vs home" cases without introducing a profile concept.

    8.5. No Hostname-Suffix Surface in v1 Docs

        The dimension table accepts `hostname`, but v1 docs and the seed `[gates]` table do not include hostname-derived built-in labels. Reason: hostnames are user-specific; baking any into built-ins is presumptuous. Users define their own (`laptop = { hostname = "mbp-arthur" }`) and use the resulting label.

        The implementation supports `hostname` in user labels on day one. We just don't lead with it.

    8.6. No Matcher Awareness Beyond Filter

        Gates do not modify rule priorities, do not change handler dispatch order, and do not introduce new handler categories. The filter-handler precedent is fully sufficient.

    8.7. No `dodot adopt` Round-Trip Magic

        Adopting `~/.bashrc` from a darwin host produces `home.bashrc` (no gate). Adding a gate is a manual rename. Reason: adopt's job is "infer where this file lives"; "should this file exist only on this OS" is a design intent the user has to declare. A `--only-os <os>` flag is a minor future polish (deferred to phase C5).

    8.8. No Gate Segments Inside Routing-Prefix Subtrees

        `_home/_darwin/.bashrc` is *not* supported. The scanner expands gate dirs only at pack-root level; recursion inside `_home/`, `_xdg/`, `_app/`, and `_lib/` is owned by the symlink handler and is intentionally gate-unaware. Users who want OS-specific routing put the gate at the top level: `_darwin/_home/.bashrc`. This composes correctly because gate dirs strip on a matching host, leaving `_home/.bashrc` for the symlink resolver to route.

        Extending the symlink handler's per-file recursion to evaluate nested gate segments is straightforward but adds dispatch logic to a code path the trait is otherwise minimal in. We defer until a real use case shows the outer-gate form is insufficient.


9. Implementation Sketch

    The implementation is structurally additive: every change either introduces a new scanner stage, a new handler, or a new config key. No existing behaviour changes.

    9.1. Module Layout

        New code:

            crates/dodot-lib/src/gates/
                mod.rs            # GateTable type, label resolution
                builtins.rs       # built-in seed (darwin, linux, arm64, ...)
                parse.rs          # filename + dirname gate token parsing

            crates/dodot-lib/src/handlers/
                gate.rs           # GateHandler (filter-phase, registered)

        :: text ::

        Modified code:

            crates/dodot-lib/src/config/
                mod.rs            # add [gates], [pack] os to schema

            crates/dodot-lib/src/packs/
                mod.rs            # pack-level os gate evaluation

            crates/dodot-lib/src/rules/
                scanner.rs        # call into gates::evaluate during entry classification

        :: text ::

    9.2. Trait Surface

        The Handler trait does not change. `GateHandler` implements `Handler` with the existing surface — it's a registered filter handler with dynamically generated rules. Rule generation is a new helper:

        Rule generation:

            pub fn gate_rules_for(gates: &GateTable, host: &HostFacts) -> Vec<Rule> {
                let mut rules = vec![];
                for (label, pred) in gates.iter() {
                    if !pred.matches(host) {
                        rules.push(Rule {
                            handler: HANDLER_GATE.into(),
                            pattern: format!("*._{}.*", label),
                            priority: 75,           // between ignore (100) and skip (50)
                            case_sensitive: true,
                        });
                        rules.push(Rule {
                            handler: HANDLER_GATE.into(),
                            pattern: format!("*/_{}/*", label),
                            priority: 75,
                            case_sensitive: true,
                        });
                    }
                }
                rules
            }

        :: rust ::

        The gate rules are added to the rule set per-pack (because `[gates]` can be pack-overridden). Cost: per-pack rule generation is `O(labels)` and runs once per `dodot up`/`status`.

    9.3. Status Surface

        `dodot status` rendering recognises `HANDLER_GATE` matches and renders them with the "gated out (label=X, current=Y)" line. The rendering helper consults the `GateTable` to produce the human-readable expected/actual.

        Pack-level "inactive (os=...)" rows are rendered by the pack-discovery layer, not by a handler — analogous to how `.dodotignore`-skipped packs are rendered today.

    9.4. Tests

        Unit tests:
            - `gates::parse` for filename grammar parsing (positive, negative, alias, malformed).
            - `gates::resolve` for built-in + user label lookup, AND semantics, dimension validation.
            - `gate_rules_for` for dynamic rule generation across host configurations.

        Integration tests against `TempEnvironment`:
            - Pack with `[pack] os = ["darwin"]` deploys on darwin, inactive on linux.
            - File with `._darwin.sh` deploys on darwin, gated out on linux.
            - Directory `_darwin/` gates a subtree.
            - Compound user label (`arm-mac`) works.
            - Unknown label is a hard error with file path in the message.
            - Gate composes with template (`._darwin.sh.tmpl` renders only on darwin).
            - Gate composes with routing prefix (`home.bashrc._darwin`).
            - `[mappings.gates]` and filename gate on the same file is a hard error (deferred to C4 if `[mappings.gates]` doesn't ship in C1–C3).

        E2E (bats) tests:
            - One darwin-only and one linux-only pack on a real test host; `dodot status` and `dodot up` produce the expected rows and deployments.


10. Documentation Deliverables

    Required docs updates when this lands:

        - New user doc: `docs/user/conditional-running.lex`. Walks through OS gating with examples; mentions arch and user-defined labels in a footnote.
        - Update `docs/user/configuration.lex`: add `[pack] os` and `[gates]` sections.
        - Update `docs/user/templates.lex`: add a "When NOT to use templates" subsection pointing at gates for whole-file gating.
        - Update `docs/reference/handlers.lex`: add `GateHandler` to the handler taxonomy.
        - Update `docs/dev/handlers.lex`: add the new handler module and its dynamic-rule generation.
        - Update `docs/reference/terms-and-concepts.lex`: add `gate`, `gate label`, `gate table` entries.


11. Phased Implementation

    Each phase ships value on its own.

    Phase C1: Filename gating, files only.
        - Built-in gate table (`darwin`, `linux`, `macos`, `arm64`, `x86_64`).
        - Filename grammar: `<stem>._<label>.<ext>` and extensionless `<name>._<label>`.
        - `GateHandler` registered in the filter phase.
        - Dynamic rule generation; failed-gate matches drop, passing-gate matches pass with stripped names.
        - Status renders "gated out" rows.
        - User-defined `[gates]` config section (one-line entries, AND semantics).
        - Tests covering the basic OS use case.

        Smallest shippable slice that solves the original three motivating cases.

    Phase C2: Directory gating.
        - Directory segment `_<label>/` parsing.
        - Composition with routing-prefix subtrees.
        - Tests covering subtree gates inside `_home/`, `_xdg/`, `_app/`.

    Phase C3: Pack-level gating.
        - `[pack] os = [...]` config key.
        - Pack-discovery pass evaluates gate; inactive packs short-circuit at scan.
        - Status renders "inactive (os=...)" rows.
        - Tests covering inactive-pack behaviour.

    Phase C4: Glob-based gating in mappings.
        - `[mappings.gates]` config key.
        - Hard-error conflict with filename grammar on the same file.
        - Useful for repos that can't rename files.

    Phase C5: Adopt round-trip polish.
        - `dodot adopt --only-os <os>` flag.
        - Adopt produces a gated filename when explicitly requested.

    C1 is the minimum viable feature. C2 and C3 are independent of each other and ship in either order. C4 is an escape hatch for legacy repos and can be deferred indefinitely. C5 is ergonomic polish.


12. What This Costs the User

    Being honest about the price tag:

        - One new grammar to learn: `_<label>` as filename infix or directory segment. The token universe is small (six built-ins) and self-explanatory.
        - One config key (`[pack] os`) and one config table (`[gates]`) — both opt-in, both inert if absent.
        - A nudge in user docs: "when you'd reach for a template just to skip a file, use a gate instead."

    What this does NOT cost:

        - No CLI surface change. No new commands, no new flags (until C5).
        - No workflow change. `dodot up` and `dodot status` work the same way; gates surface in status output but don't require user interaction.
        - No template migration. Existing `{% if dodot.os %}` templates keep working; users can migrate at their own pace.
        - No git integration cost. Gates do not enter the preprocessing pipeline, do not contribute to the baseline cache, do not require a clean filter.
        - No lock-in. Removing a gate is a file rename; removing the feature entirely is a `dodot up` away.


13. Open Questions

    13.1. Built-in Hostname Labels?

        Should dodot ship with built-in labels for common hostname patterns (`{ hostname = $current_hostname }` as label `host`)? Probably not: hostnames are user-specific and the user-defined-label path covers the case cleanly.

    13.2. WSL Detection?

        WSL hosts report `linux` for `dodot.os` but have substantially different filesystem expectations (no `/Library`, possible `/mnt/c`, etc.). Should we add a `wsl` built-in label? Recommend deferring until a real WSL user surfaces a concrete need; the user-defined-label path covers it (`wsl = { os = "linux", ??? }` — there's no clean dimension to detect WSL from the existing `dodot.*` namespace, so this might motivate adding a `dodot.platform` or `dodot.kernel` field).

    13.3. Case Sensitivity of Labels?

        macOS filesystems are case-insensitive by default; Linux is case-sensitive. If a user names a file `install._Darwin.sh` (capital D), what happens? Recommend: labels are case-sensitive (compared exactly to the table). An entry `_Darwin` is unknown; hard error. The hardness encourages consistent naming and avoids a "did the user mean `darwin`?" silent miss.

    13.4. Empty-Match Behaviour?

        If `[gates]` has no user entries and a pack has no gated files, the entire feature is invisible — no rules generated, no scanner overhead beyond the per-entry parse-attempt. Confirm this is the expected zero-cost path.

    13.5. Should `[pack] os` Generalize to `[pack] gates`?

        v1 ships only `[pack] os` for the concrete OS-only case (per user feedback during design). If we later want pack-level architecture or hostname gating, do we add `[pack] arch`, `[pack] hostname`, ... or a single `[pack] gates = ["arm-mac"]`? Recommend deferring until demand surfaces; either choice is additive.

    13.6. `dodot config gen` Output

        `dodot config gen` emits a starter `.dodot.toml` with all keys commented out. Should it include the entire built-in `[gates]` table commented? Recommend: just `# [gates]` with a one-line example, since the built-ins are not user-overridable in practice (and the docs cover them). Keeps the generated file from ballooning.


14. Implementation Notes vs. Spec

    The implementation deviates from the spec above in a few places. Listed here so future readers don't mistake the spec for the source of truth:

    14.1. Rightmost-wins Filename Parser, Not "At Most One"

        Spec §2.3 / §8.2 originally claimed "a file may carry at most one filename gate token." The shipped parser (`gates::parse_basename_gate`) instead uses rightmost-wins: given multiple `._<token>` segments in a basename (`foo._bar._darwin.sh`), only the rightmost (`_darwin`) is treated as a gate; everything to its left stays in the stem (`foo._bar.sh`). This is a friendlier behaviour — it lets users have legitimate name components like `foo._bar.…` that aren't gates without forcing a hard error — and the spec was updated post-review to describe the rightmost-wins rule explicitly. Cross-reference: §2.3 paragraph "Implementation note" and §8.2 second paragraph.

    14.2. `darwin` vs `macos` Naming Disagrees with Templates

        Spec §5.2 and §6.1 implicitly assume the gate `dimension = value` checks compare against the same OS values templates expose as `dodot.os`. They don't. Templates pass through `std::env::consts::OS` as `"macos"` on macOS hosts; the gate machinery normalises the host OS to `"darwin"` (the Rust `target_os` name) and accepts `"macos"` only as an alias. The two surfaces disagree on the canonical macOS string. Documented in [./../../user/configuration.lex] §2.1 and [./../../user/conditional-running.lex] §5; the gate side ships with the alias bridge so `[pack] os = ["macos"]` Just Works, but the asymmetry is a real wart for users mixing both surfaces.

    14.3. Helper Renamed to `filter_pre_preprocess_gates`

        Spec §9.2 sketched a `gate_rules_for(...)` helper for dynamic rule generation. The shipped form is `filter_pre_preprocess_gates` (in `crates/dodot-lib/src/packs/orchestration.rs`), which evaluates all three gate sources (basename suffix, directory segment, `[mappings.gates]` glob) before preprocessing runs. Same role in the pipeline (filter-phase gate evaluation) but a different name and a broader scope than the spec suggested — the spec assumed only basename gates would need pre-preprocess handling, but mapping-gates also reach the preprocessor and needed the same treatment.

    14.4. Gates Inside Routing-Prefix Subtrees Are Not Supported

        Spec §5.1 implied gate dirs could nest inside routing-prefix subtrees (`_home/_darwin/.bashrc`). The shipped scanner only evaluates gate dirs at the pack root; the symlink handler's per-file recursion inside `_home/`, `_xdg/`, `_app/`, `_lib/` is intentionally gate-unaware. The supported nesting is the *opposite* — gate at the outer level: `_darwin/_home/.bashrc`. Spec was updated post-review (§5.1 example table, §8.8 deferral note) to describe the supported shape and document the inverse case as future work. The reverse case (gate inside routing prefix) is straightforward to implement — extend the symlink handler's recursion to consult the gate table per directory segment — but adds dispatch logic to a code path the trait is otherwise minimal in. Deferred until a real use case shows the outer-gate form is insufficient.

    14.5. `--only-os` Validates Against Root Config Only

        Spec §11 (phase C5) said `--only-os` validates the label against the resolved gate table. The shipped form validates against the *root* `.dodot.toml`'s `[gates]` only — labels defined exclusively in a pack-level `.dodot.toml` are not visible because adopt validates before it knows which pack the source maps into (and pack inference can require `--into`, which adopts validates *after*). Documented in [./../../user/conditional-running.lex] §8. Users who want a custom label for `--only-os` define it in the root config; the label is still referenceable from any pack via filename grammar or `[mappings.gates]`.

    14.6. `[mappings.gates]` Glob Scope Is Top-Level

        Spec §6.1 / §7 example showed `[mappings.gates] "setup/*.sh" = "linux"` suggesting nested-path globs work. They don't, in the shipped implementation: the scanner surfaces top-level entries and `[mappings.gates]` matches against *that* shape — globs containing path separators only fire when a top-level entry has the matching shape. The symlink handler's nested per-file recursion is intentionally gate-unaware (mirroring §14.4's directory-gate limitation). Spec was updated post-review (§6.1) to remove the misleading nested example and describe the depth-1 scope explicitly. Same future-work note as §14.4 — extending nested matching is straightforward but waits for real demand.

    14.7. Status Footnote Format

        Spec §7.2 sketched a status row reading `gated out (label=X, current=Y)`. The shipped renderer uses two parts: the row label is `gated out (X)` (just the label name) and a footnote stamps `expected os=darwin; got os=linux` (test-failure idiom) — see [./../../user/conditional-running.lex] §10. The spec's combined form was rejected during implementation as harder to scan when many rows share the same gate; splitting label-on-row from predicate-in-footnote keeps the per-row column alignment stable.

    14.8. `HostFacts` Caching on `ExecutionContext`

        Spec §7.1 / §9.1 didn't specify when host facts get detected. The shipped form caches them once per `ExecutionContext` (`HostFacts::detect()` runs in `production()`); per-pack scanning then borrows `&HostFacts` from the context instead of re-detecting. Without this, the `hostname(1)` shell-out fired per pack — a measurable per-run cost on repos with many packs, and a correctness concern when system state changes mid-run. The detection helpers (`detect_hostname` / `detect_username`) are also shared with the template preprocessor's `dodot.hostname` / `dodot.username` resolution so the two paths can never disagree.
