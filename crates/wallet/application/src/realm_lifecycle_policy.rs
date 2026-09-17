// SPDX-License-Identifier: Apache-2.0

//! Deterministic lifecycle request policy for one selected wallet realm.
//!
//! This module has no clock, scheduler, renderer, or port dependency. Callers
//! supply monotonic observations and execute accepted requests through the
//! selected-realm reconciliation boundary.

use oxid_wallet_domain::{ChainNetworkId, WalletProfileId};

use crate::{
    WalletRealmFacetState, WalletRealmReconciliationState, WalletRealmReconciliationTrigger,
};

pub const DEFAULT_WALLET_REALM_LIFECYCLE_DEBOUNCE_MILLIS: u64 = 5_000;
pub const DEFAULT_WALLET_REALM_LIFECYCLE_STALE_AGE_MILLIS: u64 = 30_000;
pub const DEFAULT_WALLET_REALM_LIFECYCLE_BACKOFF_BASE_MILLIS: u64 = 1_000;
pub const DEFAULT_WALLET_REALM_LIFECYCLE_BACKOFF_MAX_MILLIS: u64 = 60_000;
pub const DEFAULT_WALLET_REALM_LIFECYCLE_RETRY_CEILING: u8 = 5;
pub const DEFAULT_WALLET_REALM_LIFECYCLE_JITTER_WINDOW_MILLIS: u64 = 250;

/// Validated bounds for the pure lifecycle policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecyclePolicyConfig {
    debounce_millis: u64,
    stale_age_millis: u64,
    backoff_base_millis: u64,
    backoff_max_millis: u64,
    retry_ceiling: u8,
    jitter_window_millis: u64,
}

impl Default for WalletRealmLifecyclePolicyConfig {
    fn default() -> Self {
        Self {
            debounce_millis: DEFAULT_WALLET_REALM_LIFECYCLE_DEBOUNCE_MILLIS,
            stale_age_millis: DEFAULT_WALLET_REALM_LIFECYCLE_STALE_AGE_MILLIS,
            backoff_base_millis: DEFAULT_WALLET_REALM_LIFECYCLE_BACKOFF_BASE_MILLIS,
            backoff_max_millis: DEFAULT_WALLET_REALM_LIFECYCLE_BACKOFF_MAX_MILLIS,
            retry_ceiling: DEFAULT_WALLET_REALM_LIFECYCLE_RETRY_CEILING,
            jitter_window_millis: DEFAULT_WALLET_REALM_LIFECYCLE_JITTER_WINDOW_MILLIS,
        }
    }
}

