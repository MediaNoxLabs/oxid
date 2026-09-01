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

use oxid_adapter_deployment_profile::{
    AuthenticatedDeploymentProfile, DeploymentProfileVerifier, DeploymentTrustRoot,
};
use oxid_adapter_midnight::{
    MidnightAccountCheckpointConfig, MidnightDustCheckpointConfig,
    MidnightShieldedCheckpointConfig, MidnightStandaloneConfig, MidnightSubmissionJournalConfig,
    protected_simulated_midnight_wallet,
    protected_standalone_midnight_wallet_with_checkpoint_options,
};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_platform_ports::{PlatformError, RandomPort};
use oxid_wallet_application::{
    AuthorizeWalletDustRegistrationCommand, AuthorizeWalletTransferCommand,
    CreateWalletProfileCommand, DeriveWalletAccountCommand, GetWalletDustRegistrationStatusCommand,
    PrepareShieldedWalletTransferCommand, PrepareWalletDustRegistrationCommand,
    PrepareWalletTransferCommand, ReconcileWalletDustRegistrationSubmissionCommand,
    SelectWalletNetworkCommand, SensitiveOperationConfirmation,
    SubmitWalletDustRegistrationCommand, SubmitWalletTransferCommand, WalletAccountQuery,
    WalletDustRegistrationError, WalletDustRegistrationPortError,
    WalletDustRegistrationPreviewView, WalletDustSyncCommand, WalletDustSyncView,
    WalletHdPathComponent, WalletProfileSecurityCommand, WalletProtectionPort,
    WalletShieldedSyncCommand, WalletShieldedSyncView, WalletTransactionError,
    WalletTransactionPortError, WalletTransferDraftQuery, WalletTransferSubmissionQuery,
};
use zeroize::Zeroizing;

use super::{
    ApplicationServices,
    identity::{CredentialPresentationComposition, HeadlessCredentialProfile},
    standalone_genesis::{PUBLIC_STANDALONE_PROFILE_NAME, public_profile_protection},
    wiring::{compose_with_adapters, compose_with_adapters_and_credential_profile},
};

const ENABLE_ENV: &str = "OXID_ENABLE_LIVE_STANDALONE_FUNDING";
const PUBLIC_BALANCE_ENABLE_ENV: &str = "OXID_ENABLE_LIVE_STANDALONE_BALANCES";
const FUNDER_SEED_ENV: &str = "OXID_STANDALONE_FUNDER_SEED_HEX";
const PREPROD_ENABLE_ENV: &str = "OXID_ENABLE_LIVE_PREPROD_E2E";
const PREPROD_MASTER_SEED_ENV: &str = "OXID_PREPROD_MASTER_SEED_HEX";
const PREPROD_CASE_INDEX_ENV: &str = "OXID_PREPROD_E2E_CASE_INDEX";
const PREPROD_COMMIT_ENV: &str = "OXID_PREPROD_E2E_COMMIT";
const PREPROD_PUBLIC_PROVER_ACK_ENV: &str = "OXID_ACKNOWLEDGE_PREPROD_PUBLIC_PROVER_PRIVACY";
const PREPROD_STATE_DIR_ENV: &str = "OXID_PREPROD_E2E_STATE_DIR";
const PREPROD_NETWORK_ID: &str = "preprod";
const PREPROD_MANIFEST_START: &str = "OXID_PREPROD_FUNDING_MANIFEST_V2";
const PREPROD_MANIFEST_END: &str = "OXID_PREPROD_FUNDING_MANIFEST_END";
const PREPROD_OBSERVATION_START: &str = "OXID_PREPROD_FUNDING_OBSERVATION_V1";
const PREPROD_OBSERVATION_END: &str = "OXID_PREPROD_FUNDING_OBSERVATION_END";
const PREPROD_PROFILE_ID: &str = "oxid-preprod-registration-e2e-2026-08";
const PREPROD_SIGNING_KEY_ID: &str = "oxid-preprod-e2e-2026-01";
const PREPROD_PROFILE_VALID_FROM_SECONDS: u64 = 1_782_864_000;
const PREPROD_PROFILE_VALID_UNTIL_SECONDS: u64 = 1_893_456_000;
const PREPROD_GENESIS_HASH: [u8; 32] = [
    0xdf, 0x83, 0x1b, 0x09, 0xa8, 0xba, 0xa9, 0x2b, 0xad, 0xf4, 0x77, 0x62, 0xce, 0x5a, 0xc4, 0x39,
    0xb7, 0xe4, 0x7e, 0x3e, 0xd3, 0xd3, 0x96, 0x00, 0xcf, 0xdd, 0x44, 0xfa, 0xd5, 0x52, 0x36, 0x1b,
];
const PREPROD_PROFILE_VERIFYING_KEY: [u8; 32] = [
    0x78, 0x67, 0x5f, 0xb8, 0x60, 0xe6, 0xcc, 0xde, 0xaa, 0xf5, 0xe4, 0xd9, 0xc2, 0x7e, 0x0a, 0xa7,
    0x80, 0xdd, 0x11, 0x7c, 0xbd, 0x58, 0x38, 0x21, 0xb4, 0x6b, 0x77, 0xb9, 0xcd, 0xfd, 0x3f, 0x5f,
];
const PREPROD_PROFILE_ENVELOPE: &[u8] =
    include_bytes!("../tests/fixtures/preprod-registration-deployment-profile.json");
const PREPROD_TRANSFER_POLICY: &str = "half_observed_shielded_night_minimum_one";
const PREPROD_EXPECTED_B_NIGHT_ATOMIC_UNITS: u128 = 0;
const PREPROD_EXPECTED_B_SHIELDED_NIGHT_ATOMIC_UNITS: u128 = 0;
const PREPROD_EXPECTED_A_ELIGIBLE_UNSHIELDED_OUTPUT_COUNT: u16 = 1;
const PREPROD_EXPECTED_A_SHIELDED_NOTE_COUNT: u64 = 1;
const PREPROD_EXPECTED_B_ELIGIBLE_UNSHIELDED_OUTPUT_COUNT: u16 = 0;
const PREPROD_EXPECTED_B_SHIELDED_NOTE_COUNT: u64 = 0;
const MAX_PREPROD_INSUFFICIENT_DUST_RETRIES: u8 = 8;
const MAX_PREPROD_CASE_INDEX: u32 = (WalletHdPathComponent::MAX_INDEX - 1) / 2;
const TRANSFER_ATOMIC_UNITS: u128 = 5_000_000;
const SHIELDED_TRANSFER_ATOMIC_UNITS: u128 = 1_000_000;
// Exact public fixture values for the images pinned in
// `scripts/standalone-stack.yml`; update the pins and this contract atomically.
const PUBLIC_GENESIS_NIGHT_ATOMIC_UNITS: u128 = 250_000_000_000_000;
const NIGHT_ATOMIC_UNITS: u128 = 1_000_000;
const DUST_ATOMIC_UNITS: u128 = 1_000_000_000_000_000;
const DUST_PER_NIGHT_AT_CAP: u128 = 5;
const PUBLIC_GENESIS_DUST_CAP_ATOMIC_UNITS: u128 = PUBLIC_GENESIS_NIGHT_ATOMIC_UNITS
    / NIGHT_ATOMIC_UNITS
    * DUST_PER_NIGHT_AT_CAP
    * DUST_ATOMIC_UNITS;
