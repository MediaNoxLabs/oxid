// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    io::{Cursor, Read as _, Write as _},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use futures::{Stream, StreamExt as _};
use midnight_base_crypto::data_provider::{FetchMode, MidnightDataProvider, OutputMode, hexhash};
use midnight_ledger::{
    dust::{DUST_EXPECTED_FILES, DustResolver},
    prove::Resolver as LedgerResolver,
    structure::{ProofMarker, ProofPreimageMarker, Transaction},
};
use midnight_storage::DefaultDB;
use midnight_transient_crypto::{
    commitment::{PedersenRandomness, PureGeneratorPedersen},
    proofs::Zkir as _,
};
use midnight_zkir::{IrSource, LocalProvingProvider};
use midnight_zswap::{ZSWAP_EXPECTED_FILES, prove::ZswapResolver};
use oxid_wallet_application::WalletTransactionPortError;
use rand::{RngCore as _, rngs::OsRng};
use reqwest::Url;
use sha2::{Digest as _, Sha256};

use midnight_base_crypto::signatures::Signature;

const PARAMETER_SOURCE: &str = "https://srs.midnight.network/";
const DUST_CIRCUIT_K: u8 = 13;
const DUST_PARAMETER_NAME: &str = "bls_midnight_2p13";
const DUST_PARAMETER_HASH: [u8; 32] =
    hexhash(b"d3324910969c4cc54143b8045b649e5c3a4bd5fb7b8f85fe1b770f640ce1c803");
const MAX_CACHE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PARAMETER_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PROVER_KEY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VERIFIER_KEY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_IR_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 64;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10 * 60);

type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;
pub(crate) type ProvenTransaction =
    Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

/// Explicit app-private cache boundary for authenticated Midnight proving material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightLocalProvingConfig {
    cache_directory: PathBuf,
}

impl MidnightLocalProvingConfig {
    /// Creates a local-proving configuration rooted at an app-owned absolute path.
    pub fn new(
        cache_directory: impl Into<PathBuf>,
    ) -> Result<Self, MidnightLocalProvingConfigError> {
        let cache_directory = cache_directory.into();
        if !cache_directory.is_absolute()
            || cache_directory
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(MidnightLocalProvingConfigError::InvalidCacheDirectory);
        }
        Ok(Self { cache_directory })
    }

    #[must_use]
    pub fn cache_directory(&self) -> &Path {
        &self.cache_directory
    }
}

/// Safe configuration failures that never render a local filesystem path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightLocalProvingConfigError {
    InvalidCacheDirectory,
}

impl std::fmt::Display for MidnightLocalProvingConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Midnight local proving cache must be an absolute app-private path")
    }
}

impl std::error::Error for MidnightLocalProvingConfigError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidnightLocalProvingMetrics {
    circuit_k: u8,
    circuit_rows: u64,
    cache_bytes: u64,
    preparation_elapsed: Duration,
    proving_elapsed: Duration,
}

/// Output from the opt-in, non-production DUST proving interoperability harness.
#[cfg(feature = "proving-bench")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidnightLocalProvingFixtureReport {
    metrics: MidnightLocalProvingMetrics,
    proof_bytes: usize,
    sealed_transaction_bytes: usize,
}

#[cfg(feature = "proving-bench")]
impl MidnightLocalProvingFixtureReport {
    #[must_use]
    pub const fn metrics(self) -> MidnightLocalProvingMetrics {
        self.metrics
    }

    #[must_use]
    pub const fn proof_bytes(self) -> usize {
        self.proof_bytes
    }

    #[must_use]
    pub const fn sealed_transaction_bytes(self) -> usize {
        self.sealed_transaction_bytes
    }
}

impl MidnightLocalProvingMetrics {
    #[must_use]
    pub const fn circuit_k(self) -> u8 {
        self.circuit_k
    }

    #[must_use]
    pub const fn circuit_rows(self) -> u64 {
        self.circuit_rows
    }

    #[must_use]
    pub const fn cache_bytes(self) -> u64 {
        self.cache_bytes
    }

    #[must_use]
    pub const fn preparation_elapsed(self) -> Duration {
        self.preparation_elapsed
    }

    #[must_use]
    pub const fn proving_elapsed(self) -> Duration {
        self.proving_elapsed
    }
}

