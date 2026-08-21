:: verified ::
Execution order

The order in which handlers run within a pack, and the order in which packs run relative to each other. Both orderings are fixed and visible — you don't have to guess what runs first. The one exception, in §4, is two handlers that share a phase: their order relative to each other is not specified.

1. Within a pack: phases

    :: handler-roster:begin ::

    Inside a single pack, every handler belongs to one of seven phases. They run in this fixed order:

        | Order | Phase      | Handlers            | Why this slot                                                              |
        | 1     | Filter     | ignore, skip, gate  | Emit no work at all; listed first so the phases read like the priority ladder. |
        | 2     | External   | external            | Fetch upstream content first, so later handlers find it already at its target path. |
        | 3     | Provision  | homebrew, nix       | Install packages, so anything later may use what they put on `$PATH`.      |
        | 4     | Setup      | install             | User setup scripts that may rely on Provision having completed.            |
        | 5     | PathExport | path                | Stage `bin/` directories onto `$PATH` before shell init reads it.          |
        | 6     | ShellInit  | shell               | Register shell startup files, which can reference PathExport executables.  |
        | 7     | Link       | symlink             | Catch-all; deploys whatever no precise handler claimed.                    |

    :: table align=rlll ::

    :: handler-roster:end ::

    The order is encoded as a Rust `enum` declared in execution order in `crates/dodot-lib/src/handlers/mod.rs`. Adding or moving a phase is a visible, deliberate code change — not an accident of alphabetical sort.

    What the phase order does *not* decide is which handler gets a file. That is settled earlier, when dodot matches each file against the rules: the highest-priority pattern wins, and gates are evaluated before matching runs at all. So a `README` is kept off the symlink handler by `skip` sitting at priority 50, not by the Filter phase running first — see [./mappings.lex]. By the time phases matter, every file already has its handler, and the phase order only says who acts first.

    The roster above is reproduced here because phase order is what this page is about; the copy that cannot drift is generated straight from the registry at [./../../reference/handler-registry.lex], which also carries each handler's category, match mode, and default claims.

2. Cross-pack: lexicographic by directory name

    Across packs, dodot processes packs in lexicographic order of their on-disk directory names. For most pack arrangements that's `aws`, `git`, `nvim`, `zsh` — alphabetical, no surprises.

    For the small handful of cases where pack-to-pack ordering matters — `compinit` after completion plugins are on `$PATH`, a helper function defined before the alias file that uses it, … — name your pack directories with a numeric prefix so lexicographic order matches the order you want.

    The historically most common occasion for reaching for a prefix — a `001-homebrew` bootstrap pack, so brew's `shellenv` runs before anything that calls `brew` — is retired: the init script now opens with Homebrew's environment by itself, before any pack contribution ([./../configuration.lex] §10). The prefix grammar below is unchanged; it just has one less job.

    This lex order is also what ranks packs against each other on `$PATH` specifically — but there it runs in the *opposite* direction from reading order (the last pack on disk wins the front of `$PATH`), and composes against a Homebrew tier and a system-`$PATH` floor beneath it. See [./path.lex] §3 for that contract, the worked example, and how a raw `export PATH=` mutation inside a pack's own shell script is attributed rather than dropped.

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

    A phase can hold more than one handler — Filter holds `ignore`, `skip`, and `gate`; Provision holds `homebrew` and `nix`. The order two handlers inside one phase run in is not defined: dodot sorts handler names by phase and nothing more, so the relative order of two handlers sharing a phase is whatever the sort happens to produce. Nothing in dodot depends on it, and neither should a pack.

    What you can rely on is the phase boundary. Both Provision handlers finish before any Setup handler starts, so an `install.sh` may use what a `Brewfile` or a `packages.nix` installed — but a `Brewfile` must not assume `packages.nix` already ran, or the reverse.

    Within one handler's matches for a single pack, file order follows the rule-priority then declaration order described in [./mappings.lex]. Across packs in the same phase, pack order is the cross-pack lexicographic order from §2.

5. Renaming for order

    Adding, removing, or changing a pack's ordering prefix takes effect on the next `dodot up`. There's no "ordering" state stored anywhere — the order is recomputed every run from the on-disk directory names dodot finds. Renaming `git/` to `200-git/` is a one-step change.
