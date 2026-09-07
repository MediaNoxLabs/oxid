// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, sync::Arc};

use oxid_adapter_midnight::{MidnightProofBenchmark, MidnightProofBenchmarkConfig};
#[cfg(not(target_os = "ios"))]
use oxid_adapter_platform_system::SystemProcessResourceSampler;
use oxid_platform_ports::{ProcessResourceSamplerPort, UnavailableProcessResourceSampler};
use oxid_wallet_application::{ProofBenchmarkError, RunProofBenchmarkUseCase};

/// Composes the opt-in development proof benchmark against an app-private
/// parameter cache. Production profiles never call or compile this boundary.
pub fn compose_development_proof_benchmark(
    cache_directory: impl Into<PathBuf>,
) -> Result<Arc<dyn RunProofBenchmarkUseCase>, ProofBenchmarkError> {
    let config = MidnightProofBenchmarkConfig::new(cache_directory)?;
    Ok(Arc::new(MidnightProofBenchmark::new(config)))
}

/// Composes the current-process sampler beside the development benchmark.
/// Unsupported native targets receive an explicit fail-closed adapter.
#[must_use]
pub fn compose_development_process_resource_sampler() -> Arc<dyn ProcessResourceSamplerPort> {
    #[cfg(not(target_os = "ios"))]
    if let Ok(sampler) = SystemProcessResourceSampler::new() {
        return Arc::new(sampler);
    }
    Arc::new(UnavailableProcessResourceSampler)
}
