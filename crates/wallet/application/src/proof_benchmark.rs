// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, future::Future, pin::Pin, time::Duration};

pub const PROOF_BENCHMARK_MIN_K: u8 = 1;
pub const PROOF_BENCHMARK_MAX_K: u8 = 21;
pub const PROOF_BENCHMARK_DEFAULT_MAX_K: u8 = 17;
pub const PROOF_BENCHMARK_HIGH_RESOURCE_K: u8 = 18;

pub type ProofBenchmarkFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProofBenchmarkReport, ProofBenchmarkError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunProofBenchmarkCommand {
    pub k: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofBenchmarkStage {
    Idle,
    Preparing,
    KeyGeneration,
    Proving,
    Verifying,
    Completed,
    Failed,
}

impl ProofBenchmarkStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Preparing => "preparing circuit and parameters",
            Self::KeyGeneration => "generating proving key",
            Self::Proving => "generating proof",
            Self::Verifying => "verifying proof",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofBenchmarkSnapshot {
    pub stage: ProofBenchmarkStage,
    pub active_k: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofBenchmarkRowCount {
    Measured(u64),
    Estimated(u64),
}

impl ProofBenchmarkRowCount {
    #[must_use]
    pub const fn value(self) -> u64 {
        match self {
            Self::Measured(value) | Self::Estimated(value) => value,
        }
    }

    #[must_use]
    pub const fn is_estimated(self) -> bool {
        matches!(self, Self::Estimated(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofBenchmarkVerification {
    Verified,
    Failed,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofBenchmarkReport {
    pub requested_k: u8,
    pub realized_k: u8,
    pub row_count: ProofBenchmarkRowCount,
    pub hash_chain_length: u32,
    pub key_generation: Duration,
    pub proving: Duration,
    pub verification: Option<Duration>,
    pub verification_result: ProofBenchmarkVerification,
    pub proof_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofBenchmarkError {
    InvalidCircuitSize,
    Busy,
    Unavailable,
    ResourceUnavailable,
    ProvingFailed,
    WorkerFailed,
}

impl fmt::Display for ProofBenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCircuitSize => "the requested circuit size is unsupported",
            Self::Busy => "another proof benchmark is still running",
            Self::Unavailable => "the development proof benchmark is unavailable",
            Self::ResourceUnavailable => "required proving resources are unavailable",
            Self::ProvingFailed => "the proof benchmark failed",
            Self::WorkerFailed => "the proof benchmark worker stopped unexpectedly",
        })
    }
}

impl Error for ProofBenchmarkError {}

/// Development-only incoming port for one synthetic proof measurement.
///
/// Implementations must reject invalid `k` values before allocation and retain
/// their process-wide admission slot until the worker has actually stopped.
pub trait RunProofBenchmarkUseCase: Send + Sync {
    fn snapshot(&self) -> ProofBenchmarkSnapshot;

    fn execute(&self, command: RunProofBenchmarkCommand) -> ProofBenchmarkFuture<'_>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_range_matches_the_owner_requested_mobile_envelope() {
        assert_eq!(PROOF_BENCHMARK_MIN_K, 1);
        assert_eq!(PROOF_BENCHMARK_DEFAULT_MAX_K, 17);
        assert_eq!(PROOF_BENCHMARK_HIGH_RESOURCE_K, 18);
        assert_eq!(PROOF_BENCHMARK_MAX_K, 21);
    }

    #[test]
    fn estimated_rows_are_never_presented_as_measured() {
        let measured = ProofBenchmarkRowCount::Measured(42);
        let estimated = ProofBenchmarkRowCount::Estimated(84);
        assert_eq!(measured.value(), 42);
        assert!(!measured.is_estimated());
        assert_eq!(estimated.value(), 84);
        assert!(estimated.is_estimated());
    }
}
