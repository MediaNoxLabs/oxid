// SPDX-License-Identifier: Apache-2.0

use super::*;

impl HeadlessWallet {
    pub(super) fn record_diagnostic(&self, code: DiagnosticCode, severity: DiagnosticSeverity) {
        self.application.diagnostic_events().record(code, severity);
    }

    pub(super) fn capabilities(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "system.capabilities does not accept parameters",
            ));
        }

        Dispatch::continue_with(Response::success(
            request.id,
            json!({
                "implementation": {
                    "name": "oxid-headless",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "methods": capability_manifest(
                    self.application.compact_presentation_proof_available(),
                    self.application.passport_vault_call_mode(),
                    self.application.passport_vault_state_persistence(),
                ),
                "passportVaultState": {
                    "mode": "standalone",
                    "persistence": self.application.passport_vault_state_persistence(),
                    "settlesOnMidnight": false,
                },
                "passportVaultContractCalls": {
                    "mode": self.application.passport_vault_call_mode(),
                    "contractAddressHex": self.application.passport_vault_call_contract_address_hex(),
                    "settlesOnMidnight": self.application.passport_vault_call_mode() == "native_settlement"
                },
                "custodyMode": "development_only",
                "compatibilityAliases": ["quit", "exit"]
            }),
        ))
    }

    pub(super) fn diagnostics_snapshot(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "system.diagnostics.snapshot");
        }
        match self.application.get_diagnostic_snapshot().execute() {
            Ok(snapshot) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "diagnostics": diagnostic_snapshot_value(&snapshot) }),
            )),
            Err(_) => Dispatch::continue_with(Response::error(
                request.id,
                "diagnostics_unavailable",
                "local diagnostics are unavailable",
            )),
        }
    }

    pub(super) fn clear_diagnostics(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<ClearDiagnosticsParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "system.diagnostics.clear requires confirmed and intent",
                ));
            }
        };
        match self
            .application
            .clear_diagnostics()
            .execute(ClearDiagnosticsCommand {
                confirmed: params.confirmed,
                intent: params.intent,
            }) {
            Ok(cleared) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "clearedEvents": cleared.cleared_events }),
            )),
            Err(_) => Dispatch::continue_with(Response::error(
                request.id,
                "confirmation_required",
                "clearing local diagnostics requires exact confirmation",
            )),
        }
    }

    pub(super) fn quit(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "system.quit does not accept parameters",
            ));
        }

        Dispatch::exit(Response::success(
            request.id,
            json!({ "shuttingDown": true }),
        ))
    }
}
