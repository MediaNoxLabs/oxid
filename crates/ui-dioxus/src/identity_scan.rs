// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) const fn identity_scan_is_admitted(scan_busy: bool, request_pending: bool) -> bool {
    !scan_busy && !request_pending
}

pub(super) struct IdentityScanDependencies {
    pub(super) services: WalletUiServices,
    pub(super) profile_id: String,
    pub(super) scanner: Arc<dyn QrScannerPort>,
    pub(super) router: Arc<dyn RouteIdentityRequestUseCase>,
}

pub(super) struct IdentityScanSignals {
    pub(super) busy: Signal<bool>,
    pub(super) notice: Signal<Option<String>>,
    pub(super) pending_request: Signal<Option<PendingIdentityRequest>>,
    pub(super) pending_payment: Signal<Option<PendingPaymentRequest>>,
    pub(super) navigation: Signal<RouteStack>,
    pub(super) header_menu: Signal<HeaderMenu>,
}

pub(super) fn start_identity_scan(
    dependencies: IdentityScanDependencies,
    signals: IdentityScanSignals,
) {
    let IdentityScanDependencies {
        services,
        profile_id,
        scanner,
        router,
    } = dependencies;
    let IdentityScanSignals {
        mut busy,
        mut notice,
        mut pending_request,
        mut pending_payment,
        mut navigation,
        mut header_menu,
    } = signals;
    if !identity_scan_is_admitted(busy(), pending_request.read().is_some()) {
        return;
    }
    busy.set(true);
    notice.set(None);
    header_menu.set(HeaderMenu::Closed);
    spawn(async move {
        match scanner.scan().await {
            Ok(payload) => {
                if !identity_scan_is_admitted(false, pending_request.read().is_some()) {
                    busy.set(false);
                    return;
                }
                let request_uri = payload.into_inner();
                if is_public_recipient_candidate(&request_uri) {
                    let account_services = services.clone();
                    let account_profile_id = profile_id.clone();
                    let account = run_ui_blocking(move || {
                        account_services
                            .get_wallet_account()
                            .execute(WalletAccountQuery {
                                profile_id: account_profile_id,
                            })
                    })
                    .await;
                    match account {
                        Ok(Ok(account)) => match scanned_recipient_update(
                            &account.network_id,
                            request_uri,
                        ) {
                            Ok(update) => {
                                pending_payment.set(Some(PendingPaymentRequest {
                                    recipient: update.recipient,
                                }));
                                navigation.write().push(Route::Send);
                                notice.set(Some(
                                    "QR recognized as a public NIGHT payment request. Review the recipient and choose an amount; nothing has been sent."
                                        .to_owned(),
                                ));
                            }
                            Err(message) => notice.set(Some(message)),
                        },
                        Ok(Err(_)) | Err(_) => notice.set(Some(
                            "The active wallet network could not be checked. Retry after Wallet is available; nothing was imported."
                                .to_owned(),
                        )),
                    }
                    busy.set(false);
                    return;
                }
                match router.execute(RouteIdentityRequestCommand {
                    request_uri: request_uri.clone(),
                }) {
                    Ok(kind) => {
                        pending_request.set(Some(PendingIdentityRequest { kind, request_uri }));
                        navigation.write().route_scanned_identity_request(kind);
                        notice.set(Some(format!(
                            "QR recognized as {}. Review the request before consent.",
                            ui::identity_request_kind(kind)
                        )));
                    }
                    Err(error) => {
                        notice.set(Some(identity_request_routing_message(error)));
                    }
                }
            }
            Err(error) => {
                notice.set(Some(qr_scan_message(error)));
            }
        }
        busy.set(false);
    });
}
