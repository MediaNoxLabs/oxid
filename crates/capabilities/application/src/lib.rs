// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

/// Public, non-secret composition facts used to build the capability manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityManifestContext {
    compact_presentation_proof_available: bool,
    passport_vault_call_mode: PassportVaultCallMode,
    passport_vault_state_persistence: PassportVaultStatePersistence,
}

impl CapabilityManifestContext {
    /// Admits only closed Oxid composition labels. Unknown input fails closed
    /// to `unavailable` rather than becoming an arbitrary manifest value.
    #[must_use]
    pub fn new(
        compact_presentation_proof_available: bool,
        passport_vault_call_mode: &str,
        passport_vault_state_persistence: &str,
    ) -> Self {
        Self {
            compact_presentation_proof_available,
            passport_vault_call_mode: PassportVaultCallMode::parse(passport_vault_call_mode),
            passport_vault_state_persistence: PassportVaultStatePersistence::parse(
                passport_vault_state_persistence,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassportVaultCallMode {
    DeterministicSimulation,
    NativePending,
    NativeSettlement,
    Unavailable,
}

impl PassportVaultCallMode {
    fn parse(value: &str) -> Self {
        match value {
            "deterministic_simulation" => Self::DeterministicSimulation,
            "native_pending" => Self::NativePending,
            "native_settlement" => Self::NativeSettlement,
            "unavailable" => Self::Unavailable,
            _ => Self::Unavailable,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::DeterministicSimulation => "deterministic_simulation",
            Self::NativePending => "native_pending",
            Self::NativeSettlement => "native_settlement",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassportVaultStatePersistence {
    ProcessLocal,
    OwnerPrivateAtomicFile,
    Unavailable,
}

impl PassportVaultStatePersistence {
    fn parse(value: &str) -> Self {
        match value {
            "process_local" => Self::ProcessLocal,
            "owner_private_atomic_file" => Self::OwnerPrivateAtomicFile,
            "unavailable" => Self::Unavailable,
            _ => Self::Unavailable,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::ProcessLocal => "process_local",
            Self::OwnerPrivateAtomicFile => "owner_private_atomic_file",
            Self::Unavailable => "unavailable",
        }
    }
}

/// One safe machine-readable value in a capability declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityValue {
    Text(String),
    Boolean(bool),
    TextList(Vec<String>),
    Object(Vec<CapabilityFact>),
    Null,
}

impl CapabilityValue {
    #[must_use]
    pub fn display_text(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Boolean(value) => value.to_string(),
            Self::TextList(values) => values.join(", "),
            Self::Object(values) => values
                .iter()
                .map(|fact| format!("{}={}", fact.key(), fact.value().display_text()))
                .collect::<Vec<_>>()
                .join(", "),
            Self::Null => "null".to_owned(),
        }
    }
}

/// One named, public fact attached to a capability declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityFact {
    key: &'static str,
    value: CapabilityValue,
}

impl CapabilityFact {
    #[must_use]
    pub const fn new(key: &'static str, value: CapabilityValue) -> Self {
        Self { key, value }
    }

    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    #[must_use]
    pub const fn value(&self) -> &CapabilityValue {
        &self.value
    }
}

/// One method declaration shared by the headless adapter and developer UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityView {
    method: &'static str,
    status: &'static str,
    facts: Vec<CapabilityFact>,
}

impl CapabilityView {
    #[must_use]
    pub const fn new(method: &'static str, status: &'static str) -> Self {
        Self {
            method,
            status,
            facts: Vec::new(),
        }
    }

    #[must_use]
    pub fn text(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.facts.push(CapabilityFact::new(
            key,
            CapabilityValue::Text(value.into()),
        ));
        self
    }

    #[must_use]
    pub fn boolean(mut self, key: &'static str, value: bool) -> Self {
        self.facts
            .push(CapabilityFact::new(key, CapabilityValue::Boolean(value)));
        self
    }

    #[must_use]
    pub fn texts(mut self, key: &'static str, values: &[&str]) -> Self {
        self.facts.push(CapabilityFact::new(
            key,
            CapabilityValue::TextList(values.iter().map(|value| (*value).to_owned()).collect()),
        ));
        self
    }

    #[must_use]
    pub fn object(mut self, key: &'static str, values: Vec<CapabilityFact>) -> Self {
        self.facts
            .push(CapabilityFact::new(key, CapabilityValue::Object(values)));
        self
    }

    #[must_use]
    pub fn null(mut self, key: &'static str) -> Self {
        self.facts
            .push(CapabilityFact::new(key, CapabilityValue::Null));
        self
    }

    #[must_use]
    pub const fn method(&self) -> &'static str {
        self.method
    }

    #[must_use]
    pub const fn status(&self) -> &'static str {
        self.status
    }

    #[must_use]
    pub fn facts(&self) -> &[CapabilityFact] {
        &self.facts
    }

    #[must_use]
    pub fn confirmation_required(&self) -> bool {
        self.facts.iter().any(|fact| {
            fact.key == "confirmationRequired" && fact.value == CapabilityValue::Boolean(true)
        })
    }
}

fn text(key: &'static str, value: &'static str) -> CapabilityFact {
    CapabilityFact::new(key, CapabilityValue::Text(value.to_owned()))
}

/// Builds the complete, public capability manifest.
///
/// Values are deliberately constrained to stable machine strings, booleans,
/// and closed lists. The manifest cannot carry profile identifiers, request
/// payloads, claims, endpoints, keys, or operational logs.
#[must_use]
pub fn capability_manifest(context: CapabilityManifestContext) -> Vec<CapabilityView> {
    let call_mode = context.passport_vault_call_mode.as_str();
    let state_persistence = context.passport_vault_state_persistence.as_str();
    let call_authentication = match context.passport_vault_call_mode {
        PassportVaultCallMode::DeterministicSimulation => "deterministic_simulation",
        PassportVaultCallMode::NativePending | PassportVaultCallMode::NativeSettlement => {
            "canonical_finalized_replay"
        }
        PassportVaultCallMode::Unavailable => "unavailable",
    };
    let call_status = if matches!(call_mode, "deterministic_simulation" | "native_settlement") {
        "ready"
    } else {
        "composition_dependent"
    };
    let call_operations = if matches!(call_mode, "deterministic_simulation" | "native_settlement") {
        &[
            "create_lock",
            "deposit_to_lock",
            "claim_from_lock",
            "withdraw_from_lock",
        ][..]
    } else {
        &["create_lock", "deposit_to_lock", "withdraw_from_lock"][..]
    };
    let history_persistence = if call_mode == "native_settlement" {
        "public_metadata_only"
    } else {
        "process_local_public_metadata"
    };
    let reconciliation_scope = if call_mode == "native_settlement" {
        "finalized_chain"
    } else {
        "adapter_status"
    };
    let presentation_status = if context.compact_presentation_proof_available {
        "ready"
    } else {
        "blocked"
    };

    vec![
        CapabilityView::new("system.capabilities", "ready")
            .text("manifestSource", "oxid_capabilities_application")
            .text("snapshotFreshness", "composition_time")
            .text("cursor", "not_applicable")
            .text("timing", "not_collected"),
        CapabilityView::new("system.diagnostics.snapshot", "ready")
            .text("persistence", "process_local")
            .text("telemetry", "off")
            .boolean("payloadsRetained", false),
        CapabilityView::new("system.diagnostics.clear", "ready")
            .boolean("confirmationRequired", true)
            .text("intent", "CLEAR_LOCAL_DIAGNOSTICS"),
        CapabilityView::new("system.quit", "ready"),
        CapabilityView::new("wallet.profile.create", "ready"),
        CapabilityView::new("wallet.profile.list", "ready"),
        CapabilityView::new("wallet.profile.select", "ready"),
        CapabilityView::new("wallet.profile.active", "ready"),
        CapabilityView::new("wallet.security.status", "ready").text("mode", "development_only"),
        CapabilityView::new("wallet.security.initialize", "ready").text("mode", "development_only"),
        CapabilityView::new("wallet.security.unlock", "ready").text("mode", "development_only"),
        CapabilityView::new("wallet.security.lock", "ready").text("mode", "development_only"),
        CapabilityView::new("wallet.key.generate", "ready")
            .text("mode", "development_only")
            .texts(
                "algorithms",
                &["ed25519", "p256", "secp256k1-schnorr", "jubjub"],
            ),
        CapabilityView::new("wallet.key.list", "ready").text("mode", "development_only"),
        CapabilityView::new("wallet.key.sign", "ready")
            .text("mode", "development_only")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.key.delete", "ready")
            .text("mode", "development_only")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.network.list", "ready").text("mode", "standalone"),
        CapabilityView::new("wallet.network.select", "ready").text("mode", "standalone"),
        CapabilityView::new("wallet.account.derive", "ready")
            .text("mode", "development_only")
            .texts("paths", &["midnight-night-external", "midnight-zswap"]),
        CapabilityView::new("wallet.account.get", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live", "cached"]),
        CapabilityView::new("wallet.connect", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live"]),
        CapabilityView::new("wallet.bootstrap", "queued"),
        CapabilityView::new("wallet.address.list", "ready")
            .text("mode", "standalone")
            .texts(
                "sources",
                &[
                    "protected_derivation",
                    "official_public_vectors",
                    "configured_public_address",
                ],
            ),
        CapabilityView::new("wallet.address.unshielded", "ready")
            .text("mode", "standalone")
            .texts(
                "sources",
                &[
                    "protected_derivation",
                    "official_public_vectors",
                    "configured_public_address",
                ],
            ),
        CapabilityView::new("wallet.address.shielded", "ready")
            .text("mode", "standalone")
            .texts(
                "sources",
                &["protected_derivation", "official_public_vectors"],
            ),
        CapabilityView::new("wallet.balance.snapshot", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live", "cached"]),
        CapabilityView::new("wallet.transaction.history", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live", "cached"]),
        CapabilityView::new("wallet.transaction.prepare_unshielded", "ready")
            .text("mode", "development_only")
            .boolean("submissionReady", false),
        CapabilityView::new("wallet.transaction.prepare_shielded", "ready")
            .text("mode", "standalone")
            .text("requires", "fresh_shielded_sync")
            .boolean("submissionReady", false),
        CapabilityView::new("wallet.transaction.authorize_unshielded", "ready")
            .text("mode", "development_only")
            .boolean("submissionReady", true)
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.transaction.authorize_shielded", "ready")
            .text("mode", "standalone")
            .boolean("submissionReady", true)
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.transaction.draft", "ready")
            .text("mode", "development_only")
            .text("submissionReady", "state_dependent"),
        CapabilityView::new("wallet.transaction.submit_unshielded", "ready")
            .text("mode", "development_only")
            .texts("sources", &["simulated", "live"])
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.transaction.send_unshielded", "ready")
            .text("mode", "development_only")
            .text("aliasFor", "wallet.transaction.submit_unshielded")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.transaction.submit_shielded", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live"])
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.transaction.send_shielded", "ready")
            .text("mode", "standalone")
            .text("aliasFor", "wallet.transaction.submit_shielded")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.transaction.start_submission", "ready")
            .text("mode", "development_only")
            .text("execution", "adapter_worker")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.transaction.submission_status", "ready")
            .text("mode", "development_only"),
        CapabilityView::new("wallet.transaction.submission_history", "ready")
            .text("mode", "standalone")
            .text("persistence", "public_metadata_only"),
        CapabilityView::new("wallet.transaction.reconcile_submission", "ready")
            .text("mode", "standalone")
            .text("scope", "finalized_chain"),
        CapabilityView::new("wallet.transaction.cancel_submission", "ready")
            .text("mode", "development_only")
            .text("boundary", "pre_broadcast_only"),
        CapabilityView::new("wallet.sync.force", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live"]),
        CapabilityView::new("wallet.dust.sync.status", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live", "cached", "unavailable"]),
        CapabilityView::new("wallet.dust.sync.start", "ready")
            .text("mode", "standalone")
            .text("execution", "adapter_worker"),
        CapabilityView::new("wallet.dust.sync.cancel", "ready")
            .text("mode", "standalone")
            .text("checkpoint", "resumable"),
        CapabilityView::new("wallet.dust.registration.prepare", "ready")
            .text("mode", "standalone")
            .text("source", "live_indexer_v4")
            .text("feeAuthority", "same_profile_generated_dust"),
        CapabilityView::new("wallet.dust.registration.authorize", "ready")
            .text("mode", "standalone")
            .text("custody", "protected_role_2")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.dust.registration.submit", "ready")
            .text("mode", "standalone")
            .text("finality", "canonical_finalized_inclusion")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.dust.registration.start_submission", "ready")
            .text("mode", "standalone")
            .text("execution", "adapter_worker")
            .boolean("confirmationRequired", true),
        CapabilityView::new("wallet.dust.registration.draft", "ready")
            .text("mode", "standalone")
            .text("material", "public_preview_only"),
        CapabilityView::new("wallet.dust.registration.status", "ready")
            .text("mode", "standalone")
            .text("readiness", "requires_separate_dust_sync"),
        CapabilityView::new("wallet.dust.registration.cancel_submission", "ready")
            .text("mode", "standalone")
            .text("boundary", "pre_broadcast_only"),
        CapabilityView::new("wallet.dust.registration.reconcile_submission", "ready")
            .text("mode", "standalone")
            .text("scope", "finalized_chain"),
        CapabilityView::new("wallet.shielded.sync.status", "ready")
            .text("mode", "standalone")
            .texts("sources", &["simulated", "live", "cached", "unavailable"]),
        CapabilityView::new("wallet.shielded.sync.start", "ready")
            .text("mode", "standalone")
            .text("execution", "adapter_worker"),
        CapabilityView::new("wallet.shielded.sync.cancel", "ready")
            .text("mode", "standalone")
            .text("checkpoint", "resumable"),
        CapabilityView::new("vault.total_locked", "ready")
            .text("mode", "standalone")
            .text("state", state_persistence)
            .boolean("settlesOnMidnight", false),
        CapabilityView::new("vault.locks.list", "ready")
            .text("mode", "standalone")
            .text("state", state_persistence)
            .boolean("settlesOnMidnight", false),
        CapabilityView::new("vault.contract_state.decode", "ready")
            .text("mode", "native")
            .text("source", "pinned_layout_tagged_midnight_state")
            .boolean("mutates", false),
        CapabilityView::new("vault.contract_state.read", "composition_dependent")
            .text("mode", "native")
            .texts(
                "sources",
                &[
                    "deterministic_simulation",
                    "node_anchored_indexer",
                    "finalized_node_replay",
                ],
            )
            .texts(
                "stateAuthentication",
                &[
                    "deterministic_simulation",
                    "indexer_supplied_not_proven",
                    "canonical_finalized_replay",
                ],
            )
            .boolean("mutates", false),
        CapabilityView::new("vault.contract_call.prepare", call_status)
            .text("mode", call_mode)
            .texts("operations", call_operations)
            .text("requiresStateAuthentication", call_authentication)
            .boolean("privateMaterialExposed", false),
        CapabilityView::new("vault.contract_call.authorize", call_status)
            .text("mode", call_mode)
            .boolean("confirmationRequired", true)
            .text("intent", "AUTHORIZE_PASSPORT_VAULT_CALL"),
        CapabilityView::new("vault.contract_call.draft", call_status)
            .text("mode", call_mode)
            .boolean("serializedTransactionExposed", false),
        CapabilityView::new("vault.contract_call.submit", call_status)
            .text("mode", call_mode)
            .boolean("confirmationRequired", true)
            .text("intent", "SUBMIT_PASSPORT_VAULT_CALL"),
        CapabilityView::new("vault.contract_call.start_submission", call_status)
            .text("mode", call_mode)
            .text("execution", "adapter_worker"),
        CapabilityView::new("vault.contract_call.submission_status", call_status)
            .text("mode", call_mode),
        CapabilityView::new("vault.contract_call.submission_history", call_status)
            .text("mode", call_mode)
            .text("persistence", history_persistence),
        CapabilityView::new("vault.contract_call.cancel_submission", call_status)
            .text("mode", call_mode)
            .text("boundary", "pre_broadcast_only"),
        CapabilityView::new("vault.contract_call.reconcile_submission", call_status)
            .text("mode", call_mode)
            .text("scope", reconciliation_scope),
        CapabilityView::new("vault.credentials.list", "ready")
            .text("mode", "standalone")
            .text("aliasFor", "credential.list"),
        CapabilityView::new("vault.lock.create", "ready")
            .text("mode", "standalone")
            .boolean("confirmationRequired", true)
            .text("intent", "CREATE_PASSPORT_VAULT_LOCK"),
        CapabilityView::new("vault.deposit", "ready")
            .text("mode", "standalone")
            .boolean("confirmationRequired", true)
            .text("intent", "DEPOSIT_TO_PASSPORT_VAULT"),
        CapabilityView::new("vault.claim", "ready")
            .text("mode", "standalone")
            .boolean("confirmationRequired", true)
            .text("intent", "CLAIM_FROM_PASSPORT_VAULT")
            .text("credentialPolicy", "digital-passport:v1")
            .text("replayProtection", "per_lock_credential_root"),
        CapabilityView::new("vault.withdraw", "ready")
            .text("mode", "standalone")
            .boolean("confirmationRequired", true)
            .text("intent", "WITHDRAW_FROM_PASSPORT_VAULT"),
        CapabilityView::new("identity.request.route", "ready")
            .text("mode", "standalone")
            .texts(
                "inputs",
                &["openid-credential-offer", "registered_openid4vp"],
            )
            .text("unknownOpenid4vp", "fail_closed")
            .boolean("requestUriExposed", false),
        CapabilityView::new("identity.login", "ready")
            .text("mode", "standalone")
            .text("aliasFor", "identity.authentication.prepare"),
        CapabilityView::new("identity.authentication.prepare", "ready")
            .text("mode", "standalone")
            .text("standard", "SIOPv2 draft 13")
            .text("requestMode", "by_reference")
            .text("responseMode", "direct_post")
            .text("responseType", "id_token")
            .boolean("secretsExposed", false),
        CapabilityView::new("identity.authentication.accept", "ready")
            .text("mode", "standalone")
            .boolean("confirmationRequired", true)
            .texts("algorithms", &["EdDSA", "ES256"])
            .boolean("secretsExposed", false),
        CapabilityView::new("identity.authentication.refuse", "ready").text("mode", "standalone"),
        CapabilityView::new("identity.authentication.get", "ready")
            .text("mode", "standalone")
            .boolean("secretsExposed", false),
        CapabilityView::new("identity.authentication.list", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile")
            .boolean("secretsExposed", false),
        CapabilityView::new("credential.receive", "ready")
            .text("mode", "standalone")
            .text("source", "public_fixture"),
        CapabilityView::new("credential.request", "ready")
            .text("mode", "standalone")
            .text("aliasFor", "credential.receive"),
        CapabilityView::new("credential.list", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile"),
        CapabilityView::new("credential.get", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile")
            .boolean("rawCredentialExposed", false),
        CapabilityView::new("credential.reverify", "ready")
            .text("mode", "standalone")
            .texts(
                "stages",
                &[
                    "structural",
                    "issuer",
                    "proof",
                    "temporal",
                    "status",
                    "schema",
                    "trust",
                ],
            )
            .object(
                "compactPolicy",
                vec![
                    text("issuer", "did_assertion_method_and_jubjub_key"),
                    text("temporal", "current_time_and_expiry"),
                    text("trust", "pinned_standalone_anchor"),
                    text("status", "not_checked"),
                ],
            ),
        CapabilityView::new("credential.verify", "ready")
            .text("mode", "standalone")
            .text("aliasFor", "credential.reverify"),
        CapabilityView::new("credential.delete", "ready")
            .text("mode", "standalone")
            .boolean("confirmationRequired", true),
        CapabilityView::new("credential.disclosure.candidates", "ready")
            .text("mode", "standalone")
            .boolean("claimValuesExposed", false),
        CapabilityView::new("credential.disclosure.preview", "ready")
            .text("mode", "standalone")
            .boolean("generatesPresentation", false)
            .boolean("claimValuesExposed", false),
        CapabilityView::new("credential.issuance.prepare", "ready")
            .text("mode", "standalone")
            .text("standard", "OpenID4VCI 1.0 Final")
            .text("offerMode", "embedded"),
        CapabilityView::new("credential.issuance.accept", "ready")
            .text("mode", "standalone")
            .text("grant", "pre-authorized_code")
            .boolean("confirmationRequired", true)
            .text("proof", "jwt"),
        CapabilityView::new("credential.issuance.refuse", "ready").text("mode", "standalone"),
        CapabilityView::new("credential.issuance.get", "ready")
            .text("mode", "standalone")
            .boolean("secretsExposed", false),
        CapabilityView::new("credential.issuance.list", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile")
            .boolean("secretsExposed", false),
        CapabilityView::new("credential.presentation.prepare", "ready")
            .text("mode", "standalone")
            .text("standard", "OpenID4VP 1.0 Final")
            .text("query", "DCQL")
            .text("requestMode", "by_reference")
            .boolean("claimValuesExposed", false),
        {
            let capability =
                CapabilityView::new("credential.presentation.accept", presentation_status)
                    .text("mode", "standalone")
                    .boolean("confirmationRequired", true)
                    .text("holderAuthorization", "current_managed_jubjub_method")
                    .boolean(
                        "proofAvailable",
                        context.compact_presentation_proof_available,
                    )
                    .text("artifactRootEnvironment", "OXID_PRESENTATION_ARTIFACTS_DIR")
                    .boolean(
                        "generatesPresentation",
                        context.compact_presentation_proof_available,
                    );
            if context.compact_presentation_proof_available {
                capability.null("blocker")
            } else {
                capability.text("blocker", "https://github.com/MediaNoxLabs/oxid/issues/28")
            }
        },
        CapabilityView::new("credential.presentation.refuse", "ready").text("mode", "standalone"),
        CapabilityView::new("credential.presentation.get", "ready")
            .text("mode", "standalone")
            .boolean("secretsExposed", false),
        CapabilityView::new("credential.presentation.list", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile")
            .boolean("secretsExposed", false),
        CapabilityView::new("did.create", "ready")
            .text("mode", "development_only")
            .texts("networks", &["undeployed"])
            .texts("initialMethods", &["ed25519", "p256", "jubjub"]),
        CapabilityView::new("did.resolve", "ready")
            .text("mode", "standalone")
            .texts("sources", &["standalone", "live"]),
        CapabilityView::new("did.list", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile"),
        CapabilityView::new("did.get", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile"),
        CapabilityView::new("did.forget", "ready")
            .text("mode", "standalone")
            .text("scope", "active_profile"),
        CapabilityView::new("did.update", "ready")
            .text("mode", "development_only")
            .texts(
                "operations",
                &[
                    "addAlsoKnownAs",
                    "removeAlsoKnownAs",
                    "addVerificationMethod",
                    "updateVerificationMethod",
                    "removeVerificationMethod",
                    "addVerificationRelationship",
                    "removeVerificationRelationship",
                    "addService",
                    "updateService",
                    "removeService",
                ],
            )
            .boolean("confirmationRequired", true),
        CapabilityView::new("did.sign", "ready")
            .text("mode", "development_only")
            .texts("algorithms", &["ed25519", "p256", "jubjub"])
            .boolean("confirmationRequired", true),
        CapabilityView::new("did.deactivate", "ready")
            .text("mode", "development_only")
            .boolean("confirmationRequired", true),
        CapabilityView::new("diagnostics.snapshot", "superseded")
            .text("use", "system.diagnostics.snapshot"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest() -> Vec<CapabilityView> {
        capability_manifest(CapabilityManifestContext::new(
            false,
            "deterministic_simulation",
            "process_local",
        ))
    }

    #[test]
    fn method_names_are_unique_and_non_empty() {
        let mut methods = test_manifest()
            .into_iter()
            .map(|capability| capability.method())
            .collect::<Vec<_>>();
        assert!(methods.iter().all(|method| !method.is_empty()));
        let count = methods.len();
        methods.sort_unstable();
        methods.dedup();
        assert_eq!(methods.len(), count);
    }

    #[test]
    fn every_confirmation_bearing_headless_method_is_declared() {
        let manifest = test_manifest();
        let declared = |method| {
            manifest
                .iter()
                .find(|capability| capability.method() == method)
                .is_some_and(CapabilityView::confirmation_required)
        };
        for method in [
            "system.diagnostics.clear",
            "wallet.key.sign",
            "wallet.key.delete",
            "wallet.transaction.authorize_unshielded",
            "wallet.transaction.authorize_shielded",
            "wallet.transaction.submit_unshielded",
            "wallet.transaction.send_unshielded",
            "wallet.transaction.submit_shielded",
            "wallet.transaction.send_shielded",
            "wallet.transaction.start_submission",
            "wallet.dust.registration.authorize",
            "wallet.dust.registration.submit",
            "wallet.dust.registration.start_submission",
            "vault.contract_call.authorize",
            "vault.contract_call.submit",
            "vault.lock.create",
            "vault.deposit",
            "vault.claim",
            "vault.withdraw",
            "identity.authentication.accept",
            "credential.delete",
            "credential.issuance.accept",
            "credential.presentation.accept",
            "did.update",
            "did.sign",
            "did.deactivate",
        ] {
            assert!(
                declared(method),
                "{method} must declare confirmationRequired"
            );
        }
    }

    #[test]
    fn values_are_public_closed_composition_facts() {
        let rendered = test_manifest()
            .iter()
            .flat_map(CapabilityView::facts)
            .map(|fact| format!("{}={}", fact.key(), fact.value().display_text()))
            .collect::<Vec<_>>()
            .join(" ");
        for forbidden in [
            "profileId=",
            "credentialId=",
            "requestUri=",
            "payload=",
            "privateKey=",
            "seed=",
            "openid4vp://",
            "openid-credential-offer://",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "manifest leaked forbidden field {forbidden}"
            );
        }
    }

    #[test]
    fn unknown_composition_labels_fail_closed() {
        let manifest = capability_manifest(CapabilityManifestContext::new(
            false,
            "https://private.example/vault",
            "profile-secret-store",
        ));
        let vault = manifest
            .iter()
            .find(|capability| capability.method() == "vault.total_locked")
            .expect("vault capability");
        assert!(vault.facts().iter().any(|fact| {
            fact.key() == "state"
                && fact.value() == &CapabilityValue::Text("unavailable".to_owned())
        }));
        let prepare = manifest
            .iter()
            .find(|capability| capability.method() == "vault.contract_call.prepare")
            .expect("call capability");
        assert_eq!(prepare.status(), "composition_dependent");
        assert!(prepare.facts().iter().any(|fact| {
            fact.key() == "mode" && fact.value() == &CapabilityValue::Text("unavailable".to_owned())
        }));
        assert!(prepare.facts().iter().any(|fact| {
            fact.key() == "requiresStateAuthentication"
                && fact.value() == &CapabilityValue::Text("unavailable".to_owned())
        }));
        let rendered = manifest
            .iter()
            .flat_map(CapabilityView::facts)
            .map(|fact| fact.value().display_text())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!rendered.contains("private.example"));
        assert!(!rendered.contains("profile-secret-store"));
    }
}
