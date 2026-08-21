Handler Registry

    *Generated from the handler registry — do not edit.* Run `pixi run gen-docs` to regenerate this page from `crates/dodot-lib/src/handlers/catalog.rs`; the test suite fails when it drifts. This is the roster that cannot disagree with the shipped registry, so link here rather than copying the tables. A few pages do keep a hand-written roster where naming the handlers is part of explaining something else; they are listed in that same file, and a test holds each of them to naming every registered handler.

    For what each handler is *for*, see [./handlers.lex]; for the per-handler user guides, see [./../user/handlers.lex].

1. Handlers

    Every handler dodot registers, in phase order. `Claims by default` is what the handler matches when the user configures nothing.

    Registered handlers:

        | Handler    | Phase        | Category        | Match mode | Scope     | Claims by default                                                                                                                                                                | Effect                                                                                                      |
        | `gate`     | `Filter`     | `Configuration` | Precise    | Exclusive | Nothing by rule — the scanner mints gate matches from `._<label>` names, `_<label>/` directories, and `[mappings.gates]`                                                         | Drops a file whose gate does not hold on this host, listed as `gated out`                                   |
        | `ignore`   | `Filter`     | `Configuration` | Precise    | Exclusive | Nothing by default — set the matching `[mappings]` key                                                                                                                           | Drops the file silently — nothing runs, nothing is listed                                                   |
        | `skip`     | `Filter`     | `Configuration` | Precise    | Exclusive | `README`, `README.*`, `LICENSE`, `LICENSE.*`, `CHANGELOG`, `CHANGELOG.*`, `CONTRIBUTING`, `CONTRIBUTING.*`, `AUTHORS`, `AUTHORS.*`, `NOTICE`, `NOTICE.*`, `COPYING`, `COPYING.*` | Drops the file visibly — listed in `dodot status` as `skipped`                                              |
        | `external` | `External`   | `CodeExecution` | Precise    | Exclusive | `externals.toml`                                                                                                                                                                 | Fetches each declared resource, and re-fetches when its upstream signature moves                            |
        | `homebrew` | `Provision`  | `CodeExecution` | Precise    | Exclusive | `Brewfile`                                                                                                                                                                       | Runs `brew bundle` once, then holds an edited `Brewfile` at `older version` until `--provision-rerun`       |
        | `nix`      | `Provision`  | `CodeExecution` | Precise    | Exclusive | `packages.nix`                                                                                                                                                                   | Runs `nix profile install` once, then holds an edited manifest at `older version` until `--provision-rerun` |
        | `install`  | `Setup`      | `CodeExecution` | Precise    | Exclusive | `install.sh`, `install.bash`, `install.zsh`                                                                                                                                      | Runs the script once, then holds an edited script at `older version` until `--provision-rerun`              |
        | `path`     | `PathExport` | `Configuration` | Precise    | Exclusive | `bin/`                                                                                                                                                                           | Stages the directory onto `$PATH`                                                                           |
        | `shell`    | `ShellInit`  | `Configuration` | Precise    | Exclusive | `*.sh`, `*.bash`, `*.zsh`                                                                                                                                                        | Sources the file at shell startup                                                                           |
        | `symlink`  | `Link`       | `Configuration` | Catchall   | Exclusive | Anything no other handler claimed (catchall)                                                                                                                                     | Links the entry into `$HOME` or `$XDG_CONFIG_HOME`                                                          |

    :: table align=lllllll ::

2. Phases

    Phases run in the order below — the order the `ExecutionPhase` variants are declared in. A phase may hold more than one handler, and the order two handlers sharing a phase run in is not pinned down; nothing in dodot depends on it.

    Execution phases:

        | Phase        | Handlers                 | Category        |
        | `Filter`     | `gate`, `ignore`, `skip` | `Configuration` |
        | `External`   | `external`               | `CodeExecution` |
        | `Provision`  | `homebrew`, `nix`        | `CodeExecution` |
        | `Setup`      | `install`                | `CodeExecution` |
        | `PathExport` | `path`                   | `Configuration` |
        | `ShellInit`  | `shell`                  | `Configuration` |
        | `Link`       | `symlink`                | `Configuration` |

    :: table align=lll ::

3. Default Mappings

    The rules `[mappings]` produces with no user configuration. Higher priority wins: a `README.md` is claimed by `skip` at 50 rather than by `symlink` at 0. The handlers with no row here receive no default rule — `gate` matches are minted by the scanner, and `ignore` waits for a pattern you set.

    Default rules, highest priority first:

        | Priority | Handler    | Patterns                                                                                                                                                                         |
        | 50       | `skip`     | `README`, `README.*`, `LICENSE`, `LICENSE.*`, `CHANGELOG`, `CHANGELOG.*`, `CONTRIBUTING`, `CONTRIBUTING.*`, `AUTHORS`, `AUTHORS.*`, `NOTICE`, `NOTICE.*`, `COPYING`, `COPYING.*` |
        | 20       | `external` | `externals.toml`                                                                                                                                                                 |
        | 20       | `install`  | `install.sh`, `install.bash`, `install.zsh`                                                                                                                                      |
        | 10       | `homebrew` | `Brewfile`                                                                                                                                                                       |
        | 10       | `nix`      | `packages.nix`                                                                                                                                                                   |
        | 10       | `path`     | `bin/`                                                                                                                                                                           |
        | 10       | `shell`    | `*.sh`, `*.bash`, `*.zsh`                                                                                                                                                        |
        | 0        | `symlink`  | `*`                                                                                                                                                                              |

    :: table align=rll ::