const PUBLIC_GENESIS_SHIELDED_NIGHT_ATOMIC_UNITS: u128 = 250_000_000_000_000;
const PUBLIC_GENESIS_SHIELDED_NOTE_COUNT: u64 = 7;
const NATIVE_SHIELDED_TOKEN_TYPE: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

static PATH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreprodHarnessInputError {
    MasterSeed,
    CaseIndex,
    Commit,
}

impl fmt::Display for PreprodHarnessInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MasterSeed => "the preprod master seed must contain exactly 32 hexadecimal bytes",
            Self::CaseIndex => "the preprod E2E case index is invalid",
            Self::Commit => "the preprod E2E commit identifier is invalid",
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
            return Err(PreprodHarnessInputError::CaseIndex);
        }
        let case_index = value
            .parse::<u32>()
            .map_err(|_| PreprodHarnessInputError::CaseIndex)?;
        if case_index > MAX_PREPROD_CASE_INDEX {
            return Err(PreprodHarnessInputError::CaseIndex);
        }
        let wallet_a_account_index = case_index
            .checked_mul(2)
            .ok_or(PreprodHarnessInputError::CaseIndex)?;
        let wallet_b_account_index = wallet_a_account_index
            .checked_add(1)
            .ok_or(PreprodHarnessInputError::CaseIndex)?;
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
    wallet_b_expected_night_atomic_units: u128,
    wallet_b_expected_shielded_night_atomic_units: u128,
    wallet_a_expected_eligible_unshielded_output_count: u16,
    wallet_a_expected_shielded_note_count: u64,
    wallet_b_expected_eligible_unshielded_output_count: u16,
    wallet_b_expected_shielded_note_count: u64,
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
        writeln!(formatter, "walletA.unshieldedNightRequirement=positive")?;
        writeln!(formatter, "walletA.shieldedNightRequirement=positive")?;
        writeln!(
            formatter,
            "walletA.expectedEligibleUnshieldedOutputCount={}",
            self.wallet_a_expected_eligible_unshielded_output_count
        )?;
        writeln!(
            formatter,
            "walletA.expectedShieldedNoteCount={}",
            self.wallet_a_expected_shielded_note_count
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
        writeln!(
            formatter,
            "walletB.expectedUnshieldedNightAtomicUnits={}",
            self.wallet_b_expected_night_atomic_units
        )?;
        writeln!(
            formatter,
            "walletB.expectedShieldedNightAtomicUnits={}",
            self.wallet_b_expected_shielded_night_atomic_units
        )?;
        writeln!(
            formatter,
            "walletB.expectedEligibleUnshieldedOutputCount={}",
            self.wallet_b_expected_eligible_unshielded_output_count
        )?;
        writeln!(
            formatter,
            "walletB.expectedShieldedNoteCount={}",
            self.wallet_b_expected_shielded_note_count
        )?;
        writeln!(formatter, "transfer.policy={PREPROD_TRANSFER_POLICY}")?;
        formatter.write_str(PREPROD_MANIFEST_END)
    }
}

fn parse_master_seed(value: &str) -> Result<Zeroizing<[u8; 32]>, PreprodHarnessInputError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PreprodHarnessInputError::MasterSeed);
    }
    let mut root = Zeroizing::new([0_u8; 32]);
    hex::decode_to_slice(value.as_bytes(), root.as_mut())
        .map_err(|_| PreprodHarnessInputError::MasterSeed)?;
    Ok(root)
}

fn load_preprod_master_seed() -> Result<Zeroizing<[u8; 32]>, PreprodHarnessInputError> {
    let encoded = std::env::var_os(PREPROD_MASTER_SEED_ENV)
        .and_then(|value| value.into_string().ok())
        .map(Zeroizing::new)
        .ok_or(PreprodHarnessInputError::MasterSeed)?;
    parse_master_seed(encoded.as_str())
}

fn parse_commit(value: &str) -> Result<String, PreprodHarnessInputError> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PreprodHarnessInputError::Commit);
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
        wallet_b_expected_night_atomic_units: PREPROD_EXPECTED_B_NIGHT_ATOMIC_UNITS,
        wallet_b_expected_shielded_night_atomic_units:
            PREPROD_EXPECTED_B_SHIELDED_NIGHT_ATOMIC_UNITS,
        wallet_a_expected_eligible_unshielded_output_count:
            PREPROD_EXPECTED_A_ELIGIBLE_UNSHIELDED_OUTPUT_COUNT,
        wallet_a_expected_shielded_note_count: PREPROD_EXPECTED_A_SHIELDED_NOTE_COUNT,
        wallet_b_expected_eligible_unshielded_output_count:
            PREPROD_EXPECTED_B_ELIGIBLE_UNSHIELDED_OUTPUT_COUNT,
        wallet_b_expected_shielded_note_count: PREPROD_EXPECTED_B_SHIELDED_NOTE_COUNT,
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

struct FundingStateDirectory {
    path: PathBuf,
    retain_on_drop: bool,
}

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
        Self {
            path,
            retain_on_drop: false,
        }
    }

    fn retained_preprod(case_index: u32) -> Self {
        let expected_parent = format!("case-{case_index}.started");
        let path = std::env::var_os(PREPROD_STATE_DIR_ENV)
            .map(PathBuf::from)
            .filter(|path| {
                path.is_absolute()
                    && path.file_name().and_then(|name| name.to_str()) == Some("state")
                    && path
                        .parent()
                        .and_then(|parent| parent.file_name())
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name == expected_parent)
            })
            .expect("the repository script must supply an absolute private PreProd state path");
        fs::create_dir(&path).expect("a fresh private PreProd state directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("private PreProd state directory permissions");
        }
        Self {
            path,
            retain_on_drop: true,
        }
    }

    fn account_checkpoint(&self, label: &str) -> MidnightAccountCheckpointConfig {
        MidnightAccountCheckpointConfig::new(
            self.path.join(format!("account-{label}-checkpoints.json")),
        )
        .expect("isolated account checkpoint path")
    }

    fn dust_checkpoint(&self, label: &str) -> MidnightDustCheckpointConfig {
        MidnightDustCheckpointConfig::new(self.path.join(format!("dust-{label}-checkpoints.bin")))
            .expect("isolated DUST checkpoint path")
    }

    fn journal(&self, label: &str) -> MidnightSubmissionJournalConfig {
        MidnightSubmissionJournalConfig::new(self.path.join(format!("{label}-submissions.json")))
            .expect("isolated journal path")
    }

    fn shielded_checkpoint(&self, label: &str) -> MidnightShieldedCheckpointConfig {
        MidnightShieldedCheckpointConfig::new(
            self.path.join(format!("shielded-{label}-checkpoints.bin")),
        )
        .expect("isolated shielded checkpoint path")
    }

    fn cleanup(mut self) {
        fs::remove_dir_all(&self.path).expect("isolated funding state cleanup");
        self.path = PathBuf::new();
    }
}

