:: verified ::
Execution order

The order in which handlers run within a pack, and the order in which packs run relative to each other. Both orderings are deterministic and visible — you don't have to guess what runs first.

1. Within a pack: phases

    Inside a single pack, every handler belongs to one of six phases. They run in this fixed order:

        | Order | Phase      | Handler             | Why this slot                                                              |
        | 1     | Filter     | ignore, skip, gate  | Drop matched source files before any deploying handler can claim them.    |
        | 2     | Provision  | homebrew            | Install packages first, so anything later may use what brew put on PATH.  |
        | 3     | Setup      | install             | User setup scripts that may rely on Provision having completed.           |
        | 4     | PathExport | path                | Stage `bin/` directories onto `$PATH` before shell init reads it.         |
        | 5     | ShellInit  | shell               | Register shell startup files, which can reference PathExport executables. |
        | 6     | Link       | symlink             | Catch-all; runs last because precise handlers must claim their files first. |

    :: table align=rlll ::

    The order is encoded as a Rust `enum` declared in execution order in `crates/dodot-lib/src/handlers/mod.rs`. Adding or moving a phase is a visible, deliberate code change — not an accident of alphabetical sort.

2. Cross-pack: lexicographic by directory name

    Across packs, dodot processes packs in lexicographic order of their on-disk directory names. For most pack arrangements that's `aws`, `git`, `nvim`, `zsh` — alphabetical, no surprises.

    For the small handful of cases where pack-to-pack ordering matters — Homebrew's `shellenv` before anything that calls `brew`, `compinit` after completion plugins are on `$PATH`, … — name your pack directories with a numeric prefix so lexicographic order matches the order you want.

3. The ordering-prefix grammar

    A pack directory name matching `^(\d+)[-_](.+)$` (digits, then `-` or `_`, then a non-empty name) is recognised as carrying an ordering prefix. Both forms are accepted; the choice is yours, dodot doesn't care.

    Examples:

        010-brew/      ->  display name "brew",     sorts very early
        100-zsh/       ->  display name "zsh",      sorts after 010-brew
        900-starship/  ->  display name "starship", sorts late
        020_python/    ->  display name "python",   underscore separator also valid

    :: text ::

    The prefix is invisible to user-facing surfaces. `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`, not `~/.config/010-nvim/init.lua`. `dodot status` shows `nvim`, not `010-nvim`. Only the on-disk directory name carries the prefix, so it's the only place lexicographic sort sees it.

    Zero-pad the digit run if you want comparisons to stay numeric — `010` < `100` lexicographically, `10` > `100` lexicographically. Pick a width and keep to it.

    A directory whose name is *just* an ordering prefix with nothing after the separator (e.g. `010-`, `020_`) is rejected at scan time as a malformed pack — a pack must have a name.

4. Within a phase: same-phase ordering is not specified

    Two handlers in the same phase don't currently exist (each phase is a single handler today), so cross-handler-within-phase order doesn't come up. Within one handler's matches for a single pack, file order follows the rule-priority then declaration order described in [./mappings.lex]. Across packs in the same phase, pack order is the cross-pack lexicographic order from §2.

5. Renaming for order

    Adding, removing, or changing a pack's ordering prefix takes effect on the next `dodot up`. There's no "ordering" state stored anywhere — the order is recomputed every run from the on-disk directory names dodot finds. Renaming `git/` to `200-git/` is a one-step change.
