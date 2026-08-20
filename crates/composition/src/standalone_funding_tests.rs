// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt, fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use oxid_adapter_midnight::{
    MidnightShieldedCheckpointConfig, MidnightStandaloneConfig, MidnightSubmissionJournalConfig,
    protected_simulated_midnight_wallet,
    protected_standalone_midnight_wallet_with_checkpoint_options,
};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_platform_ports::{PlatformError, RandomPort};
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, CreateWalletProfileCommand, DeriveWalletAccountCommand,
    PrepareShieldedWalletTransferCommand, PrepareWalletTransferCommand, SelectWalletNetworkCommand,
    SensitiveOperationConfirmation, SubmitWalletTransferCommand, WalletAccountQuery,
    WalletHdPathComponent, WalletProfileSecurityCommand, WalletShieldedSyncCommand,
    WalletShieldedSyncView, WalletTransactionError, WalletTransactionPortError,
    WalletTransferSubmissionQuery,
};
use zeroize::Zeroizing;

use super::{ApplicationServices, compose_with_adapters};

const ENABLE_ENV: &str = "OXID_ENABLE_LIVE_STANDALONE_FUNDING";
const FUNDER_SEED_ENV: &str = "OXID_STANDALONE_FUNDER_SEED_HEX";
const PREPROD_ENABLE_ENV: &str = "OXID_ENABLE_LIVE_PREPROD_E2E";
const PREPROD_MASTER_SEED_ENV: &str = "OXID_PREPROD_MASTER_SEED_HEX";
const PREPROD_CASE_INDEX_ENV: &str = "OXID_PREPROD_E2E_CASE_INDEX";
const PREPROD_COMMIT_ENV: &str = "OXID_PREPROD_E2E_COMMIT";
const PREPROD_NETWORK_ID: &str = "preprod";
const PREPROD_MANIFEST_START: &str = "OXID_PREPROD_FUNDING_MANIFEST_V1";
const PREPROD_MANIFEST_END: &str = "OXID_PREPROD_FUNDING_MANIFEST_END";
const MAX_PREPROD_CASE_INDEX: u32 = (WalletHdPathComponent::MAX_INDEX - 1) / 2;
const TRANSFER_ATOMIC_UNITS: u128 = 5_000_000;
const SHIELDED_TRANSFER_ATOMIC_UNITS: u128 = 1_000_000;
const NATIVE_SHIELDED_TOKEN_TYPE: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

static PATH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreprodHarnessInputError {
    InvalidMasterSeed,
    InvalidCaseIndex,
    InvalidCommit,
}

impl fmt::Display for PreprodHarnessInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidMasterSeed => {
                "the preprod master seed must contain exactly 32 hexadecimal bytes"
            }
            Self::InvalidCaseIndex => "the preprod E2E case index is invalid",
            Self::InvalidCommit => "the preprod E2E commit identifier is invalid",
        };
        formatter.write_str(message)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreprodCase {
    case_index: u32,
    wallet_a_account_index: u32,
    wallet_b_account_index: u32,
}

impl PreprodCase {
    fn parse(value: &str) -> Result<Self, PreprodHarnessInputError> {
        if value.is_empty()
            || !value.bytes().all(|byte| byte.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return Err(PreprodHarnessInputError::InvalidCaseIndex);
        }
        let case_index = value
            .parse::<u32>()
            .map_err(|_| PreprodHarnessInputError::InvalidCaseIndex)?;
        if case_index > MAX_PREPROD_CASE_INDEX {
            return Err(PreprodHarnessInputError::InvalidCaseIndex);
        }
        let wallet_a_account_index = case_index
            .checked_mul(2)
            .ok_or(PreprodHarnessInputError::InvalidCaseIndex)?;
        let wallet_b_account_index = wallet_a_account_index
            .checked_add(1)
            .ok_or(PreprodHarnessInputError::InvalidCaseIndex)?;
        Ok(Self {
            case_index,
            wallet_a_account_index,
            wallet_b_account_index,
        })
    }
}

struct OneShotRootRandom {
    root: Mutex<Option<Zeroizing<[u8; 32]>>>,
}

impl OneShotRootRandom {
    fn new(root: Zeroizing<[u8; 32]>) -> Self {
        Self {
            root: Mutex::new(Some(root)),
        }
    }
}

