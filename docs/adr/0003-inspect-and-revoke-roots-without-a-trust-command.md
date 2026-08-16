# Inspect and revoke roots without a trust command

Trusted-root management is exposed as `dodot roots list` and `dodot roots
forget <path>`. Safety Lock deliberately has no standalone trust verb: users
approve an implicit root only through the root-sensitive command's warning and
confirmation, while automation expresses deliberate selection with
`DOTFILES_ROOT`; this keeps approval inspectable and reversible without
creating a second path around the safety gate or conflating trust with the
existing dismissed-prompt registry.

Revocation remains possible after filesystem changes. `roots forget` first
canonicalizes an existing path so aliases select the same approval; when the
path no longer exists, it matches the exact stored absolute path shown by
`roots list`. Moving or deleting a root therefore cannot strand an approval
record.
