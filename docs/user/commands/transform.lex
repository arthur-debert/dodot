:: verified ::
dodot transform

The "reverse-merge deployed edits back into template sources" command family. Three subcommands:

- `dodot transform check` — the engine that does the reverse-merge.
- `dodot transform install-hook` — install the pre-commit hook that runs `transform check --strict` before every commit.
- `dodot transform status` — read-only state report for every cached preprocessed file.

This is the pre-commit-hook rung of the git-augmentation set. See [./git-augmentation.lex] for the full landscape.

1. transform check

    The engine. For every cached preprocessor baseline (one per template-rendered file), compares the deployed bytes to the baseline's recorded render. If they diverge, generates a diff and applies it back to the template source on disk. Conflict markers land inline at any block where the merge can't be resolved automatically.

    Flags:

        | Flag             | Effect                                                                                  |
        | `--strict`       | Also fail (exit code 1) if any source carries unresolved `dodot-conflict` markers.       |
        | `--dry-run`      | Report what would be patched without writing to source files.                            |

    :: table align=ll ::

    `--strict` is what makes the pre-commit hook block bad commits. Without `--strict`, `transform check` is best-effort — it merges what it can and lets you resolve markers later.

    Examples:

        dodot transform check                  # reverse-merge any divergent templates
        dodot transform check --dry-run        # preview what would be patched
        dodot transform check --strict         # fail on unresolved markers (what the hook calls)

    :: shell ::

2. transform install-hook

    Installs `<dotfiles_root>/.git/hooks/pre-commit` so every `git commit` runs:

        dodot refresh --quiet || exit 1
        dodot transform check --strict || exit 1

    :: shell ::

    The two lines together: refresh forces git's stat-cache to notice deployed-side edits, then strict-mode `transform check` reverse-merges them and fails the commit if any conflict markers remain unresolved.

    Behavior:

    - No `pre-commit` file yet → creates one with `#!/bin/sh` plus dodot's managed block, mode `0o755`.
    - Existing `pre-commit` without our managed-block markers → appends our block, preserving everything else.
    - Existing `pre-commit` with our markers, content matches → no-op success.
    - Existing `pre-commit` with our markers, content stale → rewrites our block in place, preserving everything outside the markers.

    Idempotent. Errors only when `<dotfiles_root>/.git` doesn't exist (the dotfiles repo isn't a git working tree).

    Examples:

        dodot transform install-hook           # install the hook
        cat .git/hooks/pre-commit              # see the managed block in context

    :: shell ::

3. transform status

    Read-only state report. For every cached preprocessed file, shows where it sits in the synced ↔ divergent matrix:

        | State              | What it means                                                                       |
        | `synced`           | Source and deployed are both identical to the recorded baseline. Clean.             |
        | `output-changed`   | Deployed bytes diverge from the baseline (someone edited the deployed file).        |
        | `input-changed`    | Source bytes diverge from the baseline (someone edited the template source).       |
        | `both`             | Both sides diverge. The interesting case for `transform check`.                     |
        | `missing`          | Either source or deployed isn't on disk anymore.                                    |

    :: table align=ll ::

    Useful for: confirming the baseline cache is in sync before a commit, debugging "why isn't `transform check` finding my edit", finding orphan baselines.

    Examples:

        dodot transform status

    :: shell ::

4. The `transform check` ↔ `template clean` split

    Two related but distinct mechanisms:

    - [./template.lex] (the template clean filter) shows deployed-side edits in `git status` / `git diff` *as if* they were source-side edits. Read-only — the template source on disk is untouched.
    - `dodot transform check` actually writes the merged form back to the template source on disk. Used at commit time (via the pre-commit hook) so the committed source is the merged result.

    Both are part of the templates story; you usually want both installed. The install ladder offers them as separate rungs because some users want one without the other.

5. Watch out for

    - *`transform check` writes to source files.* Without `--dry-run`, it modifies your template `.tmpl` sources on disk. The pre-commit hook is the canonical caller; running `transform check` by hand is fine but knowing it writes is important.
    - *Conflict markers are real text in your source.* When automatic merge fails, dodot inserts `dodot-conflict` markers around the contested block. The source file remains valid by your template language's rules (the markers are inside comment-like delimiters), but it's *your* job to resolve them. `--strict` fails the commit until you do.
    - *No-op without templates.* If your repo has no preprocessor baselines, `transform check` and `transform status` are zero-row no-ops. Don't bother installing the hook in that case — the install ladder won't offer it either.
    - *`install-hook` requires a git working tree.* Errors out with a clear message if `.git` is missing. Run `git init` in your dotfiles root first.
