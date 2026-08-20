// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use oxid_adapter_midnight::{
    MidnightShieldedCheckpointConfig, MidnightStandaloneConfig, MidnightSubmissionJournalConfig,
    protected_standalone_midnight_wallet_with_checkpoint_options,
};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_platform_ports::{PlatformError, RandomPort};
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, CreateWalletProfileCommand, DeriveWalletAccountCommand,
    PrepareShieldedWalletTransferCommand, PrepareWalletTransferCommand,
    SensitiveOperationConfirmation, SubmitWalletTransferCommand, WalletAccountQuery,
    WalletProfileSecurityCommand, WalletShieldedSyncCommand, WalletShieldedSyncView,
    WalletTransactionError, WalletTransactionPortError, WalletTransferSubmissionQuery,
};
use zeroize::Zeroizing;

use super::{ApplicationServices, compose_with_adapters};

const ENABLE_ENV: &str = "OXID_ENABLE_LIVE_STANDALONE_FUNDING";
const FUNDER_SEED_ENV: &str = "OXID_STANDALONE_FUNDER_SEED_HEX";
const TRANSFER_ATOMIC_UNITS: u128 = 5_000_000;
const SHIELDED_TRANSFER_ATOMIC_UNITS: u128 = 1_000_000;
const NATIVE_SHIELDED_TOKEN_TYPE: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

static PATH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
