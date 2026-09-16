// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{env, fs, process::ExitCode};

use oxid_headless::{DEFAULT_FAUCET_HTTP_ADDRESS, StandaloneFaucet, run_loopback_http};

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
    let address = env::var("OXID_STANDALONE_FAUCET_HTTP_ADDRESS")
        .unwrap_or_else(|_| DEFAULT_FAUCET_HTTP_ADDRESS.to_owned());
    let setup_svg = match env::var_os("OXID_STANDALONE_FAUCET_SETUP_SVG_PATH") {
        None => None,
        Some(path) => match fs::read(path) {
            Ok(bytes) if bytes.len() <= 128 * 1024 => Some(bytes),
            _ => {
                eprintln!("standalone faucet setup QR is unavailable or exceeds 128 KiB");
                return ExitCode::FAILURE;
            }
        },
    };
    if let Err(error) = run_loopback_http(&mut faucet, &address, setup_svg.as_deref()) {
        eprintln!("standalone faucet HTTP failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
