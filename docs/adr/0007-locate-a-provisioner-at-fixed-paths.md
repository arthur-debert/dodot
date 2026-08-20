# Locate a provisioner at fixed absolute paths

dodot finds a package manager by testing an ordered list of fixed absolute
candidate paths for a regular file carrying an execute bit, and runs the path
that answered. It does not search `PATH`, and it does not spawn a bare name and
see what happens. The candidate lists live in the compile-time descriptor
registry (`crates/dodot-lib/src/provisioners/`), where ADR-0004 requires
anything naming an executable to live.

The reason is not that `PATH` is unreliable — it is that a bare-name spawn
cannot answer the question a provisioner has to ask. `brew bundle` against a
host without brew is indistinguishable, from dodot's side, from a `Brewfile`
that failed: both arrive as a non-zero exit from a subprocess dodot decided to
start. Probing first turns "you don't have Homebrew" into a skip that names the
manager and the locations tried, before anything runs. A row that says
*homebrew not installed — probed /opt/homebrew/bin/brew, …* distinguishes "not
installed" from "installed somewhere dodot does not look"; a failed spawn
distinguishes nothing.

The list is also the same list the Homebrew shell bootstrap already probes.
`shell/homebrew.rs` has resolved brew this way since it was written, and a brew
that bootstrap emits a block for must be a brew a `Brewfile` runs against — so
the provisioner candidates are the bootstrap's prefixes with `bin/brew`
appended, pinned against them by a test rather than by intention. Nix has no
`$NIX_PREFIX` equivalent, so its candidates are enumerated outright and verified
rather than assumed. In `nixos/nix:latest` (Nix 2.35.2):

```sh
docker run --rm nixos/nix:latest sh -c '
  ls -l /nix/var/nix/profiles/default/bin/nix
  ls -ld ~/.nix-profile/bin/nix
  ls -l /run/current-system/sw/bin/nix
  command -v nix'
```

The image holds `/nix/var/nix/profiles/default/bin/nix` and a `~/.nix-profile`
symlink onto that same profile — `command -v nix` resolves to the latter, which
is why the per-user profile leads the list — and holds neither
`~/.local/state/nix/profiles/profile/bin/nix` (the Nix ≥ 2.14 profile location
`~/.nix-profile` normally points at) nor the NixOS `/run/current-system/sw/bin`.
Both are in the list because a container is not the only shape a Nix install
takes, and neither is in it on the strength of a doc page alone.

**`install` is the exception, and it uses `PATH`.** `install.sh` runs under the
bare `bash` or `zsh` its extension selects, spawned through `PATH` with no
pre-flight probe. The trust argument for fixed candidates is about a *pack*
choosing which executable runs — the argument ADR-0004 makes — and a pack cannot
choose your shell. Shells legitimately live in `/bin`, `/usr/bin`,
`/opt/homebrew/bin`, and nix profiles; dodot enumerating that list would refuse
to run scripts on hosts where the interpreter is sitting right there, and would
be wrong more often than the manager probe is right. The consequence is real and
accepted: an `install.zsh` on a host without zsh fails at spawn time the way it
always has, rather than skipping cleanly the way an absent brew now does.

The inconsistency with secret providers — which locate their binaries by
spawning a bare name against dodot's inherited `PATH` — stays, as ADR-0004
records, and is not debt with an owner. The two differ because their failure
modes differ: a secret provider that is missing means a template cannot render,
which is a hard error the user must fix, while a missing package manager means a
file does not run, which is a skip the user may not care about. If a secret
provider ever needs absence-as-skip, it gets a probe; until then there is
nothing to repay.

Reasoning: `docs/proposals/provisioning-handlers.lex` §3.3, §4.1.