impl WalletRealmLifecyclePolicyConfig {
    pub fn new(
        debounce_millis: u64,
        stale_age_millis: u64,
        backoff_base_millis: u64,
        backoff_max_millis: u64,
        retry_ceiling: u8,
        jitter_window_millis: u64,
    ) -> Result<Self, WalletRealmLifecyclePolicyConfigError> {
        if debounce_millis == 0
            || stale_age_millis == 0
            || backoff_base_millis == 0
            || backoff_max_millis < backoff_base_millis
            || retry_ceiling == 0
        {
            return Err(WalletRealmLifecyclePolicyConfigError::InvalidBounds);
        }
        Ok(Self {
            debounce_millis,
            stale_age_millis,
            backoff_base_millis,
            backoff_max_millis,
            retry_ceiling,
            jitter_window_millis,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRealmLifecyclePolicyConfigError {
    InvalidBounds,
}

/// Public resource identity; it contains no wallet secret or network payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleIdentity {
    pub profile: WalletProfileId,
    pub realm: ChainNetworkId,
}

/// Closed lifecycle inputs. Timestamps are caller-supplied monotonic millis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmLifecycleInput {
    Initialized {
        identity: WalletRealmLifecycleIdentity,
        now_millis: u64,
        facets: WalletRealmReconciliationState,
    },
    RealmSelected {
        identity: WalletRealmLifecycleIdentity,
        now_millis: u64,
        facets: WalletRealmReconciliationState,
    },
    Backgrounded {
        now_millis: u64,
    },
    Foreground {
        now_millis: u64,
        facets: WalletRealmReconciliationState,
    },
    ConnectivityRestored {
        now_millis: u64,
        facets: WalletRealmReconciliationState,
    },
    PeriodicTick {
        now_millis: u64,
        facets: WalletRealmReconciliationState,
    },
    ActionPreflight {
        now_millis: u64,
        facets: WalletRealmReconciliationState,
    },
    ReconciliationFinished {
        identity: WalletRealmLifecycleIdentity,
        sequence: u64,
        now_millis: u64,
        facets: WalletRealmReconciliationState,
        succeeded: bool,
    },
}

/// A request retained by the policy has a stable local causation sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleRequest {
    pub identity: WalletRealmLifecycleIdentity,
    pub trigger: WalletRealmReconciliationTrigger,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmLifecycleDecision {
    Request(WalletRealmLifecycleRequest),
    Retained(WalletRealmLifecycleRequest),
    Ignored,
    Superseded {
        previous: WalletRealmLifecycleIdentity,
        request: WalletRealmLifecycleRequest,
    },
}

/// Pure mutable state for the active selected realm.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalletRealmLifecyclePolicy {
    active: Option<WalletRealmLifecycleIdentity>,
    foreground: bool,
    in_flight: Option<WalletRealmLifecycleRequest>,
    pending: Option<WalletRealmLifecycleRequest>,
    last_request_millis: Option<u64>,
    last_fresh_millis: Option<u64>,
    last_failure_millis: Option<u64>,
    retry_count: u8,
    sequence: u64,
}

impl WalletRealmLifecyclePolicy {
    pub(crate) fn owns(&self, request: &WalletRealmLifecycleRequest) -> bool {
        self.active.as_ref() == Some(&request.identity)
            && self
                .in_flight
                .as_ref()
                .is_some_and(|in_flight| in_flight.sequence == request.sequence)
    }

    #[must_use]
    pub fn reduce(
        &mut self,
        config: WalletRealmLifecyclePolicyConfig,
        input: WalletRealmLifecycleInput,
    ) -> WalletRealmLifecycleDecision {
        match input {
            WalletRealmLifecycleInput::Initialized {
                identity,
                now_millis,
                facets,
            }
            | WalletRealmLifecycleInput::RealmSelected {
                identity,
                now_millis,
                facets,
            } => {
                let superseded = self
                    .active
                    .replace(identity.clone())
                    .filter(|previous| previous != &identity);
                if superseded.is_some() {
                    self.reset_active();
                }
                self.foreground = true;
                self.observe_freshness(now_millis, facets);
                let decision = self.request(
                    identity,
                    WalletRealmReconciliationTrigger::Initial,
                    now_millis,
                );
                match (superseded, decision) {
                    (Some(previous), WalletRealmLifecycleDecision::Request(request)) => {
                        WalletRealmLifecycleDecision::Superseded { previous, request }
                    }
                    (_, decision) => decision,
                }
            }
            WalletRealmLifecycleInput::Backgrounded { now_millis: _ } => {
                self.foreground = false;
                WalletRealmLifecycleDecision::Ignored
            }
            WalletRealmLifecycleInput::Foreground { now_millis, facets } => {
                self.foreground = true;
                self.automatic(config, now_millis, facets, false)
            }
            WalletRealmLifecycleInput::ConnectivityRestored { now_millis, facets } => {
                self.automatic(config, now_millis, facets, false)
            }
            WalletRealmLifecycleInput::PeriodicTick { now_millis, facets } => {
                self.automatic(config, now_millis, facets, true)
            }
            WalletRealmLifecycleInput::ActionPreflight {
                now_millis,
                facets: _,
            } => {
                let Some(identity) = self.active.clone() else {
                    return WalletRealmLifecycleDecision::Ignored;
                };
                self.request(
                    identity,
                    WalletRealmReconciliationTrigger::ActionPreflight,
                    now_millis,
                )
            }
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity,
                sequence,
                now_millis,
                facets,
                succeeded,
            } => {
                if self.active.as_ref() != Some(&identity)
                    || self.in_flight.as_ref().map(|request| request.sequence) != Some(sequence)
                {
                    return WalletRealmLifecycleDecision::Ignored;
                }
                self.in_flight = None;
                if succeeded {
                    self.retry_count = 0;
                    self.last_failure_millis = None;
                    self.observe_freshness(now_millis, facets);
                } else {
                    self.retry_count = self.retry_count.saturating_add(1);
                    self.last_failure_millis = Some(now_millis);
                }
                match self.pending.take() {
                    Some(request) => self.admit(request, now_millis),
                    None => WalletRealmLifecycleDecision::Ignored,
                }
            }
        }
    }

