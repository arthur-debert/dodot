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
//!                                ├─► ResolvedRoot ──► check ──► TrustDecision
//! git top-level / cwd ──► files ─┘        │                          │
//!                                         │                          ├─ ExplicitlySelected
//!                       schema (approved roots, one file)            ├─ AlreadyApproved
//!                                         │                          └─ ApprovalRequired ──► inventory
//!                                    list / forget                                              (prompt)
//! ```
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
//! # Boundaries
//!
//! This module is CLI-free by construction: no Clap and no Standout types
//! appear in any signature, and nothing here reads the process environment,
//! the current directory, or Git. Those are captured at the process boundary
//! and injected ([`EnvironmentRootInput`], [`FileRootInput`],
//! [`PathProbe`]). Likewise, persistence is the caller's: the checking,
//! listing, and forgetting APIs take an already-loaded [`SafetyLockConfig`]
//! and return typed state changes to write.
//!
//! # Work Stream status
//!
//! ACC01-WS01 establishes this vocabulary and the persisted schema; ACC01-WS02
//! fills in [`environment`], the authoritative `DOTFILES_ROOT` path. The
//! remaining trust-lifecycle, decision, inventory, and scoping entry points
//! carry documented signatures with `todo!()` bodies; each names the Work
//! Stream that fills it in. The data model, the schema, and environment
//! selection are complete and tested.
//!
//! Nothing in this module is wired into a command yet: the process boundary
//! that captures the environment, the current directory, and Git — and the
//! gate that consults all of this — arrives with WS07.

pub mod check;
pub mod environment;
pub mod error;
pub mod files;
pub mod forget;
pub mod inventory;
pub mod list;
pub mod roots;
pub mod schema;
pub mod scope;
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
pub use roots::{ResolvedRoot, RootIdentity, RootSource};
pub use schema::{
    SafetyLockConfig, TrustedRootsSection, SAFETY_LOCK_FILE_NAME, SAFETY_LOCK_PERSIST_SCOPE,
};
pub use scope::{scope_to_root, MutationScope, OutOfRootReason, OutOfRootTarget};
pub use util::{decode_native_path, encode_native_path, OsPathProbe, PathProbe, NATIVE_BYTES_TAG};
