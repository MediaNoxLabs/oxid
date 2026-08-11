// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::io;

use oxid_headless::HeadlessWallet;

fn main() -> Result<(), oxid_headless::HeadlessIoError> {
    let wallet = HeadlessWallet::new(oxid_composition::compose_headless());
    wallet.run(io::stdin().lock(), io::stdout().lock())
}
