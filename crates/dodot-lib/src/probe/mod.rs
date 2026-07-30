//! Probe — introspection for the deployed state.
//!
//! Read-only views over `<data_dir>`, plus the markers `dodot up`
//! writes there ([`cfprefsd_marker`], [`last_up`]):
//!
//! - [`deployment_map`] — the `pack / handler / source / deployed` map
//!   that `dodot refresh` (see `docs/proposals/magic.lex`) also
//!   consumes. Written alongside the shell init script on every `up`
//!   and `down`.
//! - [`data_dir_tree`] — a bounded-depth tree walk for `dodot probe
//!   show-data-dir`.
//! - [`shell_init`] — shell-startup timing profiles under
//!   `<data_dir>/probes/shell-init/`.
//!
//! See `docs/proposals/profiling.lex` for the full feature spec.

pub mod brew;
pub mod cfprefsd_marker;
pub mod data_dir_tree;
pub mod deployment_map;
pub mod last_up;
pub mod macos_native;
pub mod shell_init;

pub use cfprefsd_marker::{
    cfprefsd_marker_exists, cfprefsd_marker_path, clear_cfprefsd_marker, write_cfprefsd_marker,
};
pub use data_dir_tree::{collect_data_dir_tree, TreeNode};
pub use deployment_map::{
    collect_deployment_map, read_deployment_map, write_deployment_map, DeploymentKind,
    DeploymentMapEntry,
};
pub use last_up::{read_last_up_marker, write_last_up_marker};
pub use shell_init::{
    aggregate_profiles, group_profile, parse_profile, read_latest_profile, read_recent_profiles,
    rotate_profiles, summarize_history, AggregatedTarget, AggregatedView, GroupedProfile,
    HistoryEntry, Profile, ProfileEntry, ProfileGroup,
};
// Internal helper, exposed only within dodot-lib so `commands::probe`
// can compute staleness without re-implementing filename parsing.
pub(crate) use shell_init::parse_unix_ts_from_filename;
