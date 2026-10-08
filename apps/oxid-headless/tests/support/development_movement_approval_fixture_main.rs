// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{io, process::ExitCode};

use oxid_composition::compose_native_headless_process_with_development_movement_approval_from_environment;
use oxid_headless::HeadlessWallet;

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return ExitCode::FAILURE,
    };
    let _runtime_guard = runtime.enter();
    let application =
        match compose_native_headless_process_with_development_movement_approval_from_environment()
        {
            Ok(application) => application,
            Err(_) => return ExitCode::FAILURE,
        };
    let wallet = HeadlessWallet::new(application);
    if wallet.run(io::stdin().lock(), io::stdout().lock()).is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