impl Drop for FundingStateDirectory {
    fn drop(&mut self) {
        if self.path.as_os_str().is_empty() || self.retain_on_drop {
            return;
        }
        match fs::remove_dir_all(&self.path) {
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

fn verify_preprod_profile(now_seconds: u64) -> AuthenticatedDeploymentProfile {
    let root = DeploymentTrustRoot::new(
        PREPROD_SIGNING_KEY_ID,
        PREPROD_PROFILE_VERIFYING_KEY,
        PREPROD_PROFILE_VALID_FROM_SECONDS,
        PREPROD_PROFILE_VALID_UNTIL_SECONDS,
        None,
        1,
    )
    .expect("reviewed test-only PreProd trust root");
    let verifier = DeploymentProfileVerifier::new("io.medianox.oxid", [root], 1)
        .expect("reviewed test-only PreProd verifier");
    let profile = verifier
        .verify(PREPROD_PROFILE_ENVELOPE, now_seconds)
        .expect("static signed PreProd profile must authenticate");
    assert_eq!(profile.profile_id(), PREPROD_PROFILE_ID);
    assert_eq!(profile.signing_key_id(), PREPROD_SIGNING_KEY_ID);
    assert_eq!(profile.sequence(), 1);
    assert_eq!(profile.midnight().network_id(), PREPROD_NETWORK_ID);
    assert_eq!(profile.midnight().genesis_hash(), &PREPROD_GENESIS_HASH);
    assert_eq!(
        profile.midnight().indexer_http_url(),
        "https://indexer.preprod.midnight.network/api/v4/graphql"
    );
    assert_eq!(
        profile.midnight().indexer_websocket_url(),
        "wss://indexer.preprod.midnight.network/api/v4/graphql/ws"
    );
    assert_eq!(
        profile.midnight().node_websocket_url(),
        "wss://rpc.preprod.midnight.network"
    );
    assert_eq!(
        profile.midnight().proof_server_url(),
        "https://lace-proof-pub.preprod.midnight.network"
    );
    for inert_ssi_route in [
        profile.ssi().did_resolver_url(),
        profile.ssi().issuer_metadata_url(),
        profile.ssi().verifier_metadata_url(),
    ] {
        assert!(
            inert_ssi_route.contains(".invalid"),
            "the Midnight-only test profile must not claim an SSI deployment"
        );
    }
    profile
}

fn authenticated_preprod_config() -> MidnightStandaloneConfig {
    let now_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_secs();
    let profile = verify_preprod_profile(now_seconds);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("PreProd authentication runtime");
    let deployment = runtime
        .block_on(super::authenticate_production_deployment(profile))
        .expect("signed PreProd profile must match the live node genesis");
    assert_eq!(deployment.profile().profile_id(), PREPROD_PROFILE_ID);
    deployment.midnight
}

fn compose_live<N>(
    config: MidnightStandaloneConfig,
    profiles: Arc<InMemoryWalletProfileRepository>,
    security: Arc<DevelopmentWalletSecurity<SystemClock, N>>,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    journal: Option<MidnightSubmissionJournalConfig>,
) -> ApplicationServices
where
    N: RandomPort + 'static,
{
    compose_live_with_protection(
        config,
        profiles,
        security,
        account_checkpoints,
        dust_checkpoints,
        shielded_checkpoints,
        journal,
        |security| security,
    )
}

#[allow(clippy::too_many_arguments)]
fn compose_live_with_protection<N, F>(
    config: MidnightStandaloneConfig,
    profiles: Arc<InMemoryWalletProfileRepository>,
    security: Arc<DevelopmentWalletSecurity<SystemClock, N>>,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    journal: Option<MidnightSubmissionJournalConfig>,
    protection_for_security: F,
) -> ApplicationServices
where
    N: RandomPort + 'static,
    F: FnOnce(Arc<DevelopmentWalletSecurity<SystemClock, N>>) -> Arc<dyn WalletProtectionPort>,
{
    let clock = Arc::new(SystemClock);
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_checkpoint_options(
            config,
            account_checkpoints,
            dust_checkpoints,
            shielded_checkpoints,
            journal,
            clock,
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters_and_credential_profile(
        profiles,
        security,
        midnight,
        CredentialPresentationComposition::Standalone,
        HeadlessCredentialProfile::Standalone,
        protection_for_security,
    )
}

fn initialize_account(
    application: &ApplicationServices,
    name: &str,
    network_id: &str,
    account_index: u32,
) -> (String, String, String) {
    let profile = application
        .create_wallet_profile()
        .execute(CreateWalletProfileCommand {
            display_name: name.to_owned(),
        })
        .expect("profile creation");
    let selected = application
        .select_wallet_network()
        .execute(SelectWalletNetworkCommand {
            profile_id: profile.id.clone(),
            network_id: network_id.to_owned(),
        })
        .expect("wallet network selection");
    assert_eq!(selected.selected_network_id, network_id);
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
            account_index,
            address_index: 0,
        })
        .expect("protected account derivation");
    assert_eq!(account.network_id, network_id);
    assert_eq!(account.account_index, account_index);
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

fn synchronize_dust(application: &ApplicationServices, profile_id: &str) -> WalletDustSyncView {
    synchronize_dust_with_timeout(application, profile_id, Duration::from_secs(120))
}

fn synchronize_dust_with_timeout(
    application: &ApplicationServices,
    profile_id: &str,
    timeout: Duration,
) -> WalletDustSyncView {
    application
        .start_wallet_dust_sync()
        .execute(WalletDustSyncCommand {
            profile_id: profile_id.to_owned(),
        })
        .expect("DUST synchronization starts");
    let deadline = Instant::now() + timeout;
    loop {
        let status = application
            .get_wallet_dust_sync_status()
            .execute(WalletDustSyncCommand {
                profile_id: profile_id.to_owned(),
            })
            .expect("DUST synchronization status");
        match status.state.as_str() {
            "synced" => return status,
            "syncing" | "cached" => {}
            state => panic!(
                "DUST synchronization stopped in state {state} with failure {:?}",
                status.failure
            ),
        }
        assert!(
            Instant::now() < deadline,
            "DUST synchronization did not finish within {timeout:?}: state={}, current_cursor={:?}, target_cursor={:?}, events_processed={}, failure={:?}",
            status.state,
            status.current_cursor,
            status.target_cursor,
            status.events_processed,
            status.failure
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn dust_balance(status: &WalletDustSyncView) -> u128 {
    status
        .balance_atomic_units
        .as_deref()
        .unwrap_or("0")
        .parse::<u128>()
        .expect("exact DUST balance")
}

fn await_dust_balance_at_least(
    application: &ApplicationServices,
    profile_id: &str,
    minimum: u128,
    deadline: Instant,
) -> WalletDustSyncView {
    loop {
        let status = synchronize_dust(application, profile_id);
        if dust_balance(&status) >= minimum {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "the generated-DUST observation deadline elapsed before the reviewed threshold"
        );
        std::thread::sleep(Duration::from_secs(15));
    }
}

fn await_preprod_registration_preview(
    application: &ApplicationServices,
    profile_id: &str,
) -> WalletDustRegistrationPreviewView {
    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    loop {
        match application.prepare_wallet_dust_registration().execute(
            PrepareWalletDustRegistrationCommand {
                profile_id: profile_id.to_owned(),
            },
        ) {
            Ok(preview) => return preview,
            Err(WalletDustRegistrationError::Operation(
                WalletDustRegistrationPortError::InsufficientRegistrationAllowance,
            )) => {
                assert!(
                    Instant::now() < deadline,
                    "fresh NIGHT did not accrue a non-zero registration allowance within 15 minutes"
                );
                std::thread::sleep(Duration::from_secs(15));
            }
            Err(error) => panic!("protected DUST registration preparation failed: {error}"),
        }
    }
}

fn observe_registration_readiness(
    application: &ApplicationServices,
    profile_id: &str,
) -> (&'static str, Option<u16>, Option<String>) {
    match application.prepare_wallet_dust_registration().execute(
        PrepareWalletDustRegistrationCommand {
            profile_id: profile_id.to_owned(),
        },
    ) {
        Ok(preview) => (
            "prepared",
            Some(preview.input_count),
            Some(preview.registered_night.atomic_units),
        ),
        Err(WalletDustRegistrationError::Operation(
            WalletDustRegistrationPortError::NoEligibleNight,
        )) => ("no_eligible_night", Some(0), Some("0".to_owned())),
        Err(WalletDustRegistrationError::Operation(
            WalletDustRegistrationPortError::RegistrationAlreadyCurrent,
        )) => ("already_registered", None, None),
        Err(WalletDustRegistrationError::Operation(
            WalletDustRegistrationPortError::InsufficientRegistrationAllowance,
        )) => ("allowance_pending", None, None),
        Err(error) => panic!("read-only registration readiness observation failed: {error}"),
    }
}

fn preprod_shielded_transfer_amount(observed_balance: u128) -> Option<u128> {
    (observed_balance > 0).then(|| (observed_balance / 2).max(1))
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
    assert!(
        status.is_complete(),
        "incomplete shielded synchronization snapshot: {status:?}"
    );
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
            Err(PreprodHarnessInputError::CaseIndex)
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
        assert_eq!(error, PreprodHarnessInputError::MasterSeed);
        if !invalid.is_empty() {
            assert!(!error.to_string().contains(invalid));
        }
    }

    assert!(parse_commit(&"a".repeat(40)).is_ok());
    assert!(parse_commit(&"b".repeat(64)).is_ok());
    for invalid in ["", "A", &"A".repeat(40), &"g".repeat(40)] {
        let error = parse_commit(invalid).expect_err("invalid commit must fail closed");
        assert_eq!(error, PreprodHarnessInputError::Commit);
        if !invalid.is_empty() {
            assert!(!error.to_string().contains(invalid));
        }
    }
}

#[test]
fn preprod_midnight_only_profile_is_signed_static_and_test_scoped() {
    let profile = verify_preprod_profile(1_800_000_000);
    assert_eq!(
        profile.valid_until_seconds(),
        PREPROD_PROFILE_VALID_UNTIL_SECONDS
    );
    let debug = format!("{profile:?} {:?}", profile.midnight());
    assert!(!debug.contains("indexer.preprod.midnight.network"));
    assert!(!debug.contains("lace-proof-pub.preprod.midnight.network"));
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
            "walletA.unshieldedNightRequirement",
            "walletA.shieldedNightRequirement",
            "walletA.expectedEligibleUnshieldedOutputCount",
            "walletA.expectedShieldedNoteCount",
            "walletB.accountIndex",
            "walletB.addressIndex",
            "walletB.nightUnshieldedAddress",
            "walletB.nightShieldedAddress",
            "walletB.expectedUnshieldedNightAtomicUnits",
            "walletB.expectedShieldedNightAtomicUnits",
            "walletB.expectedEligibleUnshieldedOutputCount",
            "walletB.expectedShieldedNoteCount",
            "transfer.policy",
        ]
    );
    assert!(rendered.starts_with(PREPROD_MANIFEST_START));
    assert!(rendered.ends_with(PREPROD_MANIFEST_END));
    assert!(rendered.contains("network=preprod"));
    assert!(rendered.contains("walletA.accountIndex=14"));
    assert!(rendered.contains("walletB.accountIndex=15"));
    assert!(rendered.contains("walletA.unshieldedNightRequirement=positive"));
    assert!(rendered.contains("walletA.shieldedNightRequirement=positive"));
    assert!(rendered.contains("walletA.expectedEligibleUnshieldedOutputCount=1"));
    assert!(rendered.contains("walletA.expectedShieldedNoteCount=1"));
    assert!(rendered.contains("walletB.expectedUnshieldedNightAtomicUnits=0"));
    assert!(rendered.contains("walletB.expectedShieldedNightAtomicUnits=0"));
    assert!(rendered.contains("walletB.expectedEligibleUnshieldedOutputCount=0"));
    assert!(rendered.contains("walletB.expectedShieldedNoteCount=0"));
    assert!(rendered.contains("transfer.policy=half_observed_shielded_night_minimum_one"));
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

#[test]
fn preprod_transfer_policy_is_positive_bounded_and_amount_observed() {
    assert_eq!(preprod_shielded_transfer_amount(0), None);
    assert_eq!(preprod_shielded_transfer_amount(1), Some(1));
    assert_eq!(preprod_shielded_transfer_amount(2), Some(1));
    assert_eq!(preprod_shielded_transfer_amount(3), Some(1));
    assert_eq!(preprod_shielded_transfer_amount(4), Some(2));
    assert_eq!(
        preprod_shielded_transfer_amount(u128::MAX),
        Some(u128::MAX / 2)
    );
}

#[test]
fn public_balance_contract_tracks_the_exact_standalone_image_and_preset_pins() {
    let manifest = include_str!("../../../scripts/standalone-stack.yml");
    for required_setting in [
        "midnightntwrk/indexer-standalone:4.0.0",
        "midnightntwrk/midnight-node:0.22.3",
        "midnightntwrk/proof-server:8.0.3",
        "APP__APPLICATION__NETWORK_ID: \"undeployed\"",
        "CFG_PRESET: \"dev\"",
    ] {
        assert!(
            manifest.contains(required_setting),
            "standalone stack pin changed without updating the exact public balance contract: {required_setting}"
        );
    }
}

/// Synchronizes all three independent balance projections for the public
/// undeployed genesis wallet without accepting or emitting private input.
///
/// This is ignored because it requires the repository-owned standalone stack.
/// The image pins plus the node's pinned `dev` preset define the exact genesis
/// allocations and note count. The pinned genesis time places its generating
/// NIGHT at the protocol's five-DUST-per-NIGHT cap, so chain uptime cannot
/// increase this projection further. Restart the stack before the check if
/// another explicitly authorized test has spent the shared public fixture or
/// changed its notes.
#[test]
#[ignore = "requires explicit live standalone stack"]
fn public_standalone_genesis_balances_are_exact() {
    const SHARED_FIXTURE_DRIFT: &str = "shared public fixture differs from genesis; restart the standalone stack before treating this as a regression";
    assert_eq!(
        std::env::var(PUBLIC_BALANCE_ENABLE_ENV).ok().as_deref(),
        Some("1"),
        "live standalone balance proof requires explicit opt-in"
    );
    let config = standalone_config();
    let profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    let protection_profiles = Arc::clone(&profiles);
    let application = compose_live_with_protection(
        config,
        profiles,
        security,
        None,
        None,
        None,
        None,
        move |security| {
            Arc::new(
                public_profile_protection("undeployed", protection_profiles, security)
                    .expect("undeployed fixture protection"),
            )
        },
    );
    let (profile_id, _, _) = initialize_account(
        &application,
        PUBLIC_STANDALONE_PROFILE_NAME,
        "undeployed",
        0,
    );

    let shielded = synchronize_shielded(&application, &profile_id);
    assert_complete_shielded_snapshot(&shielded);
    assert_eq!(
        shielded.owned_note_count,
        Some(PUBLIC_GENESIS_SHIELDED_NOTE_COUNT),
        "{SHARED_FIXTURE_DRIFT}"
    );
    assert_eq!(
        live_night_balance(&application, &profile_id),
        PUBLIC_GENESIS_NIGHT_ATOMIC_UNITS,
        "{SHARED_FIXTURE_DRIFT}"
    );
    let dust = synchronize_dust(&application, &profile_id);
    assert_eq!(dust.state, "synced");
    assert_eq!(dust.failure, None);
    assert_eq!(dust.current_cursor, dust.target_cursor);
    assert_eq!(
        dust_balance(&dust),
        PUBLIC_GENESIS_DUST_CAP_ATOMIC_UNITS,
        "{SHARED_FIXTURE_DRIFT}"
    );
    assert_eq!(
        shielded_balance(&shielded, NATIVE_SHIELDED_TOKEN_TYPE),
        PUBLIC_GENESIS_SHIELDED_NIGHT_ATOMIC_UNITS,
        "{SHARED_FIXTURE_DRIFT}"
    );
}

/// Derives the only public values required to fund a deterministic pair of
/// preprod accounts. This deliberately performs no network I/O. The separate
/// ignored live test consumes its build-reviewed signed profile without
/// runtime route or trust-root selection.
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
        .map_err(|_| PreprodHarnessInputError::CaseIndex)
        .and_then(|value| PreprodCase::parse(&value))
        .expect("bounded preprod case index");
    let commit = std::env::var(PREPROD_COMMIT_ENV)
        .map_err(|_| PreprodHarnessInputError::Commit)
        .and_then(|value| parse_commit(&value))
        .expect("exact preprod harness commit");
    let manifest = build_preprod_funding_manifest(&root, selected_case, commit);
    println!("{manifest}");
}

/// Reads the deterministic PreProd A/B funding topology without authorization,
/// proof, persistence, broadcast, or a single-use case marker. Registration
/// readiness retains one unsigned process-local draft that is discarded with
/// the test process. Emitted fields are public aggregate observations only;
/// address derivation is checked without reproducing seed material.
#[test]
#[ignore = "requires explicit preprod opt-in, an out-of-band master seed, and live indexer reads"]
fn preprod_funding_observation_is_read_only() {
    assert_eq!(
        std::env::var(PREPROD_ENABLE_ENV).ok().as_deref(),
        Some("1"),
        "read-only PreProd observation requires explicit opt-in"
    );
    let root = load_preprod_master_seed().expect("preprod master seed input");
    let selected_case = std::env::var(PREPROD_CASE_INDEX_ENV)
        .map_err(|_| PreprodHarnessInputError::CaseIndex)
        .and_then(|value| PreprodCase::parse(&value))
        .expect("bounded PreProd case index");
    let commit = std::env::var(PREPROD_COMMIT_ENV)
        .map_err(|_| PreprodHarnessInputError::Commit)
        .and_then(|value| parse_commit(&value))
        .expect("exact PreProd observation commit");
    let expected_manifest = build_preprod_funding_manifest(&root, selected_case, commit.clone());
    let config = authenticated_preprod_config();

    let wallet_a_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let wallet_a_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OneShotRootRandom::new(copy_root(&root))),
    ));
    let wallet_a = compose_live(
        config.clone(),
        wallet_a_profiles,
        wallet_a_security,
        None,
        None,
        None,
        None,
    );
    let (wallet_a_profile_id, wallet_a_night_address, wallet_a_shielded_address) =
        initialize_account(
            &wallet_a,
            "PreProd observation wallet A",
            PREPROD_NETWORK_ID,
            selected_case.wallet_a_account_index,
        );
    assert_eq!(
        wallet_a_night_address,
        expected_manifest.wallet_a.night_unshielded_address
    );
    assert_eq!(
        wallet_a_shielded_address,
        expected_manifest.wallet_a.night_shielded_address
    );