impl RandomPort for OneShotRootRandom {
    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
        let mut root = self
            .root
            .lock()
            .map_err(|_| PlatformError::RandomnessUnavailable)?;
        if let Some(seed) = root.take() {
            if destination.len() != seed.len() {
                return Err(PlatformError::RandomnessUnavailable);
            }
            destination.copy_from_slice(seed.as_ref());
            return Ok(());
        }
        OsRandom.fill_bytes(destination)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreprodPublicAccount {
    account_index: u32,
    address_index: u32,
    night_unshielded_address: String,
    night_shielded_address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreprodFundingManifest {
    commit: String,
    case_index: u32,
    wallet_a: PreprodPublicAccount,
    wallet_b: PreprodPublicAccount,
}

impl fmt::Display for PreprodFundingManifest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "{PREPROD_MANIFEST_START}")?;
        writeln!(formatter, "commit={}", self.commit)?;
        writeln!(formatter, "network={PREPROD_NETWORK_ID}")?;
        writeln!(formatter, "caseIndex={}", self.case_index)?;
        writeln!(
            formatter,
            "walletA.accountIndex={}",
            self.wallet_a.account_index
        )?;
        writeln!(
            formatter,
            "walletA.addressIndex={}",
            self.wallet_a.address_index
        )?;
        writeln!(
            formatter,
            "walletA.nightUnshieldedAddress={}",
            self.wallet_a.night_unshielded_address
        )?;
        writeln!(
            formatter,
            "walletA.nightShieldedAddress={}",
            self.wallet_a.night_shielded_address
        )?;
        writeln!(
            formatter,
            "walletB.accountIndex={}",
            self.wallet_b.account_index
        )?;
        writeln!(
            formatter,
            "walletB.addressIndex={}",
            self.wallet_b.address_index
        )?;
        writeln!(
            formatter,
            "walletB.nightUnshieldedAddress={}",
            self.wallet_b.night_unshielded_address
        )?;
        writeln!(
            formatter,
            "walletB.nightShieldedAddress={}",
            self.wallet_b.night_shielded_address
        )?;
        formatter.write_str(PREPROD_MANIFEST_END)
    }
}

fn parse_master_seed(value: &str) -> Result<Zeroizing<[u8; 32]>, PreprodHarnessInputError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PreprodHarnessInputError::InvalidMasterSeed);
    }
    let mut root = Zeroizing::new([0_u8; 32]);
    hex::decode_to_slice(value.as_bytes(), root.as_mut())
        .map_err(|_| PreprodHarnessInputError::InvalidMasterSeed)?;
    Ok(root)
}

fn load_preprod_master_seed() -> Result<Zeroizing<[u8; 32]>, PreprodHarnessInputError> {
    let encoded = std::env::var_os(PREPROD_MASTER_SEED_ENV)
        .and_then(|value| value.into_string().ok())
        .map(Zeroizing::new)
        .ok_or(PreprodHarnessInputError::InvalidMasterSeed)?;
    parse_master_seed(encoded.as_str())
}

fn parse_commit(value: &str) -> Result<String, PreprodHarnessInputError> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PreprodHarnessInputError::InvalidCommit);
    }
    Ok(value.to_owned())
}

fn copy_root(root: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut copied = Zeroizing::new([0_u8; 32]);
    copied.as_mut().copy_from_slice(root);
    copied
}

fn derive_preprod_public_account(
    root: Zeroizing<[u8; 32]>,
    account_index: u32,
    display_name: &str,
) -> PreprodPublicAccount {
    let clock = Arc::new(SystemClock);
    let profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::clone(&clock),
        Arc::new(OneShotRootRandom::new(root)),
    ));
    let midnight = Arc::new(
        protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    let application = compose_with_adapters(profiles, security, midnight);
    let profile = application
        .create_wallet_profile()
        .execute(CreateWalletProfileCommand {
            display_name: display_name.to_owned(),
        })
        .expect("preprod manifest profile creation");
    let selected = application
        .select_wallet_network()
        .execute(SelectWalletNetworkCommand {
            profile_id: profile.id.clone(),
            network_id: PREPROD_NETWORK_ID.to_owned(),
        })
        .expect("preprod manifest network selection");
    assert_eq!(selected.selected_network_id, PREPROD_NETWORK_ID);
    application
        .initialize_wallet_security()
        .execute(WalletProfileSecurityCommand {
            profile_id: profile.id.clone(),
        })
        .expect("preprod manifest custody initialization");
    let account = application
        .derive_wallet_account()
        .execute(DeriveWalletAccountCommand {
            profile_id: profile.id,
            account_index,
            address_index: 0,
        })
        .expect("preprod manifest protected account derivation");
    assert_eq!(account.network_id, PREPROD_NETWORK_ID);
    assert_eq!(account.account_index, account_index);
    assert_eq!(account.address_index, 0);
    let night_unshielded_address = account
        .addresses
        .iter()
        .find(|address| address.kind == "unshielded")
        .expect("preprod account exposes its public NIGHT address")
        .value
        .clone();
    let night_shielded_address = account
        .addresses
        .iter()
        .find(|address| address.kind == "shielded")
        .expect("preprod account exposes its public shielded address")
        .value
        .clone();
    assert!(night_unshielded_address.starts_with("mn_addr_preprod1"));
    assert!(night_shielded_address.starts_with("mn_shield-addr_preprod1"));
    assert!(
        account
            .addresses
            .iter()
            .all(|address| address.kind != "dust")
    );
    PreprodPublicAccount {
        account_index,
        address_index: 0,
        night_unshielded_address,
        night_shielded_address,
    }
}

