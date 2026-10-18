//! In-controller logic compilation and deployment pipeline (TSK-102 / SYS-REQ-021).

use crate::compiler::{CompilationArtifacts, CompilerConfig, StrucppCompiler};
use crate::error::StrucppError;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::info;

/// Request payload for in-controller logic deployment (`POST /api/v1/control/deploy`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    /// IEC 61131-3 Structured Text source code
    pub source_st: String,
    /// Target task scan interval in milliseconds
    pub interval_ms: Option<u64>,
    /// Task priority (optional, default 80)
    pub priority: Option<u32>,
    /// Core affinity (optional)
    pub core_affinity: Option<usize>,
}

/// Result of an in-controller deployment operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub success: bool,
    pub source_hash: String,
    pub shared_library_path: String,
    pub compilation_time_ms: u64,
}

/// Deployer service coordinating in-controller compilation and artifact delivery.
pub struct LogicDeployer {
    compiler: StrucppCompiler,
}

impl LogicDeployer {
    pub fn new(config: CompilerConfig) -> Self {
        Self {
            compiler: StrucppCompiler::new(config),
        }
    }

    /// Compile and prepare logic binary atomically.
    pub fn deploy_logic(
        &self,
        req: &DeployRequest,
    ) -> Result<(DeployResult, CompilationArtifacts), StrucppError> {
        let start = Instant::now();
        info!("Initiating in-controller logic deployment");

        let artifacts = self.compiler.compile_source(&req.source_st)?;
        let elapsed = start.elapsed().as_millis() as u64;

        let result = DeployResult {
            success: true,
            source_hash: artifacts.source_hash.clone(),
            shared_library_path: artifacts.shared_library.display().to_string(),
            compilation_time_ms: elapsed,
        };

        info!(
            "Deployment compilation complete in {} ms: hash={}",
            elapsed, result.source_hash
        );
        Ok((result, artifacts))
    }
}
