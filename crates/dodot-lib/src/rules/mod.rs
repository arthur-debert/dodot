//! Rule types, pattern matching, and file scanning.
//!
//! A rule pairs a file pattern with a handler name. The [`Scanner`]
//! walks a pack directory and matches each file against the rule set.
//! Rules are checked in descending priority order; the first match
//! wins. Filter handlers (`ignore`, `skip`) sit at the highest priority
//! tier so a file the user wants dropped never gets claimed by a
//! precise mapping or the catchall.

mod grouping;
mod pattern;
mod scanner;
mod types;

pub use grouping::{group_by_handler, handler_execution_order};
pub use scanner::{should_skip_entry, Scanner, SPECIAL_FILES};
pub use types::{GateFailure, PackEntry, Rule, RuleMatch};