    fn automatic(
        &mut self,
        config: WalletRealmLifecyclePolicyConfig,
        now_millis: u64,
        facets: WalletRealmReconciliationState,
        periodic: bool,
    ) -> WalletRealmLifecycleDecision {
        let Some(identity) = self.active.clone() else {
            return WalletRealmLifecycleDecision::Ignored;
        };
        if !self.foreground
            || !self.stale_enough(config, now_millis, facets)
            || !self.outside_debounce(config, now_millis)
        {
            return WalletRealmLifecycleDecision::Ignored;
        }
        if periodic
            && (self.retry_count >= config.retry_ceiling || !self.retry_due(config, now_millis))
        {
            return WalletRealmLifecycleDecision::Ignored;
        }
        self.request(
            identity,
            WalletRealmReconciliationTrigger::ManualRefresh,
            now_millis,
        )
    }

    fn request(
        &mut self,
        identity: WalletRealmLifecycleIdentity,
        trigger: WalletRealmReconciliationTrigger,
        now_millis: u64,
    ) -> WalletRealmLifecycleDecision {
        if let Some(in_flight) = &self.in_flight {
            if trigger != WalletRealmReconciliationTrigger::ActionPreflight
                || in_flight.trigger == WalletRealmReconciliationTrigger::ActionPreflight
                || self.pending.is_some()
            {
                return WalletRealmLifecycleDecision::Ignored;
            }
            let request = WalletRealmLifecycleRequest {
                identity,
                trigger,
                sequence: self.next_sequence(),
            };
            self.pending = Some(request.clone());
            return WalletRealmLifecycleDecision::Retained(request);
        }
        let request = WalletRealmLifecycleRequest {
            identity,
            trigger,
            sequence: self.next_sequence(),
        };
        self.admit(request, now_millis)
    }

    fn admit(
        &mut self,
        request: WalletRealmLifecycleRequest,
        now_millis: u64,
    ) -> WalletRealmLifecycleDecision {
        self.last_request_millis = Some(now_millis);
        self.in_flight = Some(request.clone());
        WalletRealmLifecycleDecision::Request(request)
    }

    fn observe_freshness(&mut self, now_millis: u64, facets: WalletRealmReconciliationState) {
        if !is_stale(facets) {
            self.last_fresh_millis = Some(now_millis);
        }
    }

    fn outside_debounce(&self, config: WalletRealmLifecyclePolicyConfig, now_millis: u64) -> bool {
        self.last_request_millis
            .is_none_or(|last| now_millis.saturating_sub(last) >= config.debounce_millis)
    }

    fn stale_enough(
        &self,
        config: WalletRealmLifecyclePolicyConfig,
        now_millis: u64,
        facets: WalletRealmReconciliationState,
    ) -> bool {
        is_stale(facets)
            || self
                .last_fresh_millis
                .is_none_or(|last| now_millis.saturating_sub(last) >= config.stale_age_millis)
    }

    fn retry_due(&self, config: WalletRealmLifecyclePolicyConfig, now_millis: u64) -> bool {
        let exponent = u32::from(self.retry_count.saturating_sub(1)).min(63);
        let jitter = self.active.as_ref().map_or(0, |identity| {
            deterministic_jitter(identity, self.retry_count, config.jitter_window_millis)
        });
        let delay = config
            .backoff_base_millis
            .saturating_mul(1_u64 << exponent)
            .min(config.backoff_max_millis)
            .saturating_add(jitter);
        self.last_failure_millis
            .or(self.last_request_millis)
            .is_none_or(|last| now_millis.saturating_sub(last) >= delay)
    }