    let wallet_b_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let wallet_b_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OneShotRootRandom::new(copy_root(&root))),
    ));
    let wallet_b = compose_live(
        config,
        wallet_b_profiles,
        wallet_b_security,
        None,
        None,
        None,
        None,
    );
    let (wallet_b_profile_id, wallet_b_night_address, wallet_b_shielded_address) =
        initialize_account(
            &wallet_b,
            "PreProd observation wallet B",
            PREPROD_NETWORK_ID,
            selected_case.wallet_b_account_index,
        );
    assert_eq!(
        wallet_b_night_address,
        expected_manifest.wallet_b.night_unshielded_address
    );
    assert_eq!(
        wallet_b_shielded_address,
        expected_manifest.wallet_b.night_shielded_address
    );

    let wallet_a_night = live_night_balance(&wallet_a, &wallet_a_profile_id);
    let wallet_b_night = live_night_balance(&wallet_b, &wallet_b_profile_id);
    let wallet_a_shielded = synchronize_shielded(&wallet_a, &wallet_a_profile_id);
    let wallet_b_shielded = synchronize_shielded(&wallet_b, &wallet_b_profile_id);
    let wallet_a_dust = synchronize_dust_with_timeout(
        &wallet_a,
        &wallet_a_profile_id,
        Duration::from_secs(15 * 60),
    );
    let wallet_b_dust = synchronize_dust_with_timeout(
        &wallet_b,
        &wallet_b_profile_id,
        Duration::from_secs(15 * 60),
    );
    assert_complete_shielded_snapshot(&wallet_a_shielded);
    assert_complete_shielded_snapshot(&wallet_b_shielded);
    assert_eq!(wallet_a_dust.state, "synced");
    assert_eq!(wallet_a_dust.failure, None);
    assert_eq!(wallet_b_dust.state, "synced");
    assert_eq!(wallet_b_dust.failure, None);
    let (wallet_a_registration, wallet_a_input_count, wallet_a_registered_night) =
        observe_registration_readiness(&wallet_a, &wallet_a_profile_id);
    let (wallet_b_registration, wallet_b_input_count, wallet_b_registered_night) =
        observe_registration_readiness(&wallet_b, &wallet_b_profile_id);

    println!("{PREPROD_OBSERVATION_START}");
    println!("commit={commit}");
    println!("network={PREPROD_NETWORK_ID}");
    println!("caseIndex={}", selected_case.case_index);
    println!("walletA.unshieldedNightAtomicUnits={wallet_a_night}");
    println!(
        "walletA.shieldedNightAtomicUnits={}",
        shielded_balance(&wallet_a_shielded, NATIVE_SHIELDED_TOKEN_TYPE)
    );
    println!(
        "walletA.shieldedNoteCount={}",
        wallet_a_shielded
            .owned_note_count
            .expect("complete A note count")
    );
    println!("walletA.dustAtomicUnits={}", dust_balance(&wallet_a_dust));
    println!("walletA.registrationStatus={wallet_a_registration}");
    println!(
        "walletA.eligibleUnshieldedOutputCount={}",
        wallet_a_input_count.map_or_else(|| "unknown".to_owned(), |count| count.to_string())
    );
    println!(
        "walletA.registeredNightAtomicUnits={}",
        wallet_a_registered_night.unwrap_or_else(|| "unknown".to_owned())
    );
    println!("walletB.unshieldedNightAtomicUnits={wallet_b_night}");
    println!(
        "walletB.shieldedNightAtomicUnits={}",
        shielded_balance(&wallet_b_shielded, NATIVE_SHIELDED_TOKEN_TYPE)
    );
    println!(
        "walletB.shieldedNoteCount={}",
        wallet_b_shielded
            .owned_note_count
            .expect("complete B note count")
    );
    println!("walletB.dustAtomicUnits={}", dust_balance(&wallet_b_dust));
    println!("walletB.registrationStatus={wallet_b_registration}");
    println!(
        "walletB.eligibleUnshieldedOutputCount={}",
        wallet_b_input_count.map_or_else(|| "unknown".to_owned(), |count| count.to_string())
    );
    println!(
        "walletB.registeredNightAtomicUnits={}",
        wallet_b_registered_night.unwrap_or_else(|| "unknown".to_owned())
    );
    println!("{PREPROD_OBSERVATION_END}");
}

