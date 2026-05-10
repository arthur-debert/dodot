//! Result types and the per-pack [`Command`] trait.
//!
//! These are the two halves of the orchestration contract: every
//! command is a `Command`, and every pack-level invocation returns a
//! [`PackResult`] aggregated into [`ExecuteResult`] by the pipeline.

use serde::Serialize;

use crate::operations::OperationResult;
use crate::packs::context::ExecutionContext;
use crate::packs::Pack;
use crate::Result;

/// Result for a single pack.
#[derive(Debug, Serialize)]
pub struct PackResult {
    pub pack_name: String,
    pub success: bool,
    pub operations: Vec<OperationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregated result across all packs.
#[derive(Debug, Serialize)]
pub struct ExecuteResult {
    pub pack_results: Vec<PackResult>,
    pub total_packs: usize,
    pub successful_packs: usize,
    pub failed_packs: usize,
}

impl ExecuteResult {
    pub fn is_success(&self) -> bool {
        self.failed_packs == 0
    }
}

/// A command that operates on a single pack.
///
/// The orchestration pipeline calls `execute_for_pack` for each
/// discovered pack. Commands implement the specific logic (up, down,
/// status, etc.) while the pipeline handles discovery, config loading,
/// filtering, and aggregation.
pub trait Command: Send + Sync {
    fn name(&self) -> &str;

    fn execute_for_pack(&self, pack: &Pack, ctx: &ExecutionContext) -> Result<PackResult>;
}