    fn next_sequence(&mut self) -> u64 {
        self.sequence = self.sequence.wrapping_add(1).max(1);
        self.sequence
    }
    fn reset_active(&mut self) {
        self.in_flight = None;
        self.pending = None;
        self.last_request_millis = None;
        self.last_fresh_millis = None;
        self.last_failure_millis = None;
        self.retry_count = 0;
    }
}

fn is_stale(facets: WalletRealmReconciliationState) -> bool {
    [facets.account, facets.dust, facets.shielded]
        .into_iter()
        .any(|facet| {
            matches!(
                facet,
                WalletRealmFacetState::Stale | WalletRealmFacetState::Missing
            )
        })
}

fn deterministic_jitter(
    identity: &WalletRealmLifecycleIdentity,
    retry_count: u8,
    window_millis: u64,
) -> u64 {
    if window_millis == 0 {
        return 0;
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in identity
        .profile
        .as_str()
        .bytes()
        .chain([0xff])
        .chain(identity.realm.as_str().bytes())
        .chain([retry_count])
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (u128::from(hash) % (u128::from(window_millis) + 1)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(realm: &str) -> WalletRealmLifecycleIdentity {
        WalletRealmLifecycleIdentity {
            profile: WalletProfileId::parse("profile_lifecycle").expect("profile"),
            realm: ChainNetworkId::parse(realm).expect("realm"),
        }
    }
    fn stale() -> WalletRealmReconciliationState {
        WalletRealmReconciliationState {
            account: WalletRealmFacetState::Stale,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Current,
        }
    }
    fn fresh() -> WalletRealmReconciliationState {
        WalletRealmReconciliationState {
            account: WalletRealmFacetState::Current,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Current,
        }
    }

    #[test]
    fn initialization_requests_once_and_fresh_lifecycle_events_are_noops() {
        let mut policy = WalletRealmLifecyclePolicy::default();
        let config = WalletRealmLifecyclePolicyConfig::default();
        assert!(matches!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::Initialized {
                    identity: identity("preprod"),
                    now_millis: 0,
                    facets: stale()
                }
            ),
            WalletRealmLifecycleDecision::Request(_)
        ));
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::ReconciliationFinished {
                    identity: identity("preprod"),
                    sequence: 1,
                    now_millis: 1,
                    facets: fresh(),
                    succeeded: true
                }
            ),
            WalletRealmLifecycleDecision::Ignored
        );
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::Foreground {
                    now_millis: 20_000,
                    facets: fresh()
                }
            ),
            WalletRealmLifecycleDecision::Ignored
        );
    }

    #[test]
    fn current_facets_age_from_the_last_successful_reconciliation() {
        let config =
            WalletRealmLifecyclePolicyConfig::new(1, 10, 1, 10, 2, 0).expect("valid bounds");
        let mut policy = WalletRealmLifecyclePolicy::default();
        let active = identity("preprod");
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: active.clone(),
                now_millis: 0,
                facets: fresh(),
            },
        );
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity: active,
                sequence: 1,
                now_millis: 1,
                facets: fresh(),
                succeeded: true,
            },
        );

        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::PeriodicTick {
                    now_millis: 10,
                    facets: fresh(),
                },
            ),
            WalletRealmLifecycleDecision::Ignored
        );
        assert!(matches!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::PeriodicTick {
                    now_millis: 11,
                    facets: fresh(),
                },
            ),
            WalletRealmLifecycleDecision::Request(_)
        ));
    }

    #[test]
    fn repeated_initialization_does_not_queue_duplicate_work() {
        let mut policy = WalletRealmLifecyclePolicy::default();
        let config = WalletRealmLifecyclePolicyConfig::default();
        let initial = WalletRealmLifecycleInput::Initialized {
            identity: identity("preprod"),
            now_millis: 0,
            facets: stale(),
        };
        assert!(matches!(
            policy.reduce(config, initial.clone()),
            WalletRealmLifecycleDecision::Request(_)
        ));
        assert_eq!(
            policy.reduce(config, initial),
            WalletRealmLifecycleDecision::Ignored
        );
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::ReconciliationFinished {
                    identity: identity("preprod"),
                    sequence: 1,
                    now_millis: 1,
                    facets: fresh(),
                    succeeded: true,
                }
            ),
            WalletRealmLifecycleDecision::Ignored
        );
    }

    #[test]
    fn automatic_events_do_not_queue_behind_active_reconciliation() {
        let mut policy = WalletRealmLifecyclePolicy::default();
        let config = WalletRealmLifecyclePolicyConfig::default();
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: identity("preprod"),
                now_millis: 0,
                facets: stale(),
            },
        );
        for input in [
            WalletRealmLifecycleInput::PeriodicTick {
                now_millis: 60_000,
                facets: stale(),
            },
            WalletRealmLifecycleInput::ConnectivityRestored {
                now_millis: 60_000,
                facets: stale(),
            },
        ] {
            assert_eq!(
                policy.reduce(config, input),
                WalletRealmLifecycleDecision::Ignored
            );
        }
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::ReconciliationFinished {
                    identity: identity("preprod"),
                    sequence: 1,
                    now_millis: 60_001,
                    facets: fresh(),
                    succeeded: true,
                },
            ),
            WalletRealmLifecycleDecision::Ignored
        );
    }

    #[test]
    fn retained_preflight_keeps_its_sequence_when_admitted() {
        let mut policy = WalletRealmLifecyclePolicy::default();
        let config = WalletRealmLifecyclePolicyConfig::default();
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: identity("preprod"),
                now_millis: 0,
                facets: stale(),
            },
        );
        let retained = match policy.reduce(
            config,
            WalletRealmLifecycleInput::ActionPreflight {
                now_millis: 1,
                facets: stale(),
            },
        ) {
            WalletRealmLifecycleDecision::Retained(request) => request,
            decision => panic!("expected retained preflight, got {decision:?}"),
        };
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::ReconciliationFinished {
                    identity: identity("preprod"),
                    sequence: 1,
                    now_millis: 2,
                    facets: stale(),
                    succeeded: true,
                },
            ),
            WalletRealmLifecycleDecision::Request(retained)
        );
    }

    #[test]
    fn stale_resume_is_debounced_and_periodic_retry_is_bounded() {
        let mut policy = WalletRealmLifecyclePolicy::default();
        let config = WalletRealmLifecyclePolicyConfig::default();
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: identity("preprod"),
                now_millis: 0,
                facets: stale(),
            },
        );
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity: identity("preprod"),
                sequence: 1,
                now_millis: 1,
                facets: stale(),
                succeeded: false,
            },
        );
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::Foreground {
                    now_millis: 2_000,
                    facets: stale()
                }
            ),
            WalletRealmLifecycleDecision::Ignored
        );
        assert!(matches!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::Foreground {
                    now_millis: 31_000,
                    facets: stale()
                }
            ),
            WalletRealmLifecycleDecision::Request(_)
        ));
    }

    #[test]
    fn backgrounded_policy_ignores_periodic_and_connectivity_events_until_foregrounded() {
        let config =
            WalletRealmLifecyclePolicyConfig::new(10, 10, 10, 40, 2, 0).expect("valid bounds");
        let mut policy = WalletRealmLifecyclePolicy::default();
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: identity("preprod"),
                now_millis: 0,
                facets: stale(),
            },
        );
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity: identity("preprod"),
                sequence: 1,
                now_millis: 1,
                facets: stale(),
                succeeded: true,
            },
        );
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::Backgrounded { now_millis: 2 },
            ),
            WalletRealmLifecycleDecision::Ignored
        );
        for input in [
            WalletRealmLifecycleInput::PeriodicTick {
                now_millis: 100,
                facets: stale(),
            },
            WalletRealmLifecycleInput::ConnectivityRestored {
                now_millis: 100,
                facets: stale(),
            },
        ] {
            assert_eq!(
                policy.reduce(config, input),
                WalletRealmLifecycleDecision::Ignored
            );
        }
        assert!(matches!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::Foreground {
                    now_millis: 100,
                    facets: stale(),
                }
            ),
            WalletRealmLifecycleDecision::Request(_)
        ));
    }

    #[test]
    fn connectivity_and_periodic_inputs_obey_staleness_backoff_and_retry_ceiling() {
        let config =
            WalletRealmLifecyclePolicyConfig::new(10, 10, 10, 40, 2, 0).expect("valid bounds");
        let mut policy = WalletRealmLifecyclePolicy::default();
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: identity("preprod"),
                now_millis: 0,
                facets: stale(),
            },
        );
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity: identity("preprod"),
                sequence: 1,
                now_millis: 1,
                facets: stale(),
                succeeded: false,
            },
        );
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::PeriodicTick {
                    now_millis: 9,
                    facets: stale(),
                }
            ),
            WalletRealmLifecycleDecision::Ignored
        );
        assert!(matches!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::ConnectivityRestored {
                    now_millis: 11,
                    facets: stale(),
                }
            ),
            WalletRealmLifecycleDecision::Request(_)
        ));
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity: identity("preprod"),
                sequence: 2,
                now_millis: 12,
                facets: stale(),
                succeeded: false,
            },
        );
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::PeriodicTick {
                    now_millis: 100,
                    facets: stale(),
                }
            ),
            WalletRealmLifecycleDecision::Ignored
        );
    }

    #[test]
    fn periodic_retry_uses_a_deterministic_bounded_jitter() {
        let active = identity("preprod");
        let jitter = deterministic_jitter(&active, 1, 10);
        assert!(jitter <= 10);
        assert_eq!(jitter, deterministic_jitter(&active, 1, 10));

        let config =
            WalletRealmLifecyclePolicyConfig::new(1, 1, 10, 10, 2, 10).expect("valid bounds");
        let mut policy = WalletRealmLifecyclePolicy::default();
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: active.clone(),
                now_millis: 0,
                facets: stale(),
            },
        );
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity: active,
                sequence: 1,
                now_millis: 1_000,
                facets: stale(),
                succeeded: false,
            },
        );
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::PeriodicTick {
                    now_millis: 1_009 + jitter,
                    facets: stale(),
                },
            ),
            WalletRealmLifecycleDecision::Ignored
        );
        assert!(matches!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::PeriodicTick {
                    now_millis: 1_010 + jitter,
                    facets: stale(),
                },
            ),
            WalletRealmLifecycleDecision::Request(_)
        ));
    }

    #[test]
    fn preflight_is_retained_over_lower_priority_in_flight_work_and_switch_clears_it() {
        let mut policy = WalletRealmLifecyclePolicy::default();
        let config = WalletRealmLifecyclePolicyConfig::default();
        let _ = policy.reduce(
            config,
            WalletRealmLifecycleInput::Initialized {
                identity: identity("preprod"),
                now_millis: 0,
                facets: stale(),
            },
        );
        let retained = policy.reduce(
            config,
            WalletRealmLifecycleInput::ActionPreflight {
                now_millis: 1,
                facets: stale(),
            },
        );
        assert!(matches!(
            retained,
            WalletRealmLifecycleDecision::Retained(WalletRealmLifecycleRequest {
                trigger: WalletRealmReconciliationTrigger::ActionPreflight,
                ..
            })
        ));
        assert!(matches!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::RealmSelected {
                    identity: identity("undeployed"),
                    now_millis: 2,
                    facets: stale()
                }
            ),
            WalletRealmLifecycleDecision::Superseded {
                request: WalletRealmLifecycleRequest {
                    trigger: WalletRealmReconciliationTrigger::Initial,
                    ..
                },
                ..
            }
        ));
        assert_eq!(
            policy.reduce(
                config,
                WalletRealmLifecycleInput::ReconciliationFinished {
                    identity: identity("preprod"),
                    sequence: 1,
                    now_millis: 3,
                    facets: fresh(),
                    succeeded: true,
                }
            ),
            WalletRealmLifecycleDecision::Ignored
        );
    }
}
