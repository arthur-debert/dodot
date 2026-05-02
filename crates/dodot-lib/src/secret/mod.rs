//! Secret handling — provider trait, value wrapper, and the scheme
//! registry that maps `secret(...)` references to providers.
//!
//! This module is the rust-native side of the design in
//! `docs/proposals/secrets.lex` and the testing seam described in
//! `docs/proposals/secrets-testing.lex` §2. Concrete providers live in
//! sibling files (`pass.rs`, `op.rs`, etc.); this module owns the
//! plumbing that's the same for all of them.
//!
//! The trait deliberately stays small. Providers do three things:
//! parse a reference, talk to their tool, return a `SecretString`.
//! Cross-cutting concerns — caching within a `dodot up` run, batched
//! provider invocations, mode gating per `secrets.lex` §7.4 — live
//! above the trait, in the `secret()` MiniJinja function.

pub mod op;
pub mod pass;
pub mod provider;
pub mod registry;
pub mod secret_string;

#[cfg(test)]
pub mod test_support;

pub use op::OpProvider;
pub use pass::PassProvider;
pub use provider::{ProbeResult, SecretProvider};
pub use registry::{split_scheme, SecretRegistry};
pub use secret_string::SecretString;
