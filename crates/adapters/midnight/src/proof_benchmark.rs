// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Cow,
    collections::HashMap,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use futures::channel::oneshot;
use midnight_base_crypto::data_provider::{FetchMode, MidnightDataProvider, OutputMode};
use midnight_serialize::tagged_serialize;
use midnight_transient_crypto::{
    curve::Fr,
    proofs::{
        KeyLocation, PARAMS_VERIFIER, ProofPreimage, ProverKey, ProvingKeyMaterial,
        Resolver as ResolverPort, VerifierKey, Zkir as _,
    },
};
use midnight_zkir::IrSource;
use midnight_zswap::{ZSWAP_EXPECTED_FILES, prove::ZswapResolver};
use oxid_wallet_application::{
    PROOF_BENCHMARK_MAX_K, PROOF_BENCHMARK_MIN_K, ProofBenchmarkError, ProofBenchmarkFuture,
    ProofBenchmarkReport, ProofBenchmarkRowCount, ProofBenchmarkSnapshot, ProofBenchmarkStage,
    ProofBenchmarkVerification, RunProofBenchmarkCommand, RunProofBenchmarkUseCase,
};
use rand::rngs::OsRng;
use reqwest::Url;

const PARAMETER_SOURCE: &str = "https://srs.midnight.network/";
const MAX_EXACT_MODEL_K: u8 = 17;
const MAX_VERIFIABLE_K: u8 = 14;
const BENCHMARK_KEY_LOCATION: &str = "oxid-development-proof-benchmark-v1";

// Values measured by the reviewed midnight-ledger mobile prototype. High-k
// entries avoid `IrSource::model`, whose development cost model can itself
// exhaust mobile memory before the real proving path begins.
const HASHES_FOR_K: [u32; (PROOF_BENCHMARK_MAX_K as usize) + 1] = [
    0, 0, 1, 1, 1, 1, 2, 3, 6, 12, 24, 49, 98, 195, 390, 780, 1_560, 3_121, 6_242, 12_484, 24_967,
    49_935,
];

static IR_CACHE: OnceLock<Mutex<HashMap<u8, (IrSource, u32)>>> = OnceLock::new();
static BENCHMARK_BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightProofBenchmarkConfig {
    cache_directory: PathBuf,
}

impl MidnightProofBenchmarkConfig {
    pub fn new(cache_directory: impl Into<PathBuf>) -> Result<Self, ProofBenchmarkError> {
        let cache_directory = cache_directory.into();
        if !cache_directory.is_absolute()
            || cache_directory
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(ProofBenchmarkError::Unavailable);
        }
        Ok(Self { cache_directory })
    }
}

struct BenchmarkState {
    snapshot: Mutex<ProofBenchmarkSnapshot>,
}

impl Default for BenchmarkState {
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(ProofBenchmarkSnapshot {
                stage: ProofBenchmarkStage::Idle,
                active_k: None,
            }),
        }
    }
}

pub struct MidnightProofBenchmark {
    config: MidnightProofBenchmarkConfig,
    state: Arc<BenchmarkState>,
}

impl MidnightProofBenchmark {
    #[must_use]
    pub fn new(config: MidnightProofBenchmarkConfig) -> Self {
        Self {
            config,
            state: Arc::new(BenchmarkState::default()),
        }
    }
}

impl RunProofBenchmarkUseCase for MidnightProofBenchmark {
    fn snapshot(&self) -> ProofBenchmarkSnapshot {
        self.state.snapshot.lock().map_or(
            ProofBenchmarkSnapshot {
                stage: ProofBenchmarkStage::Failed,
                active_k: None,
            },
            |snapshot| *snapshot,
        )
    }

    fn execute(&self, command: RunProofBenchmarkCommand) -> ProofBenchmarkFuture<'_> {
        Box::pin(async move {
            validate_k(command.k)?;
            let admission = BenchmarkAdmission::reserve(Arc::clone(&self.state), command.k)?;
            let config = self.config.clone();
            let state = Arc::clone(&self.state);
            let (sender, receiver) = oneshot::channel();
            let worker = std::thread::Builder::new()
                .name("oxid-proof-benchmark".to_owned())
                .spawn(move || {
                    let _admission = admission;
                    let result = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| ProofBenchmarkError::WorkerFailed)
                        .and_then(|runtime| {
                            runtime.block_on(run_benchmark(config, state, command.k))
                        });
                    let _ = sender.send(result);
                });
            if worker.is_err() {
                set_stage(&self.state, ProofBenchmarkStage::Failed, Some(command.k));
                return Err(ProofBenchmarkError::WorkerFailed);
            }
            receiver
                .await
                .map_err(|_| ProofBenchmarkError::WorkerFailed)?
        })
    }
}

struct BenchmarkAdmission;

impl BenchmarkAdmission {
    fn reserve(state: Arc<BenchmarkState>, k: u8) -> Result<Self, ProofBenchmarkError> {
        BENCHMARK_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ProofBenchmarkError::Busy)?;
        set_stage(&state, ProofBenchmarkStage::Preparing, Some(k));
        Ok(Self)
    }
}

