:: verified ::
Git augmentation

dodot does a few things that need git's cooperation. Templates produce a deployed file whose bytes diverge from the source. macOS plists are binary, but git diffs only make sense in XML. Source mtimes need touching so git's stat-cache notices when you edited the deployed side. Each of these is a tiny, well-scoped piece of git wiring — and dodot offers to install whichever ones your repo actually needs.

This page is the conceptual map. The individual commands ([./git-install-filters.lex], [./template.lex], [./transform.lex], [./git-install-alias.lex]) are still the per-feature reference.

1. The four pieces

    Three of these are git-side filters or hooks; the fourth is a shell alias that wraps `git`.

        | Piece                       | Installed by                      | What it does                                                                  |
        | Pre-commit hook             | `dodot transform install-hook`    | Reverse-merge any deployed-side edits back into template sources at commit.   |
        | Plist clean/smudge filter   | `dodot git-install-filters`       | Translate macOS `*.plist` between binary (working tree) and canonical XML (git). |
        | Template clean filter       | `dodot template install-filter`   | Show deployed-side edits in `git status` / `git diff` as if you'd edited the source. |
        | Shell alias (Tier 2)        | `dodot git-install-alias`         | Wrap `git` so `dodot refresh --quiet` runs before every git invocation.        |

    :: table align=lll ::

    None of these is mandatory. They light up specific features:

    - You don't have templates? Skip the pre-commit hook and the template clean filter.
    - You don't have plists? Skip the plist filters.
    - You don't mind running `dodot refresh` by hand before `git status`? Skip the alias.

2. The install ladder — one Y/n during `up`

    On the first `dodot up` that detects features needing wiring, dodot offers to install whichever of the three filter/hook rungs apply, in one consolidated Y/n prompt — the *install ladder*. Three rungs, in dependency order:

    1. Pre-commit hook
    2. Plist clean/smudge filter
    3. Template clean filter

    The rungs are gated by detection: dodot only offers the pre-commit hook and template filter when your repo has templates with cached baselines; offers the plist filter only when there are tracked `.plist` files in a pack.

    Three responses:

    - `Yes` — install every applicable rung whose component dismissal is not set, in order. Each successful install dismisses its component key.
    - `Show` — walk each rung individually, printing the previewable block (`.git/config` snippet, hook script, `.gitattributes` line) without touching anything. Re-run `up` to get the prompt again for real.
    - `No` — dismiss every applicable component key. Subsequent `up` runs won't re-prompt. Resurface a single rung with `dodot prompts reset <key>` (e.g. `dodot prompts reset plist.install_filters`).

    The Tier-2 shell alias is *not* in the install ladder. It's an opinionated workflow choice that ships separately via `dodot git-install-alias`.

3. Per-clone, per-machine vs committed state

    Each rung has two halves — one in the repo (committed, travels with clones) and one outside (local, per-machine). Knowing which is which avoids the "I cloned on a new machine and nothing works" surprise.

        | Rung                    | Committed (repo)                                | Per-clone, per-machine                                |
        | Pre-commit hook         | (nothing)                                       | `.git/hooks/pre-commit` block                          |
        | Plist filter            | `.gitattributes` line `*.plist filter=dodot-plist` | `.git/config` `[filter "dodot-plist"]` block        |
        | Template filter         | `.gitattributes` line `*.tmpl filter=dodot-template` | `.git/config` `[filter "dodot-template"]` block   |
        | Tier-2 alias            | (nothing)                                       | `~/.bashrc` / `~/.zshrc` alias line                    |

    :: table align=lll ::

    The committed halves are inert without the per-machine halves. A fresh clone has the `.gitattributes` lines but no `.git/config` filter blocks, so `git` will see "filter=dodot-plist" referenced and fail loudly until `dodot up` (and the install ladder) wires up the missing half.