pub(crate) struct LocalProvingOutcome {
    pub(crate) transaction: ProvenTransaction,
    pub(crate) metrics: MidnightLocalProvingMetrics,
}

/// Proves one deterministic synthetic DUST spend and verifies that the sealed
/// transaction round-trips through the pinned ledger codec. This is an opt-in
/// measurement harness; it never submits the fixture to a node.
#[cfg(feature = "proving-bench")]
pub async fn run_local_proving_fixture(
    config: &MidnightLocalProvingConfig,
    cancellation: &AtomicBool,
) -> Result<MidnightLocalProvingFixtureReport, WalletTransactionPortError> {
    use midnight_base_crypto::{hash::HashOutput, time::Timestamp};
    use midnight_ledger::{
        dust::{
            DustActions, DustGenerationInfo, DustLocalState, DustPublicKey, DustSecretKey,
            InitialNonce, QualifiedDustOutput, dust_first_nonce,
        },
        structure::{INITIAL_PARAMETERS, Intent},
    };
    use midnight_storage::{arena::Sp, storage::Array, storage::HashMap as LedgerHashMap};

    let dust_key = DustSecretKey::derive_secret_key(&[0x5a; 32]);
    let owner = DustPublicKey::from(dust_key.clone());
    let backing_night = InitialNonce(HashOutput([0x2a; 32]));
    let created_at = Timestamp::from_secs(1);
    let generation = DustGenerationInfo {
        value: 1_000_000,
        owner,
        nonce: backing_night,
        dtime: Timestamp::MAX,
    };
    let output = QualifiedDustOutput {
        initial_value: 1_000_000,
        owner,
        nonce: dust_first_nonce(&backing_night, &owner),
        seq: 0,
        ctime: created_at,
        backing_night,
        mt_index: 0,
    };
    let state = DustLocalState::<DefaultDB>::new(INITIAL_PARAMETERS.dust)
        .insert_generation_info(0, generation, Some(backing_night))
        .map_err(|_| WalletTransactionPortError::InvalidData)?
        .insert_commitment(0, output, true)
        .map_err(|_| WalletTransactionPortError::InvalidData)?
        .add_utxo(&output.nullifier(&dust_key), &output, None)
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    let spent_at = Timestamp::from_secs(2);
    let (_, spend) = state
        .spend(&dust_key, &output, 42, spent_at)
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    let mut intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB> =
        Intent::empty(&mut OsRng, Timestamp::from_secs(3_600));
    intent.dust_actions = Some(Sp::new(DustActions {
        spends: Array::new().push(spend),
        registrations: Array::new(),
        ctime: spent_at,
    }));
    let transaction =
        Transaction::from_intents("undeployed", LedgerHashMap::new().insert(0xFEED, intent));
    let outcome = prove_transaction(transaction, config, cancellation).await?;
    let mut proof_bytes = 0_usize;
    for (_, intent) in outcome.transaction.intents() {
        if let Some(actions) = intent.dust_actions {
            for spend in actions.spends.iter_deref() {
                proof_bytes = proof_bytes
                    .checked_add(spend.proof.0.len())
                    .ok_or(WalletTransactionPortError::InvalidData)?;
            }
        }
    }
    let mut encoded = Vec::new();
    midnight_serialize::tagged_serialize(&outcome.transaction, &mut encoded)
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    let decoded: ProvenTransaction = midnight_serialize::tagged_deserialize(&encoded[..])
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    let mut round_trip = Vec::new();
    midnight_serialize::tagged_serialize(&decoded, &mut round_trip)
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    if encoded != round_trip {
        return Err(WalletTransactionPortError::InvalidData);
    }
    Ok(MidnightLocalProvingFixtureReport {
        metrics: outcome.metrics,
        proof_bytes,
        sealed_transaction_bytes: encoded.len(),
    })
}

#[derive(Clone, Copy)]
struct ExpectedArtifact {
    name: &'static str,
    hash: [u8; 32],
    maximum_bytes: u64,
}

