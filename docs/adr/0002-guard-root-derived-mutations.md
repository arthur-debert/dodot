# Guard root-derived mutations, not every mutation

Safety Lock applies when a requested mutation changes repository, deployment,
or user-visible state whose target is selected from the resolved dotfiles root.
Deployment and repository-writing commands therefore require trust, while
incidental diagnostic logs and caches do not make a diagnostic command
root-sensitive. Unrelated mutations such as shell-rc wiring, prompt-state
changes, and factory-resetting the global data directory retain their existing
safeguards; this keeps accidental root selection behind one coherent gate
without turning Safety Lock into a general confirmation framework.

The initial protected set is mutating `up` and `down`; `init`, `fill`, `adopt`,
and `addignore`; configuration actions persisted into the selected root;
mutating `refresh` and `transform check`; repository-local
`git-install-filters`, `template install-filter`, and `transform install-hook`;
and the tutorial's real deployment step. Read-only and dry-run variants do not
require trust. Post-`up` repository installers reuse the root authorization of
the protected `up` that reached them.

Commands that take their write targets from the shared preprocessor baseline
cache rather than from the root — `refresh` and `transform check` — scope the
mutation set instead of the cache: they write only baselines whose canonical
source path lies inside the authorized root and report the rest as out-of-root.
Per-root cache and datastore namespaces stay out of scope, so this scoping is
what keeps the authorized root and the mutated root the same one.

Commands whose mutations are selected independently of the dotfiles root stay
outside Safety Lock: `install --write`, `git-install-alias`, dismissed-prompt
management, factory `reset`, and stdin/stdout filter passthroughs keep their
existing safeguards. New commands must declare root sensitivity at the central
CLI seam instead of inheriting it from which context-builder helper they use.
