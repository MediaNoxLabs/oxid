# Standalone headless NIGHT funding

This development-only scenario gives two fresh headless wallets a fixed NIGHT
grant, explicitly registers each wallet for DUST, and verifies positive DUST
within ten minutes. It uses localhost only. It does not use a phone, simulator,
Tailnet, PreProd, shielded funding, or a production faucet.

## Fast protocol check

The deterministic protocol tests need no Docker services:

```sh
cargo test -p oxid-headless --features standalone-faucet --lib
```

The feature compiles a separate binary. The binary still refuses to start until
the development opt-in is present.

## Run the narrow faucet

Start or verify the repository-owned standalone stack first:

```sh
just standalone-up
just standalone-status
just standalone-faucet
```

`just standalone-faucet` owns only its private state under
`target/standalone-faucet`; it does not own or stop the Docker stack. Send one
JSON object per line. Health and shutdown take empty parameters:

```json
{"protocol":"oxid.standalone-faucet.v1","id":"health","method":"faucet.health","params":{}}
```

Use the public unshielded address returned by a fresh wallet's
`wallet.account.derive` response:

```json
{"protocol":"oxid.standalone-faucet.v1","id":"fund-a","method":"faucet.fund","params":{"requestId":"demo-wallet-a","recipientAddress":"mn_addr_undeployed1..."}}
```

The caller cannot supply the amount, realm, route, seed, mnemonic, or key. A
successful receipt always represents 50,000 NIGHT and finalized inclusion. The
same request identifier and recipient, or a second identifier for the same
recipient, returns the retained receipt without another transfer. Reusing an
identifier with another recipient fails. Receipts are bounded and process-local;
restarting the faucet permits another grant.

## Run the two-wallet acceptance

This is an explicit live, on-demand run. It creates temporary state for two
OS-random wallets and removes only that state after the test. It expects the
standalone stack to be running and does not stop it:

```sh
just standalone-up
OXID_ENABLE_LIVE_STANDALONE_FAUCET_E2E=1 \
  just standalone-faucet-headless-e2e
```

The run fails unless both wallets:

1. derive distinct undeployed unshielded addresses;
2. receive and observe exactly the fixed grant;
3. explicitly prepare, authorize, and submit DUST registration; and
4. observe positive generated DUST within ten minutes.

Only closed timings and counts are printed. Wallet roots, paths, addresses, and
transaction material are not emitted. DUST timing is acceptance evidence for
the grant size; it is not a stable performance benchmark.

Stop the standalone stack only if you started and therefore own it:

```sh
just standalone-down
```
