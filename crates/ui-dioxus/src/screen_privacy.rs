// SPDX-License-Identifier: Apache-2.0

#[cfg(any(target_os = "ios", target_os = "android", test))]
use oxid_diagnostics_application::{DiagnosticCode, DiagnosticEventSinkPort, DiagnosticSeverity};
#[cfg(any(target_os = "ios", target_os = "android", test))]
use oxid_platform_ports::ScreenPrivacyPort;

use crate::Route;

pub(crate) const fn route_forces_screen_privacy(route: Route) -> bool {
    matches!(
        route,
        Route::Settings | Route::BackupRecovery | Route::Documents | Route::CredentialRequest
    )
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
pub(crate) fn protect_suspended_snapshot(
    screen_privacy: &dyn ScreenPrivacyPort,
    diagnostic_events: &dyn DiagnosticEventSinkPort,
) {
    if screen_privacy.set_protected(true).is_err() {
        diagnostic_events.record(
            DiagnosticCode::ScreenPrivacyActivationFailed,
            DiagnosticSeverity::Error,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_platform_ports::ScreenPrivacyError;

    use super::*;

    #[test]
    fn backup_and_credential_routes_force_native_snapshot_protection() {
        assert!(route_forces_screen_privacy(Route::Settings));
        assert!(route_forces_screen_privacy(Route::Documents));
        assert!(route_forces_screen_privacy(Route::CredentialRequest));
        assert!(!route_forces_screen_privacy(Route::Home));
        assert!(!route_forces_screen_privacy(Route::Wallet));
    }

    struct FixedScreenPrivacy(Result<(), ScreenPrivacyError>);

    impl ScreenPrivacyPort for FixedScreenPrivacy {
        fn set_protected(&self, protected: bool) -> Result<(), ScreenPrivacyError> {
            assert!(protected, "suspend protection must always be enabled");
            self.0
        }
    }

    #[derive(Default)]
    struct RecordingDiagnosticSink {
        events: Mutex<Vec<(DiagnosticCode, DiagnosticSeverity)>>,
    }

    impl DiagnosticEventSinkPort for RecordingDiagnosticSink {
        fn record(&self, code: DiagnosticCode, severity: DiagnosticSeverity) {
            self.events
                .lock()
                .expect("diagnostic events")
                .push((code, severity));
        }
    }

    #[test]
    fn suspended_snapshot_failure_records_only_the_closed_diagnostic() {
        let diagnostics = RecordingDiagnosticSink::default();

        protect_suspended_snapshot(
            &FixedScreenPrivacy(Err(ScreenPrivacyError::Failed)),
            &diagnostics,
        );

        assert_eq!(
            *diagnostics.events.lock().expect("diagnostic events"),
            vec![(
                DiagnosticCode::ScreenPrivacyActivationFailed,
                DiagnosticSeverity::Error,
            )]
        );
    }

    #[test]
    fn suspended_snapshot_success_does_not_record_a_diagnostic() {
        let diagnostics = RecordingDiagnosticSink::default();

        protect_suspended_snapshot(&FixedScreenPrivacy(Ok(())), &diagnostics);

        assert!(
            diagnostics
                .events
                .lock()
                .expect("diagnostic events")
                .is_empty()
        );
    }
}