pub(crate) async fn prove_transaction(
    transaction: UnprovenTransaction,
    config: &MidnightLocalProvingConfig,
    cancellation: &AtomicBool,
) -> Result<LocalProvingOutcome, WalletTransactionPortError> {
    ensure_not_cancelled(cancellation)?;
    ensure_tls_provider()?;
    let preparation_started = Instant::now();
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(15))
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let source =
        Url::parse(PARAMETER_SOURCE).map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    #[cfg(feature = "proving-bench")]
    eprintln!("local proving: authenticating bounded cache");
    prepare_cache(config.cache_directory(), &client, &source, cancellation).await?;
    #[cfg(feature = "proving-bench")]
    eprintln!("local proving: inspecting DUST circuit");
    let (circuit_k, circuit_rows) = inspect_dust_circuit(config.cache_directory())?;
    let cache_bytes = audit_cache(config.cache_directory())?;
    let preparation_elapsed = preparation_started.elapsed();
    ensure_not_cancelled(cancellation)?;

    let expected_data = DUST_EXPECTED_FILES
        .iter()
        .chain(ZSWAP_EXPECTED_FILES.iter())
        .copied()
        .collect::<Vec<_>>();
    let parameters = MidnightDataProvider {
        fetch_mode: FetchMode::Synchronous,
        base_url: source,
        output_mode: OutputMode::Log,
        expected_data,
        dir: config.cache_directory().to_path_buf(),
    };
    let resolver = LedgerResolver::new(
        ZswapResolver(parameters.clone()),
        DustResolver(parameters.clone()),
        Box::new(|_| Box::pin(async { Ok(None) })),
    );
    let provider = LocalProvingProvider {
        rng: OsRng,
        resolver: &resolver,
        params: &resolver,
    };
    #[cfg(feature = "proving-bench")]
    eprintln!("local proving: generating DUST proof");
    let proving_started = Instant::now();
    let proved = transaction
        .prove(
            provider,
            &midnight_onchain_runtime::cost_model::INITIAL_COST_MODEL,
        )
        .await
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let proving_elapsed = proving_started.elapsed();
    #[cfg(feature = "proving-bench")]
    eprintln!("local proving: sealing proved transaction");
    ensure_not_cancelled(cancellation)?;

    Ok(LocalProvingOutcome {
        transaction: proved.seal(OsRng),
        metrics: MidnightLocalProvingMetrics {
            circuit_k,
            circuit_rows,
            cache_bytes,
            preparation_elapsed,
            proving_elapsed,
        },
    })
}

fn ensure_tls_provider() -> Result<(), WalletTransactionPortError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    rustls::crypto::CryptoProvider::get_default()
        .map(|_| ())
        .ok_or(WalletTransactionPortError::ProvingFailed)
}

async fn prepare_cache(
    directory: &Path,
    client: &reqwest::Client,
    source: &Url,
    cancellation: &AtomicBool,
) -> Result<(), WalletTransactionPortError> {
    ensure_cache_directory(directory)?;
    audit_cache(directory)?;

    for &(name, hash, _) in DUST_EXPECTED_FILES
        .iter()
        .chain(ZSWAP_EXPECTED_FILES.iter())
    {
        let maximum_bytes = if name.ends_with(".prover") {
            MAX_PROVER_KEY_BYTES
        } else if name.ends_with(".verifier") {
            MAX_VERIFIER_KEY_BYTES
        } else {
            MAX_IR_BYTES
        };
        ensure_artifact(
            directory,
            client,
            source,
            ExpectedArtifact {
                name,
                hash,
                maximum_bytes,
            },
            cancellation,
        )
        .await?;
        audit_cache(directory)?;
    }
    ensure_artifact(
        directory,
        client,
        source,
        ExpectedArtifact {
            name: DUST_PARAMETER_NAME,
            hash: DUST_PARAMETER_HASH,
            maximum_bytes: MAX_PARAMETER_BYTES,
        },
        cancellation,
    )
    .await?;
    audit_cache(directory)?;
    Ok(())
}

async fn ensure_artifact(
    directory: &Path,
    client: &reqwest::Client,
    source: &Url,
    artifact: ExpectedArtifact,
    cancellation: &AtomicBool,
) -> Result<(), WalletTransactionPortError> {
    let destination = directory.join(artifact.name);
    if validate_cached_artifact(&destination, artifact)? {
        return Ok(());
    }
    ensure_not_cancelled(cancellation)?;
    let url = source
        .join(artifact.name)
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    if !response.status().is_success() {
        return Err(WalletTransactionPortError::ProvingFailed);
    }
    install_artifact_stream(
        directory,
        artifact,
        response.content_length(),
        response.bytes_stream(),
        cancellation,
    )
    .await
}

