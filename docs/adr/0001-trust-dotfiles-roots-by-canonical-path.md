# Trust dotfiles roots by canonical path

Safety Lock binds a user's approval to the dotfiles root's canonical absolute
path, not to filesystem identifiers or repository fingerprints. This matches
the accidental-wrong-directory threat model, stays portable, and avoids
surprising re-prompts after a restore or re-clone; moving the root to a new
canonical path requires approval again, while replacing its contents at the
same path deliberately retains trust because Safety Lock is not a security
boundary.

Root selection is also stable within one invocation. Dodot resolves and
canonicalizes the root once, records its provenance, and passes that same value
through trust lookup, confirmation, and execution instead of consulting the
environment, Git, cwd, or symlinks again. This does not lock or snapshot the
repository: content changes at the same canonical path remain within the
chosen path-based trust model, and ordinary command validation handles a root
that disappears or becomes unreadable.
