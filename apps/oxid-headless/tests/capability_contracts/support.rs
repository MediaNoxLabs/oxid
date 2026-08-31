// SPDX-License-Identifier: Apache-2.0

use oxid_headless::HeadlessWallet;
use serde_json::Value;

pub(super) fn execute(input: &str) -> Vec<Value> {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    execute_with_wallet(&wallet, input)
}

pub(super) fn execute_with_wallet(wallet: &HeadlessWallet, input: &str) -> Vec<Value> {
    let mut output = Vec::new();
    wallet
        .run(input.as_bytes(), &mut output)
        .expect("protocol exchange should succeed");

    String::from_utf8(output)
        .expect("protocol output should be UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each response should be JSON"))
        .collect()
}
