Template Magic

    dodot turns templates from "files you have to render manually" into files you edit live and commit normally. Once set up, your daily workflow stays vanilla git — `git status`, `git diff`, `git commit` all see the right thing without you running any dodot-specific commands.

    This is the user-facing walkthrough. The architectural reasoning lives at [./../proposals/magic.lex].

1. The Setup Ladder

    Three install steps, in order. Each one buys a tier of correctness. You can stop at any rung — Tier 1 is the no-cost path that gives you correct commits; Tiers 2 and 3 add ambient `git status` truthfulness for users who want it.

    1.1. Tier 1 — the pre-commit hook (always recommended)

        After your first `dodot up` that deploys a template file, dodot offers to install the pre-commit hook. Accept it once per clone, per machine, and forget about it.

        Manual install:

            dodot transform install-hook

        :: shell ::

        What it does: writes `<dotfiles_root>/.git/hooks/pre-commit` with two lines:

            dodot refresh --quiet || exit 1
            dodot transform check --strict || exit 1

        :: text ::

        On every `git commit`, the hook refuses the commit if any deployed template has drifted from its baseline (running `dodot transform check`'s 4-state matrix and applying reverse-merge diffs back to source on the unambiguous cases) or if any source file carries unresolved `dodot-conflict` markers. Re-run `git add` after a refused commit and try again.

        Idempotent: re-running `install-hook` on a current block is a no-op, on an older block rewrites it in place, and on a hook file with non-dodot content appends our block while preserving everything else.

    1.2. Tier 2 — the template clean filter (recommended once you've used Tier 1 for a bit)

        Tier 1 catches drift at commit time. Tier 2 makes it visible at `git status` time too.

        Manual install:

            dodot template install-filter

        :: shell ::

        Plus one line in the repo's `.gitattributes`:

            *.tmpl filter=dodot-template

        :: text ::

        What it does: writes a `[filter "dodot-template"]` block to `.git/config` with `clean = dodot template clean --path %f`, `smudge = cat`, `required = true`. From then on, every `git status` / `git diff` invocation that re-reads a `.tmpl` source goes through the filter — which compares the current deployed file to the cached baseline and emits the patched template (or the original, on a fast path when nothing's changed).

        The filter never re-renders the template. That matters specifically because a re-render would re-trigger any `secret(...)` provider auth. Your password manager doesn't pop up every time you hit Enter in a shell.

        Note that Tier 2 by itself isn't enough — git's stat-cache uses mtimes to decide whether to re-read a working-tree file at all, so without Tier 3 (or a manual `dodot refresh`), a deployed-side edit doesn't trigger the filter on `git status`. Either install Tier 3 or run `dodot refresh` before the git command.

    1.3. Tier 3 — the interactive shell alias (opt-in)

        Manual install:

            dodot git-install-alias       # auto-detects from $SHELL
            dodot git-install-alias --shell zsh

        :: shell ::

        What it does: writes a managed block to `~/.bashrc` or `~/.zshrc`:

            alias git='dodot refresh --quiet && command git'

        :: text ::

        Now every `git` invocation in your interactive shell runs `dodot refresh` first — touches source mtimes for any deployed-side edits, so git re-reads through the filter and `git status` reflects the truth.

        Print without writing: `dodot git-show-alias`. Use this if you want to inspect the block first or paste it into a non-standard rc file. fish / nu / other shells aren't auto-supported — the show command can produce a bash snippet for adaptation.

2. The Day-to-Day Workflow

    Once installed, you don't run dodot directly during normal editing.

    2.1. Edit through the deployed file

        Open `~/.config/app/cfg.toml` (the deployed symlink) in your editor; change `port = 5432` to `port = 9999`; save. That's it.

    2.2. Inspect with vanilla git

        `git status` shows `app/cfg.toml.tmpl` as modified.

        `git diff` shows the template-space change:

            -port = 5432
            +port = 9999

        :: text ::

        The `{{ name }}` line in the template stays untouched; only the static line that you actually edited propagates back.

    2.3. Commit

        `git add` and `git commit` like normal. The pre-commit hook runs `dodot transform check --strict` — on a clean state this is silent and the commit proceeds; on unresolved conflict markers it refuses.

3. Conflicts

    burgertocow (the engine that builds template-space diffs) is honest about what it can't safely auto-merge. When you edit a line that overlaps a `{{ var }}` region, or different lines of a `{% for %}` loop differently, the reverse-merge can't pick a single template-space replacement. In those cases dodot inserts a conflict block:

        >>>>>> dodot-conflict (template)
        host = "{{ env.DB_HOST }}"
        ====== dodot-conflict (deployed)
        host = "production.db.internal"
        <<<<<< dodot-conflict

    :: text ::

    Resolve by editing — keep one side, remove the markers — then `git add` and commit. The pre-commit hook refuses to commit while markers are still present, so you can't ship a template with a half-resolved block by accident.

4. Inspecting State

    4.1. `dodot transform status`

        Read-only view. Walks the baseline cache and reports per-file state:

            $ dodot transform status
            1 synced, 1 diverged, 0 missing

            · synced app/cfg.toml.tmpl
            · output_changed app/greeting.tmpl (deployed edited; run dodot transform check)

        :: text ::

        States: `synced` (clean), `input_changed` (source edited; next `up` re-renders), `output_changed` (deployed edited; reverse-merge can apply), `both_changed` (both sides edited; conflict likely), `missing_source` (cache stale), `missing_deployed` (rendered file gone). Always exits 0 — informational.

    4.2. `dodot transform check [--strict] [--dry-run]`

        Active form. Walks the cache and either:

        - Applies a reverse-merge diff back to the source when burgertocow + diffy produce an unambiguous unified patch — `Patched`. Exit 0; nothing for the user to review. The patched source surfaces as modified on the next `git status`; the user runs `git add` and lands a follow-up commit (or amends).
        - Surfaces a conflict block in the source for the ambiguous cases — `Conflict`. Exit 1; user picks a side and resolves the markers, the same way they would resolve a `git merge` conflict.
        - Reports `MissingSource` / `MissingDeployed` / `NeedsRebaseline` (cache invariant broken). Exit 1.

        `--dry-run` reports without writing. `--strict` additionally fails on unresolved markers in any source. The pre-commit hook calls `transform check --strict`; the exit-code split above means the hook lets the original commit proceed when reverse-merge succeeds cleanly, and refuses only when human review is genuinely needed.

    4.3. `dodot refresh [--quiet] [--list-paths]`

        Touches source mtimes for any deployed-side divergence so git's stat-cache invalidates. The Tier 3 alias calls this with `--quiet`. The `--list-paths` mode prints divergent source paths and exits without writing — for editor / file-watcher integrations that want to drive the touch themselves.

    4.4. `dodot up` and divergent deployed files

        `dodot up` will not overwrite a deployed file whose bytes have diverged from the cached baseline — that is, a deployed file you've edited in place since the last successful `up`. The render is skipped and a one-line warning surfaces:

            preserved ~/.config/app/cfg.toml (deployed file was edited since the last `dodot up`).
            Run `dodot transform check` to reconcile, or re-run with --force to overwrite.

        :: text ::

        Two resolution paths:

        - `dodot transform check` — runs the 4-state matrix (§4.2) and applies a reverse-merge diff back to the source on the unambiguous case. Then `dodot up` proceeds normally on the next run.

        - `dodot up --force` — overwrites the deployed file with the rendered output, discarding the in-place edit. The escape hatch when a user knows they want the freshly-rendered output (most commonly: an env var that a template references has rotated, and the user wants the new value to land).

        Staleness is defined from file content, not from the runtime environment. Env vars referenced via `{{ env.X }}` are read live at render time and are intentionally *not* part of the cache-invalidation signal — see [./../proposals/preprocessing-pipeline.lex] §6.4. Stable values that should participate in invalidation belong in `[preprocessor.template.vars]` (the `user_vars` namespace), not `env.*`.

5. Opting Out

    Three opt-out levels (in addition to the standard `[mappings] ignore = [...]` for silent drop, or `[mappings] skip = [...]` for visible-but-undeployed):

    5.1. Disable preprocessing entirely

        [preprocessor]
        enabled = false

        :: toml ::

        All `.tmpl` files deploy verbatim with no rendering, no cache, no filter activity.

    5.2. Disable template rendering specifically

        [preprocessor.template]
        enabled = false

        :: toml ::

        Templates skipped; other preprocessors still run.

    5.3. Per-file: skip reverse-merge while keeping rendering

        [preprocessor.template]
        no_reverse = ["complex-config.toml.tmpl", "*.gen.tmpl"]

        :: toml ::

        Glob patterns. Files matching are still rendered on `dodot up` and tracked in the divergence cache, but `transform check` and the clean filter both skip the reverse-merge step (echo stdin / report-only). Useful for templates whose content is mostly dynamic — burgertocow's heuristic degrades on those and produces more conflict markers than usable diffs.

        Divergence is still detected; only auto-merge is skipped.

    5.4. Roll back the install

        Filters: edit `.git/config` and remove the `[filter "dodot-template"]` block (and the `*.tmpl filter=dodot-template` line in `.gitattributes`).
        Hook: edit `.git/hooks/pre-commit` and delete the block between `# >>> dodot transform check --strict (managed by ...)` and `# <<< dodot transform check --strict <<<`.
        Alias: edit `~/.bashrc` or `~/.zshrc` and delete the block between `# >>> dodot git alias (managed by ...)` and `# <<< dodot git alias <<<`.

        Each block is grep-detectable by the `(managed by` token, so a one-liner `sed` suffices for scripted rollback.

6. The Cost Ladder, Honestly

    Per machine:

        - one Y/n to install the pre-commit hook (Tier 1)
        - one Y/n to install the clean filter (Tier 2; offered after Tier 1 is in place)
        - optionally, one Y/n to install the interactive alias (Tier 3)
        - re-running each install on each new clone, because none of these live in the repo

    Not paid:

        - no new CLI commands the user has to remember in daily use
        - no workflow changes — you commit, diff, and status with vanilla git
        - no auth prompts from passive git commands, ever
        - no dodot-owned shell, editor, or daemon

7. Where Things Live

    7.1. Baseline cache

        `<XDG_CACHE_HOME>/dodot/preprocessor/<pack>/preprocessed/<filename>.json`

        One JSON per processed file. Fields: `version`, `source_path`, `rendered_hash`, `rendered_content`, `source_hash`, `context_hash`, `tracked_render`, `timestamp`. Re-derivable — losing the cache forces the next `dodot up` to re-baseline. The cache is what makes the clean filter cheap (fast-path is a hash compare; slow-path is `TrackedRender::from_tracked_string` + diffy, never a re-render).

    7.2. Hook block

        `<dotfiles_root>/.git/hooks/pre-commit`, between guard lines `# >>> dodot transform check --strict (managed by ...)` and `# <<< dodot transform check --strict <<<`.

    7.3. Filter config

        `<dotfiles_root>/.git/config`, the `[filter "dodot-template"]` block.

    7.4. Alias block

        `~/.bashrc` or `~/.zshrc`, between guard lines `# >>> dodot git alias (managed by ...)` and `# <<< dodot git alias <<<`.

8. When To Reach For Each Command

    The daily-use answer is "none of them — git is the interface." But the manual escape hatches matter when something's off:

        - `dodot transform status` — quick read on what's diverged. Doesn't change anything.
        - `dodot transform check` — propagate deployed-side edits back to source manually (e.g. when you don't have the hook installed yet).
        - `dodot transform check --strict` — what the hook runs.
        - `dodot transform check --dry-run` — preview what would change without writing.
        - `dodot refresh` — bump source mtimes after editing the deployed file. The Tier 3 alias does this automatically.
        - `dodot transform install-hook` — install or upgrade the hook block.
        - `dodot template install-filter` — register the clean filter.
        - `dodot git-install-alias` — wire git through `dodot refresh`.
