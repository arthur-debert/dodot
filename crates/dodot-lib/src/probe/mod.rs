//! Probe — introspection for the deployed state.
//!
//! Today this module provides two read-only views over `<data_dir>`:
//!
//! - [`deployment_map`] — the `pack / handler / source / deployed` map
//!   that `dodot refresh` (see `docs/proposals/magic.lex`) also
//!   consumes. Written alongside the shell init script on every `up`
//!   and `down`.
//! - [`data_dir_tree`] — a bounded-depth tree walk for `dodot probe
//!   show-data-dir`.
//!
//! See `docs/proposals/profiling.lex` for the full feature spec. A
//! later phase will add shell-init timing reports under
//! `<data_dir>/probes/shell-init/`; that state lives in a sibling
//! submodule when it lands.

pub mod data_dir_tree;
pub mod deployment_map;
pub mod last_up;
pub mod shell_init;

pub use data_dir_tree::{collect_data_dir_tree, TreeNode};
pub use deployment_map::{
    collect_deployment_map, read_deployment_map, write_deployment_map, DeploymentKind,
    DeploymentMapEntry,
};
pub use last_up::{read_last_up_marker, write_last_up_marker};
pub use shell_init::{
    aggregate_profiles, group_profile, parse_profile, parse_unix_ts_from_filename,
    read_latest_profile, read_recent_profiles, rotate_profiles, summarize_history,
    AggregatedTarget, AggregatedView, GroupedProfile, HistoryEntry, Profile, ProfileEntry,
    ProfileGroup,
};