async fn install_artifact_stream<S, B, E>(
    directory: &Path,
    artifact: ExpectedArtifact,
    content_length: Option<u64>,
    mut stream: S,
    cancellation: &AtomicBool,
) -> Result<(), WalletTransactionPortError>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
{
    ensure_not_cancelled(cancellation)?;
    if content_length.is_some_and(|length| length > artifact.maximum_bytes) {
        return Err(WalletTransactionPortError::ProvingFailed);
    }
    let destination = directory.join(artifact.name);
    let parent = destination
        .parent()
        .ok_or(WalletTransactionPortError::ProvingFailed)?;
    ensure_cache_parent(directory, parent)?;

    let mut random = OsRng;
    let temporary = destination.with_extension(format!("oxid-part-{:016x}", random.next_u64()));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let result = async {
        let mut length = 0_u64;
        let mut digest = Sha256::new();
        while let Some(chunk) = stream.next().await {
            ensure_not_cancelled(cancellation)?;
            let chunk = chunk.map_err(|_| WalletTransactionPortError::ProvingFailed)?;
            let chunk = chunk.as_ref();
            length = length
                .checked_add(
                    u64::try_from(chunk.len())
                        .map_err(|_| WalletTransactionPortError::ProvingFailed)?,
                )
                .ok_or(WalletTransactionPortError::ProvingFailed)?;
            if length > artifact.maximum_bytes {
                return Err(WalletTransactionPortError::ProvingFailed);
            }
            file.write_all(chunk)
                .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
            digest.update(chunk);
        }
        if <[u8; 32]>::from(digest.finalize()) != artifact.hash {
            return Err(WalletTransactionPortError::ProvingFailed);
        }
        file.sync_all()
            .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
        Ok(())
    }
    .await;
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return result;
    }
    fs::rename(&temporary, &destination).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        WalletTransactionPortError::ProvingFailed
    })?;
    Ok(())
}

fn validate_cached_artifact(
    path: &Path,
    artifact: ExpectedArtifact,
) -> Result<bool, WalletTransactionPortError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(WalletTransactionPortError::ProvingFailed),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > artifact.maximum_bytes
    {
        return Err(WalletTransactionPortError::ProvingFailed);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(WalletTransactionPortError::ProvingFailed);
        }
    }
    let file = fs::File::open(path).map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let mut reader = file.take(artifact.maximum_bytes + 1);
    let mut digest = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(
                u64::try_from(read).map_err(|_| WalletTransactionPortError::ProvingFailed)?,
            )
            .ok_or(WalletTransactionPortError::ProvingFailed)?;
        digest.update(&buffer[..read]);
    }
    if copied != metadata.len() || <[u8; 32]>::from(digest.finalize()) != artifact.hash {
        return Err(WalletTransactionPortError::ProvingFailed);
    }
    Ok(true)
}

fn inspect_dust_circuit(directory: &Path) -> Result<(u8, u64), WalletTransactionPortError> {
    let ir_path = DUST_EXPECTED_FILES
        .iter()
        .find_map(|(name, _, _)| name.ends_with(".bzkir").then(|| directory.join(name)))
        .ok_or(WalletTransactionPortError::ProvingFailed)?;
    let bytes = fs::read(ir_path).map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    if bytes.len() as u64 > MAX_IR_BYTES {
        return Err(WalletTransactionPortError::ProvingFailed);
    }
    let ir = IrSource::load_from_tagged(Cursor::new(bytes))
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let k = ir.k();
    #[cfg(feature = "proving-bench")]
    eprintln!("local proving: DUST circuit declares k={k}");
    if k != DUST_CIRCUIT_K {
        return Err(WalletTransactionPortError::ProvingFailed);
    }
    let rows =
        u64::try_from(ir.model().rows()).map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    Ok((k, rows))
}