4. Why each piece exists

    4.1. Pre-commit hook (`dodot transform install-hook`)

        Reverse-merges deployed-side template edits back into source before the commit goes through. Two-step:

        1. `dodot refresh --quiet` — touch source mtimes so git's stat-cache notices any divergence.
        2. `dodot transform check --strict` — reverse-merge the deployed-side bytes into the template source. `--strict` fails the commit if any unresolved conflict markers remain.

        Without the hook, you can `git commit -a` and silently lose deployed-side edits because the template source still holds the *pre-edit* content. With the hook, the commit either includes your edit or fails loudly.

    4.2. Plist clean/smudge filter (`dodot git-install-filters`)

        macOS prefers binary plists. Git diffs prefer text. The clean filter converts the working-tree binary plist to canonical XML on stage so the repo carries readable diffs; the smudge filter converts back to binary on checkout so macOS apps still read it correctly.

        Side effect on macOS: pulling plist changes from another machine updates the on-disk binary, but `cfprefsd` may keep serving stale values from its in-memory cache to running apps. After `up` detects a plist change, dodot offers to run `killall cfprefsd`; you can also run that by hand at any time.

    4.3. Template clean filter (`dodot template install-filter`)

        Shows deployed-side edits in `git status` / `git diff` as if they had been written to the template source.

        Mechanics: when git reads a template source file (because [./refresh.lex] touched its mtime), the filter compares the deployed bytes to a cached baseline. If they match, the filter is identity. If they diverge, the filter rehydrates the cached render, generates a diff against the baseline, and applies that diff to the template — emitting a patched-source view to git. So `git diff` shows the deployed-side edit as a source-side change.

        The smudge half is `cat` (identity). Templates must never re-render at smudge time — that would re-prompt for secret-provider auth on every `git checkout`, exactly the auth-fatigue scenario this design rules out.

    4.4. Shell alias (`dodot git-install-alias`)

        Writes one line to your shell rc:

            alias git='dodot refresh --quiet && command git'

        :: shell ::

        With this alias active, every `git` invocation runs `dodot refresh --quiet` first — so deployed-side template edits surface in `git status`, `git diff`, etc. without you having to remember.

        Tier-2 because the pre-commit hook covers the commit case authoritatively; the alias is for everything else (status, diff, stash, log) where you want git to reflect current truth without an extra command.

5. Manual install — per-feature entry points

    The install ladder is the recommended path. If you'd rather install rungs individually, or you've dismissed the ladder and want one rung anyway, each piece has its own command:

        | Piece                  | Install                                 | Inspect / preview                       |
        | Pre-commit hook        | `dodot transform install-hook`          | (read `.git/hooks/pre-commit` directly) |
        | Plist filter           | `dodot git-install-filters`             | `dodot git-show-filters`                |
        | Template filter        | `dodot template install-filter`         | (read `.git/config` directly)           |
        | Tier-2 shell alias     | `dodot git-install-alias`               | `dodot git-show-alias`                  |

    :: table align=lll ::

    All four installers are idempotent. Re-running on an already-installed rung is a no-op success that reports "already installed."

6. Resurfacing a dismissed prompt

    If you dismissed the install ladder and now want it back, `dodot prompts` is the entry point:

        dodot prompts list                              # see every prompt and its state
        dodot prompts reset magic.install_ladder        # bring the whole ladder back
        dodot prompts reset plist.install_filters       # bring back just one rung

    :: shell ::

    See [./prompts.lex] for the full surface.

7. Watch out for

    - *`$PATH` for git-invoked dodot.* Filter blocks reference the bare command `dodot` (e.g. `dodot plist clean`), so `dodot` must be on `$PATH` for whatever process invokes git — shell, editor, GUI git client. If a GUI git tool runs from a reduced environment that does not see your shell's `$PATH`, git will fail loudly because `required = true` is set on the filter blocks. Fix by putting dodot on the system `$PATH` (e.g. `/usr/local/bin/dodot`) or by hand-editing the filter block to use an absolute path.
    - *Filters are content-only, not metadata.* Adding the filter doesn't pull in your previously-committed binary plists or unrendered templates. Running `git checkout -- <file>` (or `git rm --cached <file>` then re-stage) re-applies the filter to existing files. The install ladder doesn't do this for you.
    - *`required = true` is intentional.* dodot writes the filter blocks with `required = true` so git refuses to silently skip them. The trade-off: a misconfigured filter (PATH issue, missing dodot binary) makes git noisy in a way that's recoverable. Removing `required = true` to "make git quieter" hides real failures.
    - *No combined "install everything" command outside the ladder.* `git-install-filters` is *plist-only* despite the generic name; the template filter installs via `template install-filter`; the hook installs via `transform install-hook`. The ladder is the only place all three are offered as one action.
