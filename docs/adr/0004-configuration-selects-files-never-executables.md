# Configuration selects files, never executables

A pack's configuration may decide *which file* a handler acts on. It may never
decide *which program* runs, or with what arguments. `MappingsSection` resolves
through defaults, then the dotfiles root, then the pack, so anything held there
is overridable by a `.dodot.toml` inside a cloned repository. Filenames belong
in that layer and stay there at rule priority 10 or higher: claiming
`packages.nix`, `Brewfile`, or `setup.sh` is ordinary user configuration and
the whole point of a mapping. Executables and their argument shape belong in
compile-time data — `crates/dodot-lib/src/provisioners.rs` — where no
configuration path can reach them.

The distinction is what keeps cloning someone's dotfiles a bounded act. Running
a repository's `install.sh` is a decision the user makes knowingly, in the open,
about a file they can read. Having the repository quietly redirect the
`homebrew` handler at an executable of its choosing is not the same decision,
and no visible part of the pack would announce it.

This is a new boundary rather than a restatement. ADR-0001 through ADR-0003
bind trust to a dotfiles root's canonical path, and `docs/spec/safety-lock.md`
places content evaluation of install scripts, Brewfiles, and Nix manifests
explicitly out of scope. Safety Lock therefore governs *which root* dodot acts
on and never *what a pack may redefine* — a gap this rule closes.

A related inconsistency is settled here too, because two philosophies coexisting
unremarked is worse than two philosophies coexisting. Secret providers locate
their binaries by spawning a bare name against dodot's inherited `PATH`;
provisioners follow the Homebrew bootstrap's model of probing an ordered list of
absolute candidates. Both stay, and the difference is deliberate: a probe over
fixed candidates is what lets an absent manager become a clean skip that names
where it looked.

A descriptor field that would let configuration name an executable, an argument
template, or an environment variable for a spawn is out of bounds by this rule,
whatever else recommends it.

Reasoning: `docs/proposals/provisioning-handlers.lex` §3.3.