fn ensure_cache_directory(directory: &Path) -> Result<(), WalletTransactionPortError> {
    let mut created = false;
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(WalletTransactionPortError::ProvingFailed);
                }
            }
        }
        Ok(_) => return Err(WalletTransactionPortError::ProvingFailed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                let mut builder = fs::DirBuilder::new();
                builder.recursive(true).mode(0o700);
                builder
                    .create(directory)
                    .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(directory).map_err(|_| WalletTransactionPortError::ProvingFailed)?;
            created = true;
        }
        Err(_) => return Err(WalletTransactionPortError::ProvingFailed),
    }
    #[cfg(unix)]
    if created {
        fs::set_permissions(directory, {
            use std::os::unix::fs::PermissionsExt as _;
            fs::Permissions::from_mode(0o700)
        })
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    }
    Ok(())
}

fn ensure_cache_parent(root: &Path, parent: &Path) -> Result<(), WalletTransactionPortError> {
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(WalletTransactionPortError::ProvingFailed);
        };
        current.push(component);
        let created = match fs::symlink_metadata(&current) {
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt as _;
                    fs::DirBuilder::new()
                        .mode(0o700)
                        .create(&current)
                        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
                }
                #[cfg(not(unix))]
                fs::create_dir(&current).map_err(|_| WalletTransactionPortError::ProvingFailed)?;
                true
            }
            Err(_) => return Err(WalletTransactionPortError::ProvingFailed),
        };
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(WalletTransactionPortError::ProvingFailed);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if created {
                fs::set_permissions(&current, fs::Permissions::from_mode(0o700))
                    .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
            } else if metadata.permissions().mode() & 0o077 != 0 {
                return Err(WalletTransactionPortError::ProvingFailed);
            }
        }
    }
    Ok(())
}

fn audit_cache(directory: &Path) -> Result<u64, WalletTransactionPortError> {
    fn walk(
        path: &Path,
        entries: &mut usize,
        total: &mut u64,
    ) -> Result<(), WalletTransactionPortError> {
        for entry in fs::read_dir(path).map_err(|_| WalletTransactionPortError::ProvingFailed)? {
            let entry = entry.map_err(|_| WalletTransactionPortError::ProvingFailed)?;
            *entries = entries
                .checked_add(1)
                .ok_or(WalletTransactionPortError::ProvingFailed)?;
            if *entries > MAX_CACHE_ENTRIES {
                return Err(WalletTransactionPortError::ProvingFailed);
            }
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
            if metadata.file_type().is_symlink() {
                return Err(WalletTransactionPortError::ProvingFailed);
            }
            if metadata.is_dir() {
                walk(&entry.path(), entries, total)?;
            } else if metadata.is_file() {
                *total = total
                    .checked_add(metadata.len())
                    .ok_or(WalletTransactionPortError::ProvingFailed)?;
                if *total > MAX_CACHE_BYTES {
                    return Err(WalletTransactionPortError::ProvingFailed);
                }
            } else {
                return Err(WalletTransactionPortError::ProvingFailed);
            }
        }
        Ok(())
    }

    let mut entries = 0;
    let mut total = 0;
    walk(directory, &mut entries, &mut total)?;
    Ok(total)
}

