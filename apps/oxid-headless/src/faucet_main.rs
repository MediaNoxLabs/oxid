// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{env, io, process::ExitCode};

use oxid_headless::StandaloneFaucet;

fn main() -> ExitCode {
    if env::var("OXID_ENABLE_STANDALONE_FAUCET").as_deref() != Ok("1") {
        eprintln!(
            "standalone faucet is disabled; set OXID_ENABLE_STANDALONE_FAUCET=1 for an isolated development stack"
        );
        return ExitCode::FAILURE;
    }
    let application = match oxid_composition::compose_headless_public_genesis_local_standalone() {
        Ok(application) => application,
        Err(error) => {
            eprintln!("standalone faucet startup failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut faucet = match StandaloneFaucet::new(application) {
        Ok(faucet) => faucet,
        Err(error) => {
            eprintln!("standalone faucet startup failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    if faucet.run(io::stdin().lock(), io::stdout().lock()).is_err() {
        eprintln!("standalone faucet I/O failed");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