/// Proves the complete fresh-wallet PreProd journey with deterministic,
/// externally funded test accounts and the same application ports used by the
/// mobile shell. The committed profile authenticates only the Midnight routes;
/// its inert `.invalid` SSI routes are never composed or contacted here.
///
/// The public prover receives private proof preimages over TLS. This ignored
/// interoperability test therefore requires a separate explicit privacy
/// acknowledgement and is not production privacy evidence. The repository
/// script withholds the master seed from Cargo/build scripts and supplies it
/// only to the compiled observer and write-test processes; this module never
/// logs or persists it.
#[test]
#[ignore = "requires funded PreProd A/B accounts, public-prover acknowledgement, and explicit opt-in"]
fn preprod_funded_registration_observes_dust_and_spends_shielded_night() {
    assert_eq!(
        std::env::var(PREPROD_ENABLE_ENV).ok().as_deref(),
        Some("1"),
        "live PreProd registration requires explicit opt-in"
    );
    assert_eq!(
        std::env::var(PREPROD_PUBLIC_PROVER_ACK_ENV).ok().as_deref(),
        Some("1"),
        "the public PreProd prover privacy tradeoff requires explicit acknowledgement"
    );
    let root = load_preprod_master_seed().expect("preprod master seed input");
    let selected_case = std::env::var(PREPROD_CASE_INDEX_ENV)
        .map_err(|_| PreprodHarnessInputError::CaseIndex)
        .and_then(|value| PreprodCase::parse(&value))
        .expect("bounded PreProd case index");
    let commit = std::env::var(PREPROD_COMMIT_ENV)
        .map_err(|_| PreprodHarnessInputError::Commit)
        .and_then(|value| parse_commit(&value))
        .expect("exact PreProd harness commit");
    let expected_manifest = build_preprod_funding_manifest(&root, selected_case, commit);
    let config = authenticated_preprod_config();
    let state = FundingStateDirectory::retained_preprod(selected_case.case_index);

    let wallet_a_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let wallet_a_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OneShotRootRandom::new(copy_root(&root))),
    ));
    let wallet_a = compose_live(
        config.clone(),
        Arc::clone(&wallet_a_profiles),
        Arc::clone(&wallet_a_security),
        Some(state.account_checkpoint("preprod-wallet-a")),
        Some(state.dust_checkpoint("preprod-wallet-a")),
        Some(state.shielded_checkpoint("preprod-wallet-a")),
        Some(state.journal("preprod-wallet-a")),
    );
    let (wallet_a_profile_id, wallet_a_night_address, wallet_a_shielded_address) =
        initialize_account(
            &wallet_a,
            "PreProd E2E wallet A",
            PREPROD_NETWORK_ID,
            selected_case.wallet_a_account_index,
        );

    let wallet_b_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let wallet_b_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OneShotRootRandom::new(copy_root(&root))),
    ));
    let wallet_b = compose_live(
        config.clone(),
        Arc::clone(&wallet_b_profiles),
        Arc::clone(&wallet_b_security),
        Some(state.account_checkpoint("preprod-wallet-b")),
        Some(state.dust_checkpoint("preprod-wallet-b")),
        Some(state.shielded_checkpoint("preprod-wallet-b")),
        Some(state.journal("preprod-wallet-b")),
    );
    let (wallet_b_profile_id, wallet_b_night_address, wallet_b_shielded_address) =
        initialize_account(
            &wallet_b,
            "PreProd E2E wallet B",
            PREPROD_NETWORK_ID,
            selected_case.wallet_b_account_index,
        );

    assert_eq!(
        wallet_a_night_address,
        expected_manifest.wallet_a.night_unshielded_address
    );
    assert_eq!(
        wallet_a_shielded_address,
        expected_manifest.wallet_a.night_shielded_address
    );
    assert_eq!(
        wallet_b_night_address,
        expected_manifest.wallet_b.night_unshielded_address
    );
    assert_eq!(
        wallet_b_shielded_address,
        expected_manifest.wallet_b.night_shielded_address
    );
    let wallet_a_night_before = live_night_balance(&wallet_a, &wallet_a_profile_id);
    assert!(
        wallet_a_night_before > 0,
        "wallet A must receive positive unshielded NIGHT funding"
    );
    assert_eq!(
        live_night_balance(&wallet_b, &wallet_b_profile_id),
        PREPROD_EXPECTED_B_NIGHT_ATOMIC_UNITS,
        "wallet B must begin with no public NIGHT funding"
    );
    assert_eq!(
        wallet_b
            .prepare_wallet_dust_registration()
            .execute(PrepareWalletDustRegistrationCommand {
                profile_id: wallet_b_profile_id.clone(),
            }),
        Err(WalletDustRegistrationError::Operation(
            WalletDustRegistrationPortError::NoEligibleNight
        )),
        "wallet B must begin with no eligible unshielded NIGHT output"
    );

    let wallet_a_shielded_before = synchronize_shielded(&wallet_a, &wallet_a_profile_id);
    assert_complete_shielded_snapshot(&wallet_a_shielded_before);
    let wallet_a_shielded_night_before =
        shielded_balance(&wallet_a_shielded_before, NATIVE_SHIELDED_TOKEN_TYPE);
    assert!(
        wallet_a_shielded_night_before > 0,
        "wallet A must receive positive shielded NIGHT funding"
    );
    assert_eq!(
        wallet_a_shielded_before.owned_note_count,
        Some(PREPROD_EXPECTED_A_SHIELDED_NOTE_COUNT),
        "wallet A funding must be exactly one shielded NIGHT note"
    );
    let wallet_b_shielded_before = synchronize_shielded(&wallet_b, &wallet_b_profile_id);
    assert_complete_shielded_snapshot(&wallet_b_shielded_before);
    assert_eq!(
        shielded_balance(&wallet_b_shielded_before, NATIVE_SHIELDED_TOKEN_TYPE),
        PREPROD_EXPECTED_B_SHIELDED_NIGHT_ATOMIC_UNITS,
        "wallet B must begin as an empty shielded recipient"
    );
    assert_eq!(
        wallet_b_shielded_before.owned_note_count,
        Some(PREPROD_EXPECTED_B_SHIELDED_NOTE_COUNT),
        "wallet B must begin with no shielded notes"
    );

    let initial_dust = synchronize_dust_with_timeout(
        &wallet_a,
        &wallet_a_profile_id,
        Duration::from_secs(15 * 60),
    );
    assert_eq!(initial_dust.state, "synced");
    assert_eq!(initial_dust.current_cursor, initial_dust.target_cursor);
    assert_eq!(
        dust_balance(&initial_dust),
        0,
        "a fresh funded wallet intentionally begins with zero DUST"
    );

    let prepared = await_preprod_registration_preview(&wallet_a, &wallet_a_profile_id);
    assert_eq!(prepared.state, "prepared");
    assert!(prepared.authorization_ready);
    assert!(!prepared.submission_ready);
    assert_eq!(prepared.network_id, PREPROD_NETWORK_ID);
    assert_eq!(
        prepared.registered_night.atomic_units,
        wallet_a_night_before.to_string(),
        "the one eligible output must register the exact observed NIGHT principal"
    );
    assert_eq!(
        prepared.input_count, PREPROD_EXPECTED_A_ELIGIBLE_UNSHIELDED_OUTPUT_COUNT,
        "wallet A funding must be exactly one eligible unshielded NIGHT output"
    );
    assert!(
        prepared
            .maximum_fee_allowance
            .atomic_units
            .parse::<u128>()
            .expect("exact registration allowance")
            > 0
    );
    assert_eq!(prepared.fee_state, "requires_balancing");

    let authorized = wallet_a
        .authorize_wallet_dust_registration()
        .execute(AuthorizeWalletDustRegistrationCommand {
            profile_id: wallet_a_profile_id.clone(),
            draft_id: prepared.draft_id.clone(),
            authorization_challenge: prepared.authorization_challenge.clone(),
            confirmation: SensitiveOperationConfirmation {
                title: "Authorize PreProd DUST registration".to_owned(),
                summary: "Register wallet A's exact reviewed NIGHT for DUST generation".to_owned(),
                confirmed: true,
            },
        })
        .expect("explicit DUST registration authorization");
    assert_eq!(authorized.state, "authorized");
    assert!(authorized.submission_ready);

    let submitted =
        futures::executor::block_on(wallet_a.submit_wallet_dust_registration().execute(
            SubmitWalletDustRegistrationCommand {
                profile_id: wallet_a_profile_id.clone(),
                draft_id: prepared.draft_id.clone(),
                confirmation: SensitiveOperationConfirmation {
                    title: "Submit PreProd DUST registration".to_owned(),
                    summary: "Prove and submit wallet A's authorized DUST registration".to_owned(),
                    confirmed: true,
                },
            },
        ))
        .expect("PreProd DUST registration proof, submission, and finality");
    assert_eq!(submitted.mode, "live");
    assert_eq!(submitted.registration.state, "submitted");
    assert_eq!(submitted.registration_observation, "included");
    assert_eq!(submitted.dust_readiness, "requires_synchronization");
    assert!(!submitted.transaction_id.is_empty());
    assert!(!submitted.block_id.is_empty());

    let included = wallet_a
        .get_wallet_dust_registration_status()
        .execute(GetWalletDustRegistrationStatusCommand {
            profile_id: wallet_a_profile_id.clone(),
            draft_id: prepared.draft_id.clone(),
        })
        .expect("included registration status");
    assert_eq!(included.state, "included");
    assert_eq!(included.registration_observation, "included");
    assert_eq!(included.dust_readiness, "requires_synchronization");
    assert!(!included.reconciliation_allowed);
    assert_eq!(
        live_night_balance(&wallet_a, &wallet_a_profile_id),
        wallet_a_night_before,
        "registration returns the exact NIGHT principal to wallet A"
    );

    let registration_fee = submitted
        .fee
        .atomic_units
        .parse::<u128>()
        .expect("exact registration fee");
    assert!(
        registration_fee > 0,
        "registration must report its exact nonzero fee"
    );
    // A positive balance is only a liveness observation, not a quote for the
    // later shielded transfer. Exact fee balancing happens inside the adapter
    // immediately before proving and broadcast.
    let dust_spend_deadline = Instant::now() + Duration::from_secs(15 * 60);
    let spend_readiness_threshold = 1;
    let generated_dust = await_dust_balance_at_least(
        &wallet_a,
        &wallet_a_profile_id,
        spend_readiness_threshold,
        dust_spend_deadline,
    );
    assert_eq!(generated_dust.state, "synced");
    assert_eq!(generated_dust.current_cursor, generated_dust.target_cursor);
    assert!(dust_balance(&generated_dust) >= spend_readiness_threshold);
    drop(wallet_a);

    let reconstructed_a = compose_live(
        config.clone(),
        Arc::clone(&wallet_a_profiles),
        Arc::clone(&wallet_a_security),
        Some(state.account_checkpoint("preprod-wallet-a")),
        Some(state.dust_checkpoint("preprod-wallet-a")),
        Some(state.shielded_checkpoint("preprod-wallet-a")),
        Some(state.journal("preprod-wallet-a")),
    );
    let restored_registration = futures::executor::block_on(
        reconstructed_a
            .reconcile_wallet_dust_registration_submission()
            .execute(ReconcileWalletDustRegistrationSubmissionCommand {
                profile_id: wallet_a_profile_id.clone(),
                draft_id: prepared.draft_id,
            }),
    )
    .expect("included registration restoration from the public journal");
    assert_eq!(restored_registration.state, "included");
    assert_eq!(restored_registration.registration_observation, "included");
    assert_eq!(
        restored_registration.dust_readiness,
        "requires_synchronization"
    );
    let reconstructed_dust = synchronize_dust(&reconstructed_a, &wallet_a_profile_id);
    assert_eq!(reconstructed_dust.state, "synced");
    assert_eq!(
        reconstructed_dust.current_cursor,
        reconstructed_dust.target_cursor
    );
    assert!(
        reconstructed_dust.current_cursor >= generated_dust.current_cursor,
        "adapter reconstruction must authoritatively resynchronize from no earlier DUST cursor"
    );
    assert!(
        dust_balance(&reconstructed_dust) >= dust_balance(&generated_dust),
        "adapter reconstruction plus authoritative resynchronization must preserve generated DUST"
    );

    let shielded_transfer_atomic_units =
        preprod_shielded_transfer_amount(wallet_a_shielded_night_before)
            .expect("positive observed shielded balance");
    let wallet_a_expected_shielded_night_after = wallet_a_shielded_night_before
        .checked_sub(shielded_transfer_atomic_units)
        .expect("the deterministic shielded transfer is bounded by the observed balance");
    let transfer = reconstructed_a
        .prepare_shielded_wallet_transfer()
        .execute(PrepareShieldedWalletTransferCommand {
            profile_id: wallet_a_profile_id.clone(),
            recipient_address: wallet_b_shielded_address,
            token_type: NATIVE_SHIELDED_TOKEN_TYPE.to_owned(),
            amount_atomic_units: shielded_transfer_atomic_units.to_string(),
        })
        .expect("deterministic PreProd shielded preview after DUST recovery");
    assert_eq!(transfer.state, "prepared");
    assert_eq!(
        transfer.amount.atomic_units,
        shielded_transfer_atomic_units.to_string()
    );
    assert_eq!(transfer.input_count, 1);
    assert_eq!(transfer.recipient_kind, "shielded");
    let authorized_transfer = reconstructed_a
        .authorize_wallet_transfer()
        .execute(AuthorizeWalletTransferCommand {
            profile_id: wallet_a_profile_id.clone(),
            draft_id: transfer.draft_id.clone(),
            authorization_challenge: transfer.authorization_challenge.clone(),
            confirmation: SensitiveOperationConfirmation {
                title: "Authorize PreProd shielded transfer".to_owned(),
                summary: "Send the deterministic observed-balance share from A to empty B"
                    .to_owned(),
                confirmed: true,
            },
        })
        .expect("explicit PreProd shielded transfer authorization");
    assert_eq!(authorized_transfer.state, "authorized");
    let mut observed_dust = reconstructed_dust;
    let mut insufficient_dust_retries = 0_u8;
    let submitted_transfer = loop {
        assert!(
            Instant::now() < dust_spend_deadline,
            "shielded fee balancing did not become ready within 15 minutes"
        );
        let result = futures::executor::block_on(reconstructed_a.submit_wallet_transfer().execute(
            SubmitWalletTransferCommand {
                profile_id: wallet_a_profile_id.clone(),
                draft_id: transfer.draft_id.clone(),
                confirmation: SensitiveOperationConfirmation {
                    title: "Submit PreProd shielded transfer".to_owned(),
                    summary: "Prove and submit the authorized A-to-B shielded transfer".to_owned(),
                    confirmed: true,
                },
            },
        ));
        match result {
            Ok(submitted) => break submitted,
            Err(WalletTransactionError::Operation(
                WalletTransactionPortError::InsufficientDust,
            )) => {
                insufficient_dust_retries = insufficient_dust_retries
                    .checked_add(1)
                    .expect("bounded insufficient-DUST retries");
                assert!(
                    insufficient_dust_retries <= MAX_PREPROD_INSUFFICIENT_DUST_RETRIES,
                    "shielded fee balancing remained underfunded after bounded pre-broadcast waits"
                );
                let retained = reconstructed_a
                    .get_wallet_transfer_draft()
                    .execute(WalletTransferDraftQuery {
                        profile_id: wallet_a_profile_id.clone(),
                        draft_id: transfer.draft_id.clone(),
                    })
                    .expect("insufficient DUST retains the exact authorized draft");
                assert_eq!(retained.state, "authorized");
                assert!(retained.submission_ready);
                let next_threshold = dust_balance(&observed_dust)
                    .checked_add(1)
                    .expect("next DUST observation threshold is exact");
                observed_dust = await_dust_balance_at_least(
                    &reconstructed_a,
                    &wallet_a_profile_id,
                    next_threshold,
                    dust_spend_deadline,
                );
            }
            Err(error) => panic!(
                "PreProd shielded proof, submission, and finalized inclusion failed: {error}"
            ),
        }
    };
    assert_eq!(submitted_transfer.mode, "live");
    assert_eq!(submitted_transfer.transfer.state, "submitted");
    assert!(!submitted_transfer.transaction_id.is_empty());
    assert!(!submitted_transfer.block_id.is_empty());
    let transfer_transaction_id = submitted_transfer.transaction_id.clone();

    let duplicate = reconstructed_a.prepare_shielded_wallet_transfer().execute(
        PrepareShieldedWalletTransferCommand {
            profile_id: wallet_a_profile_id.clone(),
            recipient_address: transfer.recipient_address.clone(),
            token_type: NATIVE_SHIELDED_TOKEN_TYPE.to_owned(),
            amount_atomic_units: shielded_transfer_atomic_units.to_string(),
        },
    );
    assert_eq!(
        duplicate,
        Err(WalletTransactionError::Operation(
            WalletTransactionPortError::DraftConflict
        )),
        "the included journal barrier must block duplicate delivery before replay"
    );
    drop(reconstructed_a);

    let restored_a = compose_live(
        config,
        wallet_a_profiles,
        wallet_a_security,
        Some(state.account_checkpoint("preprod-wallet-a")),
        Some(state.dust_checkpoint("preprod-wallet-a")),
        Some(state.shielded_checkpoint("preprod-wallet-a")),
        Some(state.journal("preprod-wallet-a")),
    );
    let restored_transfer =
        futures::executor::block_on(restored_a.reconcile_wallet_transfer_submission().execute(
            WalletTransferSubmissionQuery {
                profile_id: wallet_a_profile_id.clone(),
                draft_id: transfer.draft_id,
            },
        ))
        .expect("included shielded transfer restoration from the public journal");
    assert_eq!(restored_transfer.state, "included");
    assert_eq!(
        restored_transfer.transaction_id.as_deref(),
        Some(transfer_transaction_id.as_str())
    );
    let wallet_a_after = await_shielded_balance(
        &restored_a,
        &wallet_a_profile_id,
        wallet_a_expected_shielded_night_after,
    );
    assert_complete_shielded_snapshot(&wallet_a_after);
    let wallet_b_after = await_shielded_balance(
        &wallet_b,
        &wallet_b_profile_id,
        shielded_transfer_atomic_units,
    );
    assert_complete_shielded_snapshot(&wallet_b_after);
    assert_eq!(
        shielded_balance(&wallet_a_after, NATIVE_SHIELDED_TOKEN_TYPE),
        wallet_a_expected_shielded_night_after
    );
    assert_eq!(
        shielded_balance(&wallet_b_after, NATIVE_SHIELDED_TOKEN_TYPE),
        shielded_transfer_atomic_units
    );
    assert_eq!(
        wallet_a_after.owned_note_count,
        Some(u64::from(wallet_a_expected_shielded_night_after > 0))
    );
    assert_eq!(wallet_b_after.owned_note_count, Some(1));
    drop(restored_a);
    drop(wallet_b);
    state.cleanup();
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
        None,
        None,
    );
    let (recipient_profile_id, recipient_address, _) =
        initialize_account(&recipient, "Ephemeral funded recipient", "undeployed", 0);

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
        None,
        None,
        Some(state.journal("unshielded-funder")),
    );
    let (funder_profile_id, _, _) =
        initialize_account(&funder, "Standalone funding authority", "undeployed", 0);
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
        None,
        None,
        Some(state.journal("unshielded-funder")),
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
        None,
        None,
        Some(state.shielded_checkpoint("recipient")),
        None,
    );
    let (recipient_profile_id, _, recipient_shielded_address) =
        initialize_account(&recipient, "Ephemeral shielded recipient", "undeployed", 0);
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
        None,
        None,
        Some(state.shielded_checkpoint("funder")),
        Some(state.journal("shielded-funder")),
    );
    let (funder_profile_id, _, _) = initialize_account(
        &funder,
        "Standalone shielded funding authority",
        "undeployed",
        0,
    );
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
        None,
        None,
        Some(state.shielded_checkpoint("funder")),
        Some(state.journal("shielded-funder")),
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
