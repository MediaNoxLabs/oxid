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

## Run the loopback HTTP adapter

The HTTP binary reuses the same fixed-grant dispatcher and private authority.
It is not a second faucet implementation:

```sh
just standalone-up
just standalone-faucet-http
```

It listens on `http://127.0.0.1:36301` and supports only:

```text
GET /health
POST /fund  Content-Type: application/json
```

The `/fund` body is the same closed public parameter object:

```json
{"requestId":"demo-wallet-a","recipientAddress":"mn_addr_undeployed1..."}
```

The listener accepts one request at a time, closes every response, and rejects
large, streaming, ambiguous, or non-HTTP/1.1 input. It cannot bind to a
non-loopback address. Tailnet exposure is not part of this command; it is owned
by follow-up issue #540.

## Desktop receive and funding handoff

A desktop artifact explicitly built with `desktop,standalone-development,standalone-local`
shows the active protected profile's available receive rails and defaults to its
unshielded undeployed NIGHT rail. Until receive-request ingress lands, that
rail's QR, Copy, and Share controls all expose the validated raw undeployed
Bech32m address. The sheet labels the profile, Midnight network, route class,
and selected asset before export.

With the loopback HTTP faucet already running, the development-only **Open
development funding** action opens `http://127.0.0.1:36301`. It is a local
operator convenience, not a grant result: sync to observe an authoritative
balance and complete DUST registration separately. The app never embeds,
displays, logs, or screenshots a Tailnet hostname; Tailnet funding remains the
owner-operated setup-QR journey below.

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

To qualify the same exact grant through the loopback HTTP boundary without
repeating DUST registration, run:

```sh
OXID_ENABLE_LIVE_STANDALONE_FAUCET_E2E=1 \
  just standalone-faucet-http-headless-e2e
```

This second acceptance creates two other fresh isolated wallet roots, funds
them through HTTP, and requires each synchronized balance to equal the fixed
grant. It does not make a browser, Tailnet, simulator, or phone claim.

## Owner-authorized Tailnet HTTPS discovery

This is an on-demand development acceptance, not a phone, Android, PreProd, or
public-Internet path. It exposes the same loopback-only faucet through one
receipt-scoped Tailscale Serve HTTPS port selected at runtime. The loopback
adapter serves both the page and generated QR, so the Tailnet owns only one
reverse-proxy route and does not depend on unsupported macOS file serving. It
dynamically uses MagicDNS but never prints or commits the hostname. Existing
Serve routes are preserved; cleanup refuses any drift and removes only the
receipt-proven new port.

With an owner-approved local standalone stack already running:

```sh
just standalone-faucet-tailnet-start
just standalone-faucet-tailnet-status
OXID_ENABLE_OWNER_TAILNET_FAUCET_ACCEPTANCE=1 \
OXID_FAUCET_RECIPIENT_ADDRESS='mn_addr_undeployed1...' \
  just standalone-faucet-tailnet-accept
just standalone-faucet-tailnet-stop
```

The discovery page is responsive and offers only the fixed 50,000 NIGHT grant.
Its Tailnet-only setup QR contains protocol version, the exact `undeployed`
realm/fingerprint, and the dynamically selected HTTPS route. It cannot change
wallet, asset, realm, route, or amount. The acceptance performs HTTPS health
and one fixed funding request without a phone; its recipient is operator input
and is never retained in repository evidence. Always run `stop` before retrying
or ending the owner session.

Stop the standalone stack only if you started and therefore own it:

```sh
just standalone-down
```