fn build_preprod_funding_manifest(
    root: &Zeroizing<[u8; 32]>,
    selected_case: PreprodCase,
    commit: String,
) -> PreprodFundingManifest {
    let wallet_a = derive_preprod_public_account(
        copy_root(root),
        selected_case.wallet_a_account_index,
        "Preprod E2E wallet A",
    );
    let wallet_b = derive_preprod_public_account(
        copy_root(root),
        selected_case.wallet_b_account_index,
        "Preprod E2E wallet B",
    );
    assert_ne!(
        wallet_a.night_unshielded_address, wallet_b.night_unshielded_address,
        "hardened A/B account separation must change the public NIGHT address"
    );
    assert_ne!(
        wallet_a.night_shielded_address, wallet_b.night_shielded_address,
        "hardened A/B account separation must change the public shielded address"
    );
    PreprodFundingManifest {
        commit,
        case_index: selected_case.case_index,
        wallet_a,
        wallet_b,
    }
}

/// Supplies the externally authorized standalone funding root exactly once,
/// then delegates every nonce/reference to OS randomness. The retained root is
/// zeroized when consumed or dropped.
struct FundingRandom {
    root: Mutex<Option<Zeroizing<[u8; 32]>>>,
}

impl FundingRandom {
    fn from_environment() -> Self {
        let encoded = std::env::var_os(FUNDER_SEED_ENV)
            .and_then(|value| value.into_string().ok())
            .map(Zeroizing::new)
            .expect("the standalone funder seed must be supplied without logging it");
        if encoded.len() != 64 {
            panic!("the standalone funder seed must contain exactly 32 hexadecimal bytes");
        }
        let mut root = Zeroizing::new([0_u8; 32]);
        hex::decode_to_slice(encoded.as_bytes(), root.as_mut())
            .expect("the standalone funder seed must be hexadecimal");
        Self {
            root: Mutex::new(Some(root)),
        }
    }
}

impl RandomPort for FundingRandom {
    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
        let mut root = self
            .root
            .lock()
            .map_err(|_| PlatformError::RandomnessUnavailable)?;
        if let Some(seed) = root.take() {
            if destination.len() != seed.len() {
                return Err(PlatformError::RandomnessUnavailable);
            }
            destination.copy_from_slice(seed.as_ref());
            return Ok(());
        }
        OsRandom.fill_bytes(destination)
    }
}

struct FundingStateDirectory(PathBuf);

impl FundingStateDirectory {
    fn fresh() -> Self {
        let sequence = PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oxid-funded-finality-{}-{nanos}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("isolated funding state directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("private funding state directory");
        }
        Self(path)
    }

    fn journal(&self) -> MidnightSubmissionJournalConfig {
        MidnightSubmissionJournalConfig::new(self.0.join("submissions.json"))
            .expect("isolated journal path")
    }

    fn shielded_checkpoint(&self, label: &str) -> MidnightShieldedCheckpointConfig {
        MidnightShieldedCheckpointConfig::new(
            self.0.join(format!("shielded-{label}-checkpoints.bin")),
        )
        .expect("isolated shielded checkpoint path")
    }

    fn cleanup(mut self) {
        fs::remove_dir_all(&self.0).expect("isolated funding state cleanup");
        self.0 = PathBuf::new();
    }
}

impl Drop for FundingStateDirectory {
    fn drop(&mut self) {
        if self.0.as_os_str().is_empty() {
            return;
        }
        match fs::remove_dir_all(&self.0) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // Drop must not mask a more useful assertion failure. The directory
            // is random, owner-only, and removed on every successful run.
            Err(_) => {}
        }
    }
}

