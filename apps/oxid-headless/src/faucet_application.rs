// SPDX-License-Identifier: Apache-2.0

//! Application-port adapter for the development-only standalone faucet.

use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use oxid_composition::{ApplicationServices, public_standalone_profile_name};
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, CreateWalletProfileCommand, DeriveWalletAccountCommand,
    PrepareWalletTransferCommand, SelectWalletNetworkCommand, SelectWalletProfileCommand,
    SensitiveOperationConfirmation, SubmitWalletTransferCommand, WalletAccountQuery,
    WalletProfileSecurityCommand, WalletTransactionError, WalletTransactionPortError,
};
use oxid_wallet_domain::WalletProtectionState;

use crate::faucet::{
    FIXED_GRANT_ATOMIC_UNITS, GrantError, GrantOutcome, NETWORK_ID, NightGrantPort,
};
use crate::faucet_errors::FaucetStartupError;

pub(super) struct ApplicationNightGrant {
    application: ApplicationServices,
    profile_id: String,
    pending_maximum_night_balance: Mutex<Option<u128>>,
}

const INDEXER_OBSERVATION_DEADLINE: Duration = Duration::from_secs(120);
const INDEXER_OBSERVATION_INTERVAL: Duration = Duration::from_secs(2);

impl ApplicationNightGrant {
    pub(super) fn new(application: ApplicationServices) -> Result<Self, FaucetStartupError> {
        let profiles = application
            .list_wallet_profiles()
            .execute()
            .map_err(|_| FaucetStartupError::ProfileUnavailable)?;
        let matches = profiles
            .iter()
            .filter(|profile| profile.display_name == public_standalone_profile_name())
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err(FaucetStartupError::AmbiguousAuthority);
        }
        let profile_id = if let Some(profile) = matches.first() {
            profile.id.clone()
        } else {
            application
                .create_wallet_profile()
                .execute(CreateWalletProfileCommand {
                    display_name: public_standalone_profile_name().to_owned(),
                })
                .map_err(|_| FaucetStartupError::ProfileUnavailable)?
                .id
        };
        application
            .select_wallet_profile()
            .execute(SelectWalletProfileCommand {
                profile_id: profile_id.clone(),
            })
            .map_err(|_| FaucetStartupError::ProfileUnavailable)?;
        application
            .select_wallet_network()
            .execute(SelectWalletNetworkCommand {
                profile_id: profile_id.clone(),
                network_id: NETWORK_ID.to_owned(),
            })
            .map_err(|_| FaucetStartupError::AccountUnavailable)?;

        initialize_security(&application, &profile_id)?;
        application
            .derive_wallet_account()
            .execute(DeriveWalletAccountCommand {
                profile_id: profile_id.clone(),
                account_index: 0,
                address_index: 0,
            })
            .map_err(|_| FaucetStartupError::AccountUnavailable)?;

        Ok(Self {
            application,
            profile_id,
            pending_maximum_night_balance: Mutex::new(None),
        })
    }

    fn await_spendable_balance(&self) -> Result<u128, GrantError> {
        let maximum_balance = self
            .pending_maximum_night_balance
            .lock()
            .map_err(|_| GrantError::Unavailable)?
            .to_owned();
        let started = Instant::now();
        loop {
            let account =
                futures::executor::block_on(self.application.sync_wallet_account().execute(
                    WalletAccountQuery {
                        profile_id: self.profile_id.clone(),
                    },
                ))
                .map_err(|_| GrantError::Unavailable)?;
            let balance = night_balance(&account)?;
            let Some(maximum_balance) = maximum_balance else {
                return Ok(balance);
            };
            if balance <= maximum_balance {
                let mut pending = self
                    .pending_maximum_night_balance
                    .lock()
                    .map_err(|_| GrantError::Unavailable)?;
                if pending.as_ref() == Some(&maximum_balance) {
                    *pending = None;
                }
                return Ok(balance);
            }
            if started.elapsed() >= INDEXER_OBSERVATION_DEADLINE {
                return Err(GrantError::OutcomeUnknown);
            }
            thread::sleep(INDEXER_OBSERVATION_INTERVAL);
        }
    }
}

