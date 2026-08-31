// SPDX-License-Identifier: Apache-2.0

use super::*;

impl HeadlessWallet {
    pub(super) fn list_networks(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.network.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_wallet_networks()
            .execute(WalletAccountQuery { profile_id })
        {
            Ok(networks) => Dispatch::continue_with(Response::success(
                request.id,
                network_list_value(&networks),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    pub(super) fn select_network(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SelectNetworkParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.network.select requires only a string networkId",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .select_wallet_network()
            .execute(SelectWalletNetworkCommand {
                profile_id,
                network_id: params.network_id,
            }) {
            Ok(networks) => Dispatch::continue_with(Response::success(
                request.id,
                network_list_value(&networks),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    pub(super) fn get_account(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.account.get");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_account()
            .execute(WalletAccountQuery { profile_id })
        {
            Ok(account) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "account": account_value(&account) }),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    pub(super) fn derive_account(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeriveAccountParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.account.derive accepts only optional accountIndex and addressIndex integers",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .derive_wallet_account()
            .execute(DeriveWalletAccountCommand {
                profile_id,
                account_index: params.account_index,
                address_index: params.address_index,
            }) {
            Ok(account) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "account": derived_account_value(&account) }),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }
}