fn standalone_config() -> MidnightStandaloneConfig {
    let placeholder = oxid_adapter_midnight::standalone_configuration_placeholder_address()
        .expect("public configuration address");
    MidnightStandaloneConfig::new(
        "undeployed",
        "ws://127.0.0.1:8088/api/v4/graphql/ws",
        "http://127.0.0.1:8088/api/v4/graphql",
        "ws://127.0.0.1:9944",
        "http://127.0.0.1:6300",
        placeholder.value(),
    )
    .expect("reviewed standalone routes")
}

fn compose_live<N>(
    config: MidnightStandaloneConfig,
    profiles: Arc<InMemoryWalletProfileRepository>,
    security: Arc<DevelopmentWalletSecurity<SystemClock, N>>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    journal: Option<MidnightSubmissionJournalConfig>,
) -> ApplicationServices
where
    N: RandomPort + 'static,
{
    let clock = Arc::new(SystemClock);
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_checkpoint_options(
            config,
            None,
            None,
            shielded_checkpoints,
            journal,
            clock,
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters(profiles, security, midnight)
}

fn initialize_account(application: &ApplicationServices, name: &str) -> (String, String, String) {
    let profile = application
        .create_wallet_profile()
        .execute(CreateWalletProfileCommand {
            display_name: name.to_owned(),
        })
        .expect("profile creation");
    application
        .initialize_wallet_security()
        .execute(WalletProfileSecurityCommand {
            profile_id: profile.id.clone(),
        })
        .expect("ephemeral custody initialization");
    let account = application
        .derive_wallet_account()
        .execute(DeriveWalletAccountCommand {
            profile_id: profile.id.clone(),
            account_index: 0,
            address_index: 0,
        })
        .expect("protected account derivation");
    let shielded_address = account
        .addresses
        .iter()
        .find(|address| address.kind == "shielded")
        .expect("protected shielded address")
        .value
        .clone();
    (profile.id, account.receive_address.value, shielded_address)
}

fn live_night_balance(application: &ApplicationServices, profile_id: &str) -> u128 {
    let account = futures::executor::block_on(application.sync_wallet_account().execute(
        WalletAccountQuery {
            profile_id: profile_id.to_owned(),
        },
    ))
    .expect("live account synchronization");
    assert_eq!(account.source, "live");
    assert_eq!(account.sync.state, "synced");
    account
        .balances
        .iter()
        .find(|balance| balance.symbol == "NIGHT")
        .map(|balance| {
            balance
                .atomic_units
                .parse::<u128>()
                .expect("exact NIGHT balance")
        })
        .unwrap_or(0)
}

fn await_live_night_balance(
    application: &ApplicationServices,
    profile_id: &str,
    expected: u128,
) -> u128 {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let balance = live_night_balance(application, profile_id);
        if balance == expected {
            return balance;
        }
        assert!(
            balance < expected,
            "the fresh standalone recipient received an unexpected balance"
        );
        assert!(
            Instant::now() < deadline,
            "the standalone indexer did not expose finalized recipient funds within 90 seconds"
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn synchronize_shielded(
    application: &ApplicationServices,
    profile_id: &str,
) -> WalletShieldedSyncView {
    application
        .start_wallet_shielded_sync()
        .execute(WalletShieldedSyncCommand {
            profile_id: profile_id.to_owned(),
        })
        .expect("shielded synchronization starts");
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let status = application
            .get_wallet_shielded_sync_status()
            .execute(WalletShieldedSyncCommand {
                profile_id: profile_id.to_owned(),
            })
            .expect("shielded synchronization status");
        match status.state.as_str() {
            "synced" => return status,
            "syncing" | "cached" => {}
            state => panic!(
                "shielded synchronization stopped in state {state} with failure {:?}",
                status.failure
            ),
        }
        assert!(
            Instant::now() < deadline,
            "shielded synchronization did not finish within 90 seconds"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn shielded_balance(status: &WalletShieldedSyncView, token_type: &str) -> u128 {
    status
        .balances
        .iter()
        .find(|balance| balance.token_type_hex == token_type)
        .map(|balance| {
            balance
                .atomic_units
                .parse::<u128>()
                .expect("exact shielded balance")
        })
        .unwrap_or(0)
}

fn assert_complete_shielded_snapshot(status: &WalletShieldedSyncView) {
    assert_eq!(status.state, "synced");
    assert_eq!(status.failure, None);
    assert!(status.current_cursor.is_some());
    assert_eq!(status.current_cursor, status.target_cursor);
    assert!(status.owned_note_count.is_some());
    assert!(status.commitment_count.is_some());
    assert!(status.updated_at_millis.is_some());
}

fn await_shielded_balance(
    application: &ApplicationServices,
    profile_id: &str,
    expected: u128,
) -> WalletShieldedSyncView {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let status = synchronize_shielded(application, profile_id);
        let balance = shielded_balance(&status, NATIVE_SHIELDED_TOKEN_TYPE);
        if balance == expected {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "the standalone indexer did not expose the exact shielded balance within 90 seconds"
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn preprod_case_indices_are_canonical_bounded_hardened_account_pairs() {
    assert_eq!(
        PreprodCase::parse("0"),
        Ok(PreprodCase {
            case_index: 0,
            wallet_a_account_index: 0,
            wallet_b_account_index: 1,
        })
    );
    assert_eq!(
        PreprodCase::parse(&MAX_PREPROD_CASE_INDEX.to_string()),
        Ok(PreprodCase {
            case_index: MAX_PREPROD_CASE_INDEX,
            wallet_a_account_index: WalletHdPathComponent::MAX_INDEX - 1,
            wallet_b_account_index: WalletHdPathComponent::MAX_INDEX,
        })
    );
    for invalid in [
        "",
        "00",
        "01",
        "+1",
        "-1",
        " 1",
        "1 ",
        "1a",
        "1073741824",
        "4294967296",
    ] {
        assert_eq!(
            PreprodCase::parse(invalid),
            Err(PreprodHarnessInputError::InvalidCaseIndex)
        );
    }
}

#[test]
fn preprod_master_seed_and_commit_inputs_fail_without_echoing_values() {
    let secret = "ab".repeat(32);
    assert!(parse_master_seed(&secret).is_ok());
    assert!(parse_master_seed(&secret.to_ascii_uppercase()).is_ok());
    for invalid in ["", "ab", "0x01", &"gg".repeat(32), &format!("{secret}\n")] {
        let error = parse_master_seed(invalid).expect_err("invalid seed must fail closed");
        assert_eq!(error, PreprodHarnessInputError::InvalidMasterSeed);
        if !invalid.is_empty() {
            assert!(!error.to_string().contains(invalid));
        }
    }

    assert!(parse_commit(&"a".repeat(40)).is_ok());
    assert!(parse_commit(&"b".repeat(64)).is_ok());
    for invalid in ["", "A", &"A".repeat(40), &"g".repeat(40)] {
        let error = parse_commit(invalid).expect_err("invalid commit must fail closed");
        assert_eq!(error, PreprodHarnessInputError::InvalidCommit);
        if !invalid.is_empty() {
            assert!(!error.to_string().contains(invalid));
        }
    }
}

#[test]
fn preprod_manifest_derives_separate_accounts_and_emits_only_public_allowlisted_fields() {
    let encoded_root = "01".repeat(32);
    let root = parse_master_seed(&encoded_root).expect("public conformance root");
    let selected_case = PreprodCase::parse("7").expect("bounded case index");
    let manifest = build_preprod_funding_manifest(&root, selected_case, "a".repeat(40));
    let rendered = manifest.to_string();
    let keys = rendered
        .lines()
        .filter_map(|line| line.split_once('=').map(|(key, _)| key))
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec![
            "commit",
            "network",
            "caseIndex",
            "walletA.accountIndex",
            "walletA.addressIndex",
            "walletA.nightUnshieldedAddress",
            "walletA.nightShieldedAddress",
            "walletB.accountIndex",
            "walletB.addressIndex",
            "walletB.nightUnshieldedAddress",
            "walletB.nightShieldedAddress",
        ]
    );
    assert!(rendered.starts_with(PREPROD_MANIFEST_START));
    assert!(rendered.ends_with(PREPROD_MANIFEST_END));
    assert!(rendered.contains("network=preprod"));
    assert!(rendered.contains("walletA.accountIndex=14"));
    assert!(rendered.contains("walletB.accountIndex=15"));
    assert!(!rendered.contains(&encoded_root));
    for forbidden in [
        "seed",
        "digest",
        "dustAddress",
        "dustPublicKey",
        "profileId",
        "transactionKey",
        "utxo",
    ] {
        assert!(!rendered.contains(forbidden));
    }
}

/// Derives the only public values required to fund a deterministic pair of
/// preprod accounts. This deliberately performs no network I/O. A live
/// registration/spend test must remain unavailable until a build-reviewed
/// signed preprod deployment profile and trust root are provisioned and an
/// authenticated test composition can consume them without runtime route or
/// trust-root selection.
#[test]
#[ignore = "requires explicit preprod opt-in and an out-of-band master seed"]
fn preprod_deterministic_funding_manifest_exposes_public_addresses_only() {
    assert_eq!(
        std::env::var(PREPROD_ENABLE_ENV).ok().as_deref(),
        Some("1"),
        "preprod funding manifest requires explicit opt-in"
    );
    let root = load_preprod_master_seed().expect("preprod master seed input");
    let selected_case = std::env::var(PREPROD_CASE_INDEX_ENV)
        .map_err(|_| PreprodHarnessInputError::InvalidCaseIndex)
        .and_then(|value| PreprodCase::parse(&value))
        .expect("bounded preprod case index");
    let commit = std::env::var(PREPROD_COMMIT_ENV)
        .map_err(|_| PreprodHarnessInputError::InvalidCommit)
        .and_then(|value| parse_commit(&value))
        .expect("exact preprod harness commit");
    let manifest = build_preprod_funding_manifest(&root, selected_case, commit);
    println!("{manifest}");
}

/// Funds a fresh OS-random recipient through the same live typed transaction
/// path as mobile, proves finalized inclusion, reconstructs the adapter from
/// the public journal, reconciles, and confirms the recipient sees one output.
///
/// This is ignored because it spends the public development genesis fixture
/// against a running local node. The repository script requires an explicit
/// opt-in and receives the fixture seed from the operator without printing or
/// persisting it.
#[test]
#[ignore = "requires explicit standalone stack and externally supplied development funding seed"]
fn funded_unshielded_finality_survives_adapter_restart_without_duplicate_delivery() {
    assert_eq!(
        std::env::var(ENABLE_ENV).ok().as_deref(),
        Some("1"),
        "live standalone funding requires explicit opt-in"
    );
    let config = standalone_config();
    let state = FundingStateDirectory::fresh();

    let recipient_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let recipient_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    let recipient = compose_live(
        config.clone(),
        recipient_profiles,
        recipient_security,
        None,
        None,
    );
    let (recipient_profile_id, recipient_address, _) =
        initialize_account(&recipient, "Ephemeral funded recipient");

    let funder_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let funder_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(FundingRandom::from_environment()),
    ));
    let funder = compose_live(
        config.clone(),
        Arc::clone(&funder_profiles),
        Arc::clone(&funder_security),
        None,
        Some(state.journal()),
    );
    let (funder_profile_id, _, _) = initialize_account(&funder, "Standalone funding authority");
    assert!(
        live_night_balance(&funder, &funder_profile_id) > TRANSFER_ATOMIC_UNITS,
        "the externally selected standalone funding authority is not funded"
    );

    let prepared = funder
        .prepare_wallet_transfer()
        .execute(PrepareWalletTransferCommand {
            profile_id: funder_profile_id.clone(),
            recipient_address,
            amount_atomic_units: TRANSFER_ATOMIC_UNITS.to_string(),
        })
        .expect("exact unshielded preview");
    assert_eq!(prepared.state, "prepared");
    assert_eq!(
        prepared.amount.atomic_units,
        TRANSFER_ATOMIC_UNITS.to_string()
    );
    assert_eq!(prepared.fee_state, "requires_balancing");

    let authorized = funder
        .authorize_wallet_transfer()
        .execute(AuthorizeWalletTransferCommand {
            profile_id: funder_profile_id.clone(),
            draft_id: prepared.draft_id.clone(),
            authorization_challenge: prepared.authorization_challenge,
            confirmation: SensitiveOperationConfirmation {
                title: "Authorize NIGHT transfer".to_owned(),
                summary: "Fund one ephemeral standalone wallet after exact preview review"
                    .to_owned(),
                confirmed: true,
            },
        })
        .expect("explicit transfer authorization");
    assert_eq!(authorized.state, "authorized");
    assert!(authorized.submission_ready);

    let submitted = futures::executor::block_on(funder.submit_wallet_transfer().execute(
        SubmitWalletTransferCommand {
            profile_id: funder_profile_id.clone(),
            draft_id: prepared.draft_id.clone(),
            confirmation: SensitiveOperationConfirmation {
                title: "Submit NIGHT transfer".to_owned(),
                summary: "Prove and submit the authorized standalone funding transfer".to_owned(),
                confirmed: true,
            },
        },
    ))
    .expect("live proof, submission, and finalized inclusion");
    assert_eq!(submitted.mode, "live");
    assert_eq!(submitted.transfer.state, "submitted");
    assert!(!submitted.transaction_id.is_empty());
    assert!(!submitted.block_id.is_empty());
    let transaction_id = submitted.transaction_id.clone();
    drop(funder);

    let restarted = compose_live(
        config,
        funder_profiles,
        funder_security,
        None,
        Some(state.journal()),
    );
    let history = restarted
        .list_wallet_transfer_submissions()
        .execute(funder_profile_id.clone())
        .expect("durable public submission history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].draft_id, prepared.draft_id);
    assert_eq!(history[0].state, "included");
    assert_eq!(
        history[0].transaction_id.as_deref(),
        Some(transaction_id.as_str())
    );

    let reconciled =
        futures::executor::block_on(restarted.reconcile_wallet_transfer_submission().execute(
            WalletTransferSubmissionQuery {
                profile_id: funder_profile_id,
                draft_id: prepared.draft_id,
            },
        ))
        .expect("restart reconciliation");
    assert_eq!(reconciled.state, "included");
    assert_eq!(
        reconciled.transaction_id.as_deref(),
        Some(transaction_id.as_str())
    );

    let first_balance =
        await_live_night_balance(&recipient, &recipient_profile_id, TRANSFER_ATOMIC_UNITS);
    let second_balance = live_night_balance(&recipient, &recipient_profile_id);
    assert_eq!(first_balance, TRANSFER_ATOMIC_UNITS);
    assert_eq!(second_balance, TRANSFER_ATOMIC_UNITS);
    drop(restarted);
    drop(recipient);
    state.cleanup();
}

/// Spends one genesis-funded shielded note through the normal protected
/// application boundary, then rebuilds the adapter from its private checkpoint
/// and public journal. The post-reconstruction replay must consume the input
/// nullifier, retain only exact change, and never deliver or journal the
/// transfer twice.
#[test]
#[ignore = "requires explicit standalone stack and externally supplied development funding seed"]
fn funded_shielded_finality_survives_adapter_reconstruction_and_consumes_the_input_once() {
    assert_eq!(
        std::env::var(ENABLE_ENV).ok().as_deref(),
        Some("1"),
        "live standalone funding requires explicit opt-in"
    );
    let config = standalone_config();
    let state = FundingStateDirectory::fresh();

    let recipient_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let recipient_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    let recipient = compose_live(
        config.clone(),
        recipient_profiles,
        recipient_security,
        Some(state.shielded_checkpoint("recipient")),
        None,
    );
    let (recipient_profile_id, _, recipient_shielded_address) =
        initialize_account(&recipient, "Ephemeral shielded recipient");
    let recipient_before = synchronize_shielded(&recipient, &recipient_profile_id);
    assert_complete_shielded_snapshot(&recipient_before);
    assert_eq!(
        shielded_balance(&recipient_before, NATIVE_SHIELDED_TOKEN_TYPE),
        0
    );
    assert_eq!(recipient_before.owned_note_count, Some(0));

    let funder_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let funder_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(FundingRandom::from_environment()),
    ));
    let funder = compose_live(
        config.clone(),
        Arc::clone(&funder_profiles),
        Arc::clone(&funder_security),
        Some(state.shielded_checkpoint("funder")),
        Some(state.journal()),
    );
    let (funder_profile_id, _, _) =
        initialize_account(&funder, "Standalone shielded funding authority");
    assert!(
        live_night_balance(&funder, &funder_profile_id) > 0,
        "the shielded funding authority must synchronize its fee-bearing public account"
    );
    let funder_before = synchronize_shielded(&funder, &funder_profile_id);
    assert_complete_shielded_snapshot(&funder_before);
    let initial_funder_balance = shielded_balance(&funder_before, NATIVE_SHIELDED_TOKEN_TYPE);
    assert!(
        initial_funder_balance > SHIELDED_TRANSFER_ATOMIC_UNITS,
        "the externally selected standalone funding authority has no spendable shielded fixture"
    );
    assert!(funder_before.owned_note_count.unwrap_or_default() > 0);

    let prepared = funder
        .prepare_shielded_wallet_transfer()
        .execute(PrepareShieldedWalletTransferCommand {
            profile_id: funder_profile_id.clone(),
            recipient_address: recipient_shielded_address,
            token_type: NATIVE_SHIELDED_TOKEN_TYPE.to_owned(),
            amount_atomic_units: SHIELDED_TRANSFER_ATOMIC_UNITS.to_string(),
        })
        .expect("exact shielded preview");
    assert_eq!(prepared.state, "prepared");
    assert_eq!(
        prepared.amount.atomic_units,
        SHIELDED_TRANSFER_ATOMIC_UNITS.to_string()
    );
    assert_eq!(prepared.recipient_kind, "shielded");
    assert_eq!(prepared.fee_state, "requires_balancing");

    let authorized = funder
        .authorize_wallet_transfer()
        .execute(AuthorizeWalletTransferCommand {
            profile_id: funder_profile_id.clone(),
            draft_id: prepared.draft_id.clone(),
            authorization_challenge: prepared.authorization_challenge,
            confirmation: SensitiveOperationConfirmation {
                title: "Authorize shielded transfer".to_owned(),
                summary: "Send one exact shielded standalone amount after preview review"
                    .to_owned(),
                confirmed: true,
            },
        })
        .expect("explicit shielded transfer authorization");
    assert_eq!(authorized.state, "authorized");
    assert!(authorized.submission_ready);

    let submitted = futures::executor::block_on(funder.submit_wallet_transfer().execute(
        SubmitWalletTransferCommand {
            profile_id: funder_profile_id.clone(),
            draft_id: prepared.draft_id.clone(),
            confirmation: SensitiveOperationConfirmation {
                title: "Submit shielded transfer".to_owned(),
                summary: "Prove and submit the authorized shielded standalone transfer".to_owned(),
                confirmed: true,
            },
        },
    ))
    .expect("live shielded proof, submission, and finalized inclusion");
    assert_eq!(submitted.mode, "live");
    assert_eq!(submitted.transfer.state, "submitted");
    assert!(!submitted.transaction_id.is_empty());
    assert!(!submitted.block_id.is_empty());
    let transaction_id = submitted.transaction_id.clone();

    let duplicate =
        funder
            .prepare_shielded_wallet_transfer()
            .execute(PrepareShieldedWalletTransferCommand {
                profile_id: funder_profile_id.clone(),
                recipient_address: prepared.recipient_address.clone(),
                token_type: NATIVE_SHIELDED_TOKEN_TYPE.to_owned(),
                amount_atomic_units: SHIELDED_TRANSFER_ATOMIC_UNITS.to_string(),
            });
    assert_eq!(
        duplicate,
        Err(WalletTransactionError::Operation(
            WalletTransactionPortError::DraftConflict
        )),
        "included journal state must block the unchanged private note set before replay"
    );
    drop(funder);

    let restarted = compose_live(
        config,
        funder_profiles,
        funder_security,
        Some(state.shielded_checkpoint("funder")),
        Some(state.journal()),
    );
    let history = restarted
        .list_wallet_transfer_submissions()
        .execute(funder_profile_id.clone())
        .expect("durable shielded submission history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].draft_id, prepared.draft_id);
    assert_eq!(history[0].state, "included");
    assert_eq!(
        history[0].transaction_id.as_deref(),
        Some(transaction_id.as_str())
    );

    let restored =
        futures::executor::block_on(restarted.reconcile_wallet_transfer_submission().execute(
            WalletTransferSubmissionQuery {
                profile_id: funder_profile_id.clone(),
                draft_id: prepared.draft_id.clone(),
            },
        ))
        .expect("durable shielded included-status restoration");
    assert_eq!(restored.state, "included");
    let restored_again =
        futures::executor::block_on(restarted.reconcile_wallet_transfer_submission().execute(
            WalletTransferSubmissionQuery {
                profile_id: funder_profile_id.clone(),
                draft_id: prepared.draft_id,
            },
        ))
        .expect("idempotent shielded included-status read");
    assert_eq!(restored_again.state, "included");
    assert_eq!(restored_again.transaction_id, restored.transaction_id);

    let funder_after = await_shielded_balance(
        &restarted,
        &funder_profile_id,
        initial_funder_balance - SHIELDED_TRANSFER_ATOMIC_UNITS,
    );
    assert_complete_shielded_snapshot(&funder_after);
    let recipient_after = await_shielded_balance(
        &recipient,
        &recipient_profile_id,
        SHIELDED_TRANSFER_ATOMIC_UNITS,
    );
    let recipient_second = synchronize_shielded(&recipient, &recipient_profile_id);
    assert_complete_shielded_snapshot(&recipient_after);
    assert_complete_shielded_snapshot(&recipient_second);
    assert_eq!(
        shielded_balance(&recipient_after, NATIVE_SHIELDED_TOKEN_TYPE),
        SHIELDED_TRANSFER_ATOMIC_UNITS
    );
    assert_eq!(
        shielded_balance(&recipient_second, NATIVE_SHIELDED_TOKEN_TYPE),
        SHIELDED_TRANSFER_ATOMIC_UNITS
    );
    assert_eq!(recipient_after.owned_note_count, Some(1));
    assert_eq!(recipient_second.owned_note_count, Some(1));
    assert_eq!(
        restarted
            .list_wallet_transfer_submissions()
            .execute(funder_profile_id)
            .expect("stable shielded submission history")
            .len(),
        1
    );
    drop(restarted);
    drop(recipient);
    state.cleanup();
}
