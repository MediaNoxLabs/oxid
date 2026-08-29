// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{io, process::ExitCode};

use oxid_headless::HeadlessWallet;

fn main() -> ExitCode {
    let application = match oxid_composition::compose_native_headless_process_from_environment() {
        Ok(application) => application,
        Err(error) => {
            eprintln!("oxid-headless startup failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let wallet = HeadlessWallet::new(application);
    if wallet.run(io::stdin().lock(), io::stdout().lock()).is_err() {
        eprintln!("oxid-headless I/O failed");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
