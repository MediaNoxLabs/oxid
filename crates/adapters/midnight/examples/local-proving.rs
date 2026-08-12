// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(not(feature = "proving-bench"))]
compile_error!("run this example with --features proving-bench");

use std::{path::PathBuf, process::ExitCode, sync::atomic::AtomicBool};

use oxid_adapter_midnight::{MidnightLocalProvingConfig, run_local_proving_fixture};
use serde::Serialize;

const CACHE_ENV: &str = "OXID_MIDNIGHT_PROVING_CACHE_DIR";
const ITERATIONS_ENV: &str = "OXID_PROVING_ITERATIONS";
const MAX_ITERATIONS: u8 = 16;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    target: &'static str,
    phase: &'static str,
    iteration: u8,
    circuit_k: u8,
    circuit_rows: u64,
    cache_bytes: u64,
    preparation_millis: u128,
    proving_millis: u128,
    proof_bytes: usize,
    sealed_transaction_bytes: usize,
    sealed_round_trip: bool,
}

fn target() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }
}

fn main() -> ExitCode {
    let cache = match std::env::var_os(CACHE_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            eprintln!("{CACHE_ENV} must name an absolute app-private cache directory");
            return ExitCode::FAILURE;
        }
    };
    let config = match MidnightLocalProvingConfig::new(cache) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("local proving configuration failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("local proving runtime initialization failed");
            return ExitCode::FAILURE;
        }
    };
    let cancellation = AtomicBool::new(false);
    let iterations = match bounded_iterations() {
        Ok(iterations) => iterations,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    for iteration in 1..=iterations {
        let phase = if iteration == 1 { "first" } else { "warm" };
        let result = runtime.block_on(run_local_proving_fixture(&config, &cancellation));
        let fixture = match result {
            Ok(fixture) => fixture,
            Err(error) => {
                eprintln!("local proving fixture failed: {error}");
                return ExitCode::FAILURE;
            }
        };
        let metrics = fixture.metrics();
        let report = Report {
            target: target(),
            phase,
            iteration,
            circuit_k: metrics.circuit_k(),
            circuit_rows: metrics.circuit_rows(),
            cache_bytes: metrics.cache_bytes(),
            preparation_millis: metrics.preparation_elapsed().as_millis(),
            proving_millis: metrics.proving_elapsed().as_millis(),
            proof_bytes: fixture.proof_bytes(),
            sealed_transaction_bytes: fixture.sealed_transaction_bytes(),
            sealed_round_trip: true,
        };
        match serde_json::to_string(&report) {
            Ok(line) => println!("{line}"),
            Err(_) => {
                eprintln!("local proving report serialization failed");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn bounded_iterations() -> Result<u8, &'static str> {
    let Some(value) = std::env::var_os(ITERATIONS_ENV) else {
        return Ok(2);
    };
    let value = value
        .into_string()
        .map_err(|_| "OXID_PROVING_ITERATIONS must be valid Unicode")?;
    let iterations = value
        .parse::<u8>()
        .map_err(|_| "OXID_PROVING_ITERATIONS must be an integer from 1 through 16")?;
    if !(1..=MAX_ITERATIONS).contains(&iterations) {
        return Err("OXID_PROVING_ITERATIONS must be an integer from 1 through 16");
    }
    Ok(iterations)
}
