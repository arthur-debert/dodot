//! External-resource fetching.
//!
//! A pack opts into externally-sourced content by placing an
//! `externals.toml` at its root. Each `[entry]` block declares a single
//! resource (a remote file today, a git repo in a later PR) and the
//! target path under `$HOME` where it should appear.
//!
//! ```toml
//! [shared-aliases]
//! type = "file"
//! url = "https://example.com/aliases.sh"
//! target = "~/.config/shared/aliases.sh"
//! sha256 = "abc123..."
//! ```
//!
//! This module owns the spec types ([`ExternalsToml`], [`FetchSpec`])
//! and the [`HttpFetcher`] abstraction. The handler in
//! `crate::handlers::externals` consumes specs to produce intents; the
//! executor consumes intents and calls a `HttpFetcher` to do the
//! actual network work.

mod fetch;
mod spec;

pub use fetch::{HttpFetchError, HttpFetcher, UreqFetcher};
pub use spec::{parse_externals_toml, ExternalEntry, ExternalsToml, FetchSpec};
