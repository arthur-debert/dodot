# Declare the manifest argument, never infer it positionally

A provisioning command's manifest — the user's `install.sh`, `Brewfile`, or
`packages.nix` — is named by the handler's descriptor, at a stated position in
the command's arguments. The datastore reads that position and nothing else.
Deducing the manifest from argument shape is not permitted, whatever the
heuristic.

`DataStore::run_and_record` needs the manifest for three things: the filename in
the run's progress header, the leading comment block it echoes before running,
and the `.snapshot` sibling it writes next to the sentinel so
`dodot status --diff` can show what changed since the run. It used to take the
command's last argument. `install` (`bash -- <script>`) and `homebrew`
(`brew bundle --file <Brewfile>`) satisfied that convention; `nix` did not,
because its command ended in `--extra-experimental-features "nix-command
flakes"`. Nix runs therefore snapshotted nothing and printed
`==== <pack> → nix → nix-command flakes` as their progress header, and
`status --diff` was permanently unavailable for `packages.nix`.

The failure is worth naming precisely, because it is the kind that repeats. Two
of three commands satisfying a rule is not a contract, and the third did not
break anything a compiler or a test could see — it produced a plausible string
and lost a feature quietly. A stated position fails loudly instead: the
descriptor row and the `command_for` that builds the arguments are pinned
against each other by `provisioners::tests::descriptor_matches_command`, so a
command that grows or reorders an argument without its row moving fails at
`cargo test` rather than at a user's next `status --diff`.

The declaration also constrains the commands, and that is a feature. `nix` now
passes the manifest as `--argstr manifest <path>`, a real argument the wrapper
expression reads as a Nix function parameter, rather than interpolating the path
into the expression's text. The command names its own manifest, which is what
the descriptor can point at — and nothing has to escape a path into Nix source,
so a manifest path containing a quote, a backslash, or a `${` is just an
argument. A provisioner whose command cannot name its manifest is a provisioner
dodot cannot record honestly.

The registry lives in compile-time data and never in `MappingsSection` — see
ADR-0004.

Verified against real Nix, because dodot's development machines have none and
every prior claim about this path was documentation-derived. Image
`nixos/nix:latest`, digest
`sha256:7a007c766426c1877758ddc5cb87a965ac131fc78c582ce0083d922d51ae945c`,
Nix 2.35.2, running a statically linked `dodot` built in `rust:alpine`:

```sh
docker run --rm -v /tmp/nixws:/work nixos/nix:latest bash -c '
  export HOME=/root DOTFILES_ROOT=/root/dotfiles \
         XDG_DATA_HOME=/root/.local/share
  mkdir -p "$DOTFILES_ROOT/tools"
  printf %s "{ pkgs ? import <nixpkgs> {} }: with pkgs; [ hello ]" \
    > "$DOTFILES_ROOT/tools/packages.nix"
  /work/dodot up
  cat "$XDG_DATA_HOME"/dodot/packs/tools/nix/*.snapshot'
```

The run printed `==== tools → nix → packages.nix`, installed `hello` into the
profile, and wrote `packages.nix-f95610d56c3bf8f2.snapshot` holding the
manifest's bytes; editing the manifest and running `dodot status tools --diff`
then produced the unified diff. The same container also established that
`nix profile install --expr` evaluates in pure mode, which rejects both the
manifest path and `<nixpkgs>` — hence the `--impure` on every invocation.

Reasoning: `docs/proposals/provisioning-handlers.lex` §3.1.
