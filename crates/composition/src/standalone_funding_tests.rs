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
    MidnightStandaloneConfig, MidnightSubmissionJournalConfig,
    protected_standalone_midnight_wallet_with_checkpoint_options,
};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_platform_ports::{PlatformError, RandomPort};
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, CreateWalletProfileCommand, DeriveWalletAccountCommand,
    PrepareWalletTransferCommand, SensitiveOperationConfirmation, SubmitWalletTransferCommand,
    WalletAccountQuery, WalletProfileSecurityCommand, WalletTransferSubmissionQuery,
};
use zeroize::Zeroizing;

use super::{ApplicationServices, compose_with_adapters};

const ENABLE_ENV: &str = "OXID_ENABLE_LIVE_STANDALONE_FUNDING";
const FUNDER_SEED_ENV: &str = "OXID_STANDALONE_FUNDER_SEED_HEX";
const TRANSFER_ATOMIC_UNITS: u128 = 5_000_000;

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

struct JournalGuard(PathBuf);

impl JournalGuard {
    fn fresh() -> Self {
        let sequence = PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "oxid-funded-finality-{}-{nanos}-{sequence}.json",
            std::process::id()
        )))
    }

    fn config(&self) -> MidnightSubmissionJournalConfig {
        MidnightSubmissionJournalConfig::new(&self.0).expect("isolated journal path")
    }
}

impl Drop for JournalGuard {
    fn drop(&mut self) {
        match fs::remove_file(&self.0) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // Drop must not mask a more useful assertion failure. The path is
            // random, contains public submission metadata only, and is also
            // removed on every successful run.
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
            None,
            journal,
            clock,
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters(profiles, security, midnight)
}

fn initialize_account(application: &ApplicationServices, name: &str) -> (String, String) {
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
    (profile.id, account.receive_address.value)
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
    let journal = JournalGuard::fresh();

    let recipient_profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let recipient_security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    let recipient = compose_live(config.clone(), recipient_profiles, recipient_security, None);
    let (recipient_profile_id, recipient_address) =
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
        Some(journal.config()),
    );
    let (funder_profile_id, _) = initialize_account(&funder, "Standalone funding authority");
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
        Some(journal.config()),
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
}
