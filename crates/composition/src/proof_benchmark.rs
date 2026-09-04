// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, sync::Arc};

use oxid_adapter_midnight::{MidnightProofBenchmark, MidnightProofBenchmarkConfig};
use oxid_wallet_application::{ProofBenchmarkError, RunProofBenchmarkUseCase};

/// Composes the opt-in development proof benchmark against an app-private
/// parameter cache. Production profiles never call or compile this boundary.
pub fn compose_development_proof_benchmark(
    cache_directory: impl Into<PathBuf>,
) -> Result<Arc<dyn RunProofBenchmarkUseCase>, ProofBenchmarkError> {
    let config = MidnightProofBenchmarkConfig::new(cache_directory)?;
    Ok(Arc::new(MidnightProofBenchmark::new(config)))
}