impl Drop for BenchmarkAdmission {
    fn drop(&mut self) {
        BENCHMARK_BUSY.store(false, Ordering::Release);
    }
}

fn set_stage(state: &BenchmarkState, stage: ProofBenchmarkStage, active_k: Option<u8>) {
    if let Ok(mut snapshot) = state.snapshot.lock() {
        *snapshot = ProofBenchmarkSnapshot { stage, active_k };
    }
}

fn validate_k(k: u8) -> Result<(), ProofBenchmarkError> {
    if (PROOF_BENCHMARK_MIN_K..=PROOF_BENCHMARK_MAX_K).contains(&k) {
        Ok(())
    } else {
        Err(ProofBenchmarkError::InvalidCircuitSize)
    }
}

async fn run_benchmark(
    config: MidnightProofBenchmarkConfig,
    state: Arc<BenchmarkState>,
    k: u8,
) -> Result<ProofBenchmarkReport, ProofBenchmarkError> {
    let result = run_benchmark_inner(&config, &state, k).await;
    set_stage(
        &state,
        if result.is_ok() {
            ProofBenchmarkStage::Completed
        } else {
            ProofBenchmarkStage::Failed
        },
        Some(k),
    );
    result
}

async fn run_benchmark_inner(
    config: &MidnightProofBenchmarkConfig,
    state: &BenchmarkState,
    k: u8,
) -> Result<ProofBenchmarkReport, ProofBenchmarkError> {
    ensure_private_cache(&config.cache_directory)?;
    let (ir, hash_chain_length) = build_ir_for_k(k)?;
    let (realized_k, row_count) = if k <= MAX_EXACT_MODEL_K {
        let model = ir.model();
        (
            model.k(),
            ProofBenchmarkRowCount::Measured(
                u64::try_from(model.rows()).map_err(|_| ProofBenchmarkError::ProvingFailed)?,
            ),
        )
    } else {
        (
            k,
            ProofBenchmarkRowCount::Estimated(
                u64::from(hash_chain_length)
                    .saturating_mul(5)
                    .saturating_add(64),
            ),
        )
    };

    let source = Url::parse(PARAMETER_SOURCE).map_err(|_| ProofBenchmarkError::Unavailable)?;
    let provider = MidnightDataProvider {
        fetch_mode: FetchMode::OnDemand,
        base_url: source,
        output_mode: OutputMode::Log,
        expected_data: ZSWAP_EXPECTED_FILES.to_vec(),
        dir: config.cache_directory.clone(),
    };
    let params = Arc::new(ZswapResolver(provider));

    set_stage(state, ProofBenchmarkStage::KeyGeneration, Some(k));
    let keygen_started = Instant::now();
    let (prover_key, verifier_key) = ir
        .keygen(params.as_ref())
        .await
        .map_err(|_| ProofBenchmarkError::ResourceUnavailable)?;
    let key_generation = keygen_started.elapsed();
    let resolver = BenchmarkResolver {
        prover_key,
        verifier_key: verifier_key.clone(),
        ir,
    };

    set_stage(state, ProofBenchmarkStage::Proving, Some(k));
    let preimage = benchmark_preimage();
    let binding_input = preimage.binding_input;
    let proving_started = Instant::now();
    let (proof, skips) = preimage
        .prove::<IrSource>(OsRng, params.as_ref(), &resolver)
        .await
        .map_err(|_| ProofBenchmarkError::ProvingFailed)?;
    if skips.iter().any(Option::is_some) {
        return Err(ProofBenchmarkError::ProvingFailed);
    }
    let proving = proving_started.elapsed();
    let mut encoded_proof = Vec::new();
    tagged_serialize(&proof, &mut encoded_proof).map_err(|_| ProofBenchmarkError::ProvingFailed)?;

    let (verification, verification_result) = if k <= MAX_VERIFIABLE_K {
        set_stage(state, ProofBenchmarkStage::Verifying, Some(k));
        let started = Instant::now();
        let result = if verifier_key
            .verify(&PARAMS_VERIFIER, &proof, std::iter::once(binding_input))
            .is_ok()
        {
            ProofBenchmarkVerification::Verified
        } else {
            ProofBenchmarkVerification::Failed
        };
        (Some(started.elapsed()), result)
    } else {
        (None, ProofBenchmarkVerification::Skipped)
    };

    Ok(ProofBenchmarkReport {
        requested_k: k,
        realized_k,
        row_count,
        hash_chain_length,
        key_generation,
        proving,
        verification,
        verification_result,
        proof_bytes: encoded_proof.len(),
    })
}

fn ensure_private_cache(path: &Path) -> Result<(), ProofBenchmarkError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path).map_err(|_| ProofBenchmarkError::Unavailable)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ProofBenchmarkError::Unavailable);
        }
    } else {
        fs::create_dir_all(path).map_err(|_| ProofBenchmarkError::Unavailable)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| ProofBenchmarkError::Unavailable)?;
    }
    Ok(())
}