fn ensure_not_cancelled(cancellation: &AtomicBool) -> Result<(), WalletTransactionPortError> {
    if cancellation.load(Ordering::Acquire) {
        Err(WalletTransactionPortError::SubmissionCancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{io, sync::atomic::AtomicBool};

    use super::*;
    use futures::{executor::block_on, stream};

    fn isolated_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("oxid-local-proving-{name}-{}", std::process::id()))
    }

    #[test]
    fn configuration_requires_an_absolute_normalized_cache_path() {
        assert_eq!(
            MidnightLocalProvingConfig::new("relative/cache").err(),
            Some(MidnightLocalProvingConfigError::InvalidCacheDirectory)
        );
        assert_eq!(
            MidnightLocalProvingConfig::new(
                std::env::temp_dir().join("..").join("oxid-invalid-cache"),
            )
            .err(),
            Some(MidnightLocalProvingConfigError::InvalidCacheDirectory)
        );
        assert!(MidnightLocalProvingConfig::new(isolated_directory("valid")).is_ok());
        assert_eq!(
            MidnightLocalProvingConfigError::InvalidCacheDirectory.to_string(),
            "Midnight local proving cache must be an absolute app-private path"
        );
    }

    #[test]
    fn proving_metrics_expose_bounded_measurements() {
        let metrics = MidnightLocalProvingMetrics {
            circuit_k: 13,
            circuit_rows: 5_646,
            cache_bytes: 3_752_829,
            preparation_elapsed: Duration::from_millis(17),
            proving_elapsed: Duration::from_millis(311),
        };

        assert_eq!(metrics.circuit_k(), 13);
        assert_eq!(metrics.circuit_rows(), 5_646);
        assert_eq!(metrics.cache_bytes(), 3_752_829);
        assert_eq!(metrics.preparation_elapsed(), Duration::from_millis(17));
        assert_eq!(metrics.proving_elapsed(), Duration::from_millis(311));
        ensure_tls_provider().expect("the pinned rustls provider installs");
    }

    #[test]
    fn streamed_artifact_installation_is_authenticated_and_bounded() {
        let directory = isolated_directory("stream-success");
        let _ = fs::remove_dir_all(&directory);
        ensure_cache_directory(&directory).expect("isolated cache is created");
        let bytes = b"authenticated fixture";
        let artifact = ExpectedArtifact {
            name: "nested/fixture",
            hash: <[u8; 32]>::from(Sha256::digest(bytes)),
            maximum_bytes: 64,
        };
        let chunks = stream::iter([
            Ok::<_, io::Error>(bytes[..8].to_vec()),
            Ok(bytes[8..].to_vec()),
        ]);

        block_on(install_artifact_stream(
            &directory,
            artifact,
            Some(bytes.len() as u64),
            chunks,
            &AtomicBool::new(false),
        ))
        .expect("authenticated bounded stream installs");

        let destination = directory.join(artifact.name);
        assert_eq!(fs::read(&destination).expect("artifact is readable"), bytes);
        assert_eq!(validate_cached_artifact(&destination, artifact), Ok(true));
        assert_eq!(audit_cache(&directory), Ok(bytes.len() as u64));
        fs::remove_dir_all(directory).expect("isolated cache is removed");
    }

    #[test]
    fn streamed_artifact_installation_rejects_untrusted_inputs_without_residue() {
        struct Case {
            name: &'static str,
            content_length: Option<u64>,
            chunks: Vec<Result<Vec<u8>, io::Error>>,
            hash: [u8; 32],
            maximum_bytes: u64,
            cancelled: bool,
        }

        let cases = [
            Case {
                name: "declared-oversize",
                content_length: Some(5),
                chunks: vec![Ok(b"data".to_vec())],
                hash: <[u8; 32]>::from(Sha256::digest(b"data")),
                maximum_bytes: 4,
                cancelled: false,
            },
            Case {
                name: "streamed-oversize",
                content_length: None,
                chunks: vec![Ok(b"abc".to_vec()), Ok(b"de".to_vec())],
                hash: <[u8; 32]>::from(Sha256::digest(b"abcde")),
                maximum_bytes: 4,
                cancelled: false,
            },
            Case {
                name: "digest-mismatch",
                content_length: Some(4),
                chunks: vec![Ok(b"data".to_vec())],
                hash: [0; 32],
                maximum_bytes: 4,
                cancelled: false,
            },
            Case {
                name: "stream-error",
                content_length: None,
                chunks: vec![Err(io::Error::other("fixture stream failed"))],
                hash: [0; 32],
                maximum_bytes: 4,
                cancelled: false,
            },
            Case {
                name: "cancelled",
                content_length: None,
                chunks: vec![Ok(b"data".to_vec())],
                hash: <[u8; 32]>::from(Sha256::digest(b"data")),
                maximum_bytes: 4,
                cancelled: true,
            },
        ];

        for case in cases {
            let directory = isolated_directory(case.name);
            let _ = fs::remove_dir_all(&directory);
            ensure_cache_directory(&directory).expect("isolated cache is created");
            let artifact = ExpectedArtifact {
                name: "fixture",
                hash: case.hash,
                maximum_bytes: case.maximum_bytes,
            };
            let result = block_on(install_artifact_stream(
                &directory,
                artifact,
                case.content_length,
                stream::iter(case.chunks),
                &AtomicBool::new(case.cancelled),
            ));

            assert!(matches!(
                result,
                Err(WalletTransactionPortError::ProvingFailed
                    | WalletTransactionPortError::SubmissionCancelled)
            ));
            assert!(!directory.join(artifact.name).exists());
            assert_eq!(audit_cache(&directory), Ok(0));
            fs::remove_dir_all(directory).expect("isolated cache is removed");
        }
    }

    #[test]
    fn cache_audit_rejects_oversized_and_symlinked_content() {
        let directory = isolated_directory("audit");
        let _ = fs::remove_dir_all(&directory);
        ensure_cache_directory(&directory).expect("isolated cache is created");
        let oversized = directory.join("oversized");
        let file = fs::File::create(&oversized).expect("test file is created");
        file.set_len(MAX_CACHE_BYTES + 1)
            .expect("sparse test file is sized");
        assert_eq!(
            audit_cache(&directory).err(),
            Some(WalletTransactionPortError::ProvingFailed)
        );
        fs::remove_file(oversized).expect("test file is removed");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&directory, directory.join("link")).expect("test symlink is created");
            assert_eq!(
                audit_cache(&directory).err(),
                Some(WalletTransactionPortError::ProvingFailed)
            );
        }
        fs::remove_dir_all(directory).expect("isolated cache is removed");
    }

    #[test]
    fn cached_artifacts_must_be_authenticated_and_owner_private() {
        let directory = isolated_directory("artifact");
        let _ = fs::remove_dir_all(&directory);
        ensure_cache_directory(&directory).expect("isolated cache is created");
        let artifact_path = directory.join("fixture");
        fs::write(&artifact_path, b"wrong bytes").expect("fixture is written");
        #[cfg(unix)]
        fs::set_permissions(&artifact_path, {
            use std::os::unix::fs::PermissionsExt as _;
            fs::Permissions::from_mode(0o600)
        })
        .expect("fixture is owner private");
        let expected = ExpectedArtifact {
            name: "fixture",
            hash: <[u8; 32]>::from(Sha256::digest(b"expected bytes")),
            maximum_bytes: 64,
        };
        assert_eq!(
            validate_cached_artifact(&artifact_path, expected).err(),
            Some(WalletTransactionPortError::ProvingFailed)
        );

        fs::write(&artifact_path, b"expected bytes").expect("fixture is corrected");
        #[cfg(unix)]
        fs::set_permissions(&artifact_path, {
            use std::os::unix::fs::PermissionsExt as _;
            fs::Permissions::from_mode(0o600)
        })
        .expect("fixture stays owner private");
        assert_eq!(validate_cached_artifact(&artifact_path, expected), Ok(true));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&artifact_path, fs::Permissions::from_mode(0o644))
                .expect("fixture permissions change");
            assert_eq!(
                validate_cached_artifact(&artifact_path, expected).err(),
                Some(WalletTransactionPortError::ProvingFailed)
            );
        }
        fs::remove_dir_all(directory).expect("isolated cache is removed");
    }

    #[cfg(unix)]
    #[test]
    fn cache_directories_and_nested_parents_must_be_owner_private_and_real() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let directory = isolated_directory("directory-security");
        let _ = fs::remove_dir_all(&directory);
        ensure_cache_directory(&directory).expect("isolated cache is created");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))
            .expect("test permissions change");
        assert_eq!(
            ensure_cache_directory(&directory).err(),
            Some(WalletTransactionPortError::ProvingFailed)
        );
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("test permissions are restored");

        let outside = isolated_directory("directory-security-outside");
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir(&outside).expect("outside directory is created");
        symlink(&outside, directory.join("nested")).expect("test symlink is created");
        assert_eq!(
            ensure_cache_parent(&directory, &directory.join("nested/child")).err(),
            Some(WalletTransactionPortError::ProvingFailed)
        );
        assert_eq!(
            ensure_cache_parent(&directory, &outside).err(),
            Some(WalletTransactionPortError::ProvingFailed)
        );

        fs::remove_dir_all(directory).expect("isolated cache is removed");
        fs::remove_dir_all(outside).expect("outside fixture is removed");
    }

    #[test]
    fn cancellation_fails_before_network_or_proving_work() {
        let cancellation = AtomicBool::new(true);
        assert_eq!(
            ensure_not_cancelled(&cancellation).err(),
            Some(WalletTransactionPortError::SubmissionCancelled)
        );
    }
}
