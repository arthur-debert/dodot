//! Safety Lock: an implicitly discovered dotfiles root must be approved
//! before it can drive a root-sensitive mutation.
//!
//! Dodot infers a dotfiles root from the enclosing Git repository, then from
//! the current directory. That convenience is also a hazard: an unrelated
//! repository looks enough like a dotfiles repository to deploy from, and a
//! wrong-root `up` can add shell files to every future session while a
//! wrong-root `down` can remove legitimate state. Safety Lock makes root
//! intent explicit without removing the convenience — read-only commands and
//! dry runs stay available on any root, and a valid `DOTFILES_ROOT` remains
//! the deliberate, noninteractive selection path.
//!
//! Governing documents: `docs/spec/safety-lock.md`,
//! `docs/adr/0001-trust-dotfiles-roots-by-canonical-path.md`,
//! `docs/adr/0002-guard-root-derived-mutations.md`, and
//! `docs/adr/0003-inspect-and-revoke-roots-without-a-trust-command.md`.
//!
//! # The shape
//!
//! ```text
//! DOTFILES_ROOT ──► environment ─┐
//!                                ├─► selection ─► ResolvedRoot ─► check ─► TrustDecision
//! git top-level / cwd ──► files ─┘                     │                        │
//!                                                      │           operation ─► authorize ─► GateOutcome
//!                            schema (approved roots, one file)                       │
//!                                                      │                             ├─ NotRootSensitive
//!                                                 list / forget                      ├─ Permitted
//!                                                                                    └─ ConfirmationRequired
//!                                                                                         └─ inventory (prompt)
//! ```
//!
//! [`resolve_root`] is the invocation's one act of selection: `DOTFILES_ROOT`,
//! then the Git top-level, then the current directory — and nothing after
//! that. Every consumer carries the [`ResolvedRoot`] it returns rather than
//! resolving again, which is what keeps the root the user approved and the
//! root Dodot mutates the same one (ADR-0001).
//!
//! Both selection paths return the same [`ResolvedRoot`]: one canonical
//! [`RootIdentity`] plus the [`RootSource`] that chose it. Provenance changes
//! authorization policy — an environment root is already deliberate, an
//! implicit one needs approval — never the path's representation. No
//! source-specific type exists past [`roots`]; checking, listing, inventory,
//! and mutation scoping all take a `ResolvedRoot` or a `RootIdentity`.
//!
//! Approved implicit roots live together in one clapfig-managed file under
//! Dodot's data directory ([`schema`]), never in the dotfiles repository.
//! Environment roots are never written there.
//!
//! [`authorize`] is the seam a command crosses. [`decide`] answers only what
//! the trust collection can answer — what standing a root has — and a root's
//! standing only matters once you know what the command is about to do:
//! `status` and `up --dry-run` read the same untrusted root that `up` may not
//! write to. [`RootOperation`] carries that half, declared per command rather
//! than inferred from which context builder it used (ADR-0002), and the two
//! compose into one [`GateOutcome`]. An unapproved implicit root gets the
//! orientation [`inventory`] built and carried out with the answer, so no
//! approvable outcome can exist for a root Dodot could not describe.
//!
//! # Boundaries
//!
//! This module is CLI-free by construction: no Clap and no Standout types
//! appear in any signature, and nothing here reads the process environment,
//! the current directory, or Git. Those are captured at the process boundary
//! and injected ([`RootSelectionInput`], [`PathProbe`],
//! [`HostFacts`](crate::gates::HostFacts), [`Fs`](crate::fs::Fs)). Likewise,
//! persistence is the caller's: the checking, listing, and forgetting APIs
//! take an already-loaded [`SafetyLockConfig`] and return typed state changes
//! to write.
//!
//! Reading the *root* is different from reading process state, and the
//! inventory does it: pack discovery and rule matching need the directory. It
//! stops at routing metadata — configuration and which handler claims which
//! entry — and never renders, resolves, or reads a candidate file's contents
//! (Spec, "Non-Goals"). The injected [`Fs`](crate::fs::Fs) is not a full
//! virtualization of that read: configuration goes through
//! [`ConfigManager`](crate::config::ConfigManager), which uses `std::fs`. See
//! [`build_inventory`](inventory::build_inventory).
//!
//! # Where the boundary is
//!
//! The capture this module refuses to perform happens in exactly one place:
//! the CLI's `safety` module, whose Standout pre-dispatch hook reads
//! `DOTFILES_ROOT`, the current directory, and the Git top-level, calls
//! [`resolve_root`] and [`authorize`], and hands the result to the command as
//! an immutable value. Handlers consume that value; nothing downstream reads
//! process state again.
//!
//! Two surfaces are still outside the gate: the CLI's passthrough commands,
//! which return before Standout dispatches and so have no hook (`config`'s
//! root-persisting actions are the one that matters), and the tutorial's real
//! deployment step. Both resolve their root through [`resolve_root`] like
//! everything else — what they lack is the [`authorize`] call. ACC01-WS08
//! closes that.

pub mod check;
pub mod environment;
pub mod error;
pub mod files;
pub mod forget;
pub mod inventory;
pub mod list;
pub mod operation;
pub mod roots;
pub mod schema;
pub mod scope;
pub mod selection;
#[cfg(test)]
mod test_probe;
pub mod util;

pub use check::{approve, decide, ApprovalChange, TrustDecision};
pub use environment::{resolve_environment_root, EnvironmentRootInput};
pub use error::{Result as SafetyLockResult, SafetyLockError};
pub use files::{resolve_file_root, FileRootInput};
pub use forget::{forget_root, ForgetChange, ForgetRequest};
pub use inventory::{
    build_inventory, CategoryCount, InventoryCategory, InventoryEntry, RootInventory,
    MAX_INVENTORY_PATHS,
};
pub use list::{list_roots, TrustedRootEntry, TrustedRootListing};
pub use operation::{authorize, GateOutcome, RootOperation};
pub use roots::{ResolvedRoot, RootIdentity, RootSource};
pub use schema::{
    SafetyLockConfig, TrustedRootsSection, SAFETY_LOCK_FILE_NAME, SAFETY_LOCK_PERSIST_SCOPE,
};
pub use scope::{scope_to_root, MutationScope, OutOfRootReason, OutOfRootTarget, ScopeOutcome};
pub use selection::{resolve_root, RootSelectionInput};
pub use util::{decode_native_path, encode_native_path, OsPathProbe, PathProbe, NATIVE_BYTES_TAG};