fn build_ir_for_k(k: u8) -> Result<(IrSource, u32), ProofBenchmarkError> {
    validate_k(k)?;
    if let Some(cached) = IR_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|cache| cache.get(&k).cloned())
    {
        return Ok(cached);
    }
    let hash_count = HASHES_FOR_K[usize::from(k)];
    let ir = if k == PROOF_BENCHMARK_MIN_K {
        IrSource::load(
            &br#"{"version":{"major":2,"minor":0},"num_inputs":1,"do_communications_commitment":false,"instructions":[{"op":"assert","cond":0}]}"#[..],
        )
    } else {
        build_hash_chain_ir(hash_count)
    }
    .map_err(|_| ProofBenchmarkError::ProvingFailed)?;
    let result = (ir, hash_count);
    if let Ok(mut cache) = IR_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() {
        cache.insert(k, result.clone());
    }
    Ok(result)
}

fn build_hash_chain_ir(hash_count: u32) -> io::Result<IrSource> {
    let mut instructions = String::with_capacity(64 * (hash_count as usize + 1));
    instructions.push_str(r#"{"op":"assert","cond":0}"#);
    let mut previous = 0_u32;
    for output in 1..=hash_count {
        instructions.push(',');
        use std::fmt::Write as _;
        write!(
            instructions,
            r#"{{"op":"transient_hash","inputs":[{previous}]}}"#
        )
        .map_err(io::Error::other)?;
        previous = output;
    }
    let json = format!(
        r#"{{"version":{{"major":2,"minor":0}},"num_inputs":1,"do_communications_commitment":false,"instructions":[{instructions}]}}"#
    );
    IrSource::load(json.as_bytes())
}

struct BenchmarkResolver {
    prover_key: ProverKey<IrSource>,
    verifier_key: VerifierKey,
    ir: IrSource,
}

impl ResolverPort for BenchmarkResolver {
    async fn resolve_key(&self, _: KeyLocation) -> io::Result<Option<ProvingKeyMaterial>> {
        let mut prover_key = Vec::new();
        tagged_serialize(&self.prover_key, &mut prover_key)?;
        let mut verifier_key = Vec::new();
        tagged_serialize(&self.verifier_key, &mut verifier_key)?;
        let mut ir_source = Vec::new();
        tagged_serialize(&self.ir, &mut ir_source)?;
        Ok(Some(ProvingKeyMaterial {
            prover_key,
            verifier_key,
            ir_source,
        }))
    }
}

fn benchmark_preimage() -> ProofPreimage {
    ProofPreimage {
        inputs: vec![Fr::from(1_u64)],
        private_transcript: Vec::new(),
        public_transcript_inputs: Vec::new(),
        public_transcript_outputs: Vec::new(),
        binding_input: Fr::from(42_u64),
        communications_commitment: None,
        key_location: KeyLocation(Cow::Borrowed(BENCHMARK_KEY_LOCATION)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_k_before_admission() {
        assert_eq!(validate_k(0), Err(ProofBenchmarkError::InvalidCircuitSize));
        assert_eq!(validate_k(22), Err(ProofBenchmarkError::InvalidCircuitSize));
    }

    #[test]
    fn safe_cost_model_shapes_are_monotonic() {
        let mut previous_realized = 0;
        for k in PROOF_BENCHMARK_MIN_K..=MAX_EXACT_MODEL_K {
            let (ir, _) = build_ir_for_k(k).expect("build benchmark IR");
            let realized = ir.model().k();
            assert!(
                realized >= previous_realized,
                "requested k={k} realized {realized} after {previous_realized}"
            );
            previous_realized = realized;
        }
    }

    #[test]
    fn high_k_shapes_build_without_running_the_memory_heavy_cost_model() {
        for k in (MAX_EXACT_MODEL_K + 1)..=PROOF_BENCHMARK_MAX_K {
            let (_, hashes) = build_ir_for_k(k).expect("build high-k benchmark IR");
            assert_eq!(hashes, HASHES_FOR_K[usize::from(k)]);
        }
    }

    #[test]
    fn admission_is_process_wide_and_released_by_worker_guard() {
        let state = Arc::new(BenchmarkState::default());
        let first = BenchmarkAdmission::reserve(Arc::clone(&state), 7).expect("first");
        assert!(matches!(
            BenchmarkAdmission::reserve(Arc::clone(&state), 8),
            Err(ProofBenchmarkError::Busy)
        ));
        drop(first);
        assert!(BenchmarkAdmission::reserve(state, 8).is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "downloads public proving parameters and executes a real proof"]
    async fn low_k_prove_and_verify_smoke() {
        let cache_directory =
            std::env::temp_dir().join(format!("oxid-proof-benchmark-smoke-{}", std::process::id()));
        let config = MidnightProofBenchmarkConfig::new(cache_directory.clone())
            .expect("isolated absolute cache path");
        let report = run_benchmark_inner(&config, &BenchmarkState::default(), 4).await;
        let _ = fs::remove_dir_all(cache_directory);
        let report = report.expect("low-k proof benchmark");

        assert_eq!(report.requested_k, 4);
        assert_eq!(
            report.verification_result,
            ProofBenchmarkVerification::Verified
        );
        assert!(report.proof_bytes > 0);
    }
}