impl NightGrantPort for ApplicationNightGrant {
    fn grant(&self, recipient_address: &str) -> Result<GrantOutcome, GrantError> {
        let expected_maximum_balance = self
            .await_spendable_balance()?
            .checked_sub(FIXED_GRANT_ATOMIC_UNITS)
            .ok_or(GrantError::AuthorityNotReady)?;
        let prepared = self
            .application
            .prepare_wallet_transfer()
            .execute(PrepareWalletTransferCommand {
                profile_id: self.profile_id.clone(),
                recipient_address: recipient_address.to_owned(),
                amount_atomic_units: FIXED_GRANT_ATOMIC_UNITS.to_string(),
            })
            .map_err(map_transaction_error)?;
        let authorized = self
            .application
            .authorize_wallet_transfer()
            .execute(AuthorizeWalletTransferCommand {
                profile_id: self.profile_id.clone(),
                draft_id: prepared.draft_id.clone(),
                authorization_challenge: prepared.authorization_challenge,
            })
            .map_err(map_transaction_error)?;
        if !authorized.submission_ready {
            return Err(GrantError::Unavailable);
        }
        let submitted =
            futures::executor::block_on(self.application.submit_wallet_transfer().execute(
                SubmitWalletTransferCommand {
                    profile_id: self.profile_id.clone(),
                    draft_id: prepared.draft_id,
                    confirmation: SensitiveOperationConfirmation {
                        title: "Submit standalone NIGHT grant".to_owned(),
                        summary:
                            "Prove and submit the authorized fixed development grant".to_owned(),
                        confirmed: true,
                    },
                },
            ))
            .map_err(map_transaction_error)?;
        *self
            .pending_maximum_night_balance
            .lock()
            .map_err(|_| GrantError::Unavailable)? = Some(expected_maximum_balance);
        Ok(GrantOutcome {
            transaction_id: submitted.transaction_id,
            block_id: submitted.block_id,
        })
    }
}

fn night_balance(account: &oxid_wallet_application::WalletAccountView) -> Result<u128, GrantError> {
    account
        .balances
        .iter()
        .find(|balance| balance.symbol == "NIGHT")
        .ok_or(GrantError::AuthorityNotReady)?
        .atomic_units
        .parse::<u128>()
        .map_err(|_| GrantError::Unavailable)
}

fn initialize_security(
    application: &ApplicationServices,
    profile_id: &str,
) -> Result<(), FaucetStartupError> {
    let command = WalletProfileSecurityCommand {
        profile_id: profile_id.to_owned(),
    };
    let status = application
        .get_wallet_security_status()
        .execute(command.clone())
        .map_err(|_| FaucetStartupError::ProtectionUnavailable)?;
    match status.state {
        WalletProtectionState::Uninitialized => application
            .initialize_wallet_security()
            .execute(command)
            .map(|_| ())
            .map_err(|_| FaucetStartupError::ProtectionUnavailable),
        WalletProtectionState::Locked => application
            .unlock_wallet()
            .execute(command)
            .map(|_| ())
            .map_err(|_| FaucetStartupError::ProtectionUnavailable),
        WalletProtectionState::Unlocked => Ok(()),
        WalletProtectionState::Unavailable => Err(FaucetStartupError::ProtectionUnavailable),
    }
}

fn map_transaction_error(error: WalletTransactionError) -> GrantError {
    match error {
        WalletTransactionError::InvalidRecipient(_) | WalletTransactionError::InvalidAmount => {
            GrantError::InvalidRecipient
        }
        WalletTransactionError::Operation(WalletTransactionPortError::InvalidRecipient) => {
            GrantError::InvalidRecipient
        }
        WalletTransactionError::Operation(WalletTransactionPortError::RecipientNetworkMismatch) => {
            GrantError::NetworkMismatch
        }
        WalletTransactionError::Operation(
            WalletTransactionPortError::InsufficientFunds
            | WalletTransactionPortError::InsufficientDust
            | WalletTransactionPortError::AccountNotSynchronized,
        ) => GrantError::AuthorityNotReady,
        WalletTransactionError::Operation(WalletTransactionPortError::SubmissionRejected) => {
            GrantError::Rejected
        }
        WalletTransactionError::Operation(WalletTransactionPortError::SubmissionOutcomeUnknown) => {
            GrantError::OutcomeUnknown
        }
        _ => GrantError::Unavailable,
    }
}
