# A browser bridge instead of a web wallet — feasibility study

- **Studied**: 2026-08-20
- **Question**: instead of a wasm/web target of the wallet, can a Chrome
  extension bridge to the wallet on the user's phone over BT / mDNS / USB, so
  the phone stays sole custodian, the browser holds no keys, and the extension
  serves the well-known wallet Connector APIs to DApps?
- **Stated constraint**: no heavy infrastructure. A product principle.

## Verdict

**The custody half of the thesis is right, well-evidenced, and already shipped
by someone else in this exact ecosystem. The no-infrastructure half does not
survive contact with the browser platform or with Midnight's proving model. And
the transport the brief leads with — USB — is the one that cannot work.**

The recommendation is to **invert the build order**: ship a QR-based remote
approval channel first, settle the byte-signing question by ADR second, treat
the extension as a later optional accelerant. The reasoning is below, with the
numbers.

Four findings dominate everything else.

**1. On Midnight the phone must be a *prover*, not a signer — and that is
architectural, not incidental.** MPS-0024 states it plainly: *"asset movement
on Midnight is proof-authorized, not primarily signature-authorized… A
conventional signature or 2-of-3 approval flow does not, by itself, authorize
or execute the shielded spend."* Prover keys are *"often 10MB-20MB, sometimes
event 80MB and more"*, and the connector spec warns that *"sizes like this come
close to, or even exceed message size limits of different transport methods."*
MPS-0004 measures a ZSwap spend proof at *"~190ms on a 32-core server but…
5-30 seconds on a consumer laptop and **infeasible in WASM on mobile
browsers**."* That last clause cuts both ways: it rules out the phone as prover
behind any local transport, **and it independently kills the wasm-web-wallet
alternative on mobile.**

**2. "No infrastructure" is already untrue, and the real principle is
narrower.** Oxid requires an indexer, a node, and a proof server on
`OXID_MIDNIGHT_PROOF_SERVER_URL` (port 6300, which the docs say *"should not be
changed"*); ADR-0097's development phone profile terminates TLS through
Tailscale Serve. The blueprint's actual wording is *no mandatory Oxid-hosted
backend* — i.e. **no Oxid-operated relay that sees user traffic.** That is
achievable and worth defending. "No infrastructure" is not, and conflating them
will produce bad decisions. The escape hatch for proving already exists as a
specification: MPS-0004's TEE-attested delegated proving, which
`midnight-confidential-space-attestation` already implements with
`requireHwModel: ['GCP_AMD_SEV_SNP']` and attestation-gated `prove()`.

**3. FIDO built the proposed design, measured it, and abandoned it.** caBLE v1
carried data over BLE GATT. Adam Langley, who designed it: *"BLE GATT
connections were just too unreliable… the most reliable combination of phone
and desktop achieved only 95% connection success."* CTAP 2.2 hybrid keeps **one
BLE advertisement** — purely as a proximity proof, feeding the tunnel handshake
so the tunnel cannot be built without physical co-location — and moves all
payload to a WSS tunnel server. Google, Apple and Microsoft ship passkeys on
it. The inversion — **BLE for proximity, internet for bytes** — is the most
transferable lesson available.

**4. QR is the shipped answer here, and the closest precedent to this exact
design is the Cardano Foundation's own identity wallet.** Midnight now lives
inside the Lace extension behind `window.midnight`; Lace Mobile shipped in
April 2026 with **no documented extension↔mobile pairing**; and in July 2026
Lace shipped air-gapped signing with SeedSigner, which *"communicates entirely
through QR codes. There is no USB connection, Bluetooth, or Wi-Fi."*

## The precedent that matters most

`veridian-id/veridian-wallet` — the Cardano Foundation's mobile identity wallet
— already implements the architecture proposed here, over CIP-45:

- Wallet name `idw_p2p`, its own trackers, keys on the phone.
- `IdentityWalletConnect` **throws `Method not implemented` for the entire
  CIP-30 read surface** and serves only signing over the peer channel.
- Human-in-the-loop budget: **`MAX_SIGN_TIME = 3_600_000` ms — one hour** —
  polled at 1 s intervals, returning `TxSignError.TimeOut` on expiry.

An identity wallet that declines to serve reads and grants an hour for a human
to approve a signature is *exactly* the shape this study concludes is correct.
It exists, it is public, and it is worth reading before writing any code.

## The custody argument is strong, and it is not theoretical

On 24 December 2025 the Trust Wallet Chrome extension shipped a malicious
update that looped every wallet and exfiltrated mnemonics on unlock, disguised
as telemetry error metadata: ~1M+ users, **~$7–8.5M drained in under 48
hours**, delivered via a Chrome Web Store API key leaked in a GitHub-secrets
compromise. **An extension that holds no key material is structurally immune to
that attack.** That is a real, quantified reduction and it is the strongest
thing in the thesis.

Oxid's architecture is also unusually well-shaped for remote approval. The
transaction model is **intent-based, not byte-based**: `prepare_unshielded`
returns a structured `WalletTransferPreviewView` plus an opaque
`authorization_challenge`; per ADR-0027 *"chain-specific signed, balanced,
proven, and serialized transactions remain retained inside the Midnight
adapter"*, and the capability manifest asserts `serializedTransactionExposed:
false` on every method. **The draft never leaves the phone**, so a bridge
carrying only preview + challenge + verdict is genuinely low-privilege, and the
phone is inherently the authoritative renderer because it is the only party
holding the material.

### But the surface-area claim needs to be stated honestly

| Dimension | Wasm web wallet | Bridge extension | Better |
| --- | --- | --- | --- |
| Key exfiltration on compromise | Catastrophic (Trust Wallet) | **Impossible — no keys present** | **Extension** |
| Arbitrary-code delivery | Page load from your origin | Store update, auto-installed silently, **no prompt unless permissions change** | Wasm |
| Ambient privilege | One origin | The user's whole browsing context | Wasm |
| Transaction substitution | Yes | **Yes — identical** | Tie |
| Distribution chokepoint | Your hosting | Google; external CRX installs blocked on Windows/macOS | Wasm |
| Incident-response latency | Instant redeploy | Store review: days to weeks | Wasm |
| New native code on the custodian device | None | BLE peripheral / USB accessory / listening socket | Wasm |
| Fits existing gates | Rust, existing CI | New TypeScript outside every gate | Wasm |
| Can prove on the client at all | **No — "infeasible in WASM on mobile browsers"** | n/a (delegated) | Extension |

**A keyless bridge extension is a smaller custody surface and a larger total
surface.** It wins the category that produced the industry's browser-extension
losses, and loses or ties on most others. Worth saying plainly rather than
discovering later.

## What an MV3 extension can actually do

| Transport | Available? | Context | Persistent permission | Survives SW idle-kill |
| --- | --- | --- | --- | --- |
| **WebUSB** | ✅ incl. **extension service worker** | `requestDevice()` in a page; rest in SW | ✅ `getDevices()` | ✅ connection keeps the SW alive |
| **WebHID** | ✅ incl. extension SW | same | ✅ | ✅ |
| **Web Bluetooth** | ⚠️ **documents only — never in the SW** | side panel / options page | ❌ `getDevices()` is **flag-gated** | ❌ dies with the document |
| **WebSocket / fetch to LAN or loopback** | ✅ | SW + pages | via `host_permissions` | ✅ traffic resets the timer |
| **WebRTC data channel** | ✅ | offscreen doc (`WEB_RTC`) | n/a | ✅ |
| **Camera (QR reverse channel)** | ✅ | offscreen doc (`USER_MEDIA`), no lifetime limit — but the *grant* needs a full tab | n/a | ✅ |
| `chrome.sockets.tcp/udp/tcpServer` | ❌ Chrome-Apps only, deprecated 2020 | — | — | — |
| Direct Sockets | ❌ Isolated Web Apps only | — | — | — |
| `chrome.mdns` | ❌ **allowlisted to four hard-coded extension IDs** | — | — | — |
| Native messaging | ✅ | SW + pages | after an OS install | ✅ strongest |

**The hard noes shape the design more than the yeses:**

- **No raw TCP/UDP from any extension context**, and **no listening socket** —
  so the extension is always the client and **the phone must be the server**.
  That single fact eliminates a class of designs.
- **No mDNS, therefore no zero-config discovery.** Pairing must be
  out-of-band — which is also where a key can be exchanged.
- **No gesture-free first pairing.** Neither the service worker nor an
  offscreen document can hold user activation.
- **Chrome's extension docs cover WebUSB and WebHID in service workers
  explicitly and never mention Web Bluetooth in extensions at all.** The
  offscreen-document reason enum has `USER_MEDIA`, `WEB_RTC` and `GEOLOCATION`
  but **no Bluetooth reason**, which suggests offscreen is not a sanctioned
  host either. Standards position is worse: **WebKit formally "oppose",
  Mozilla "negative"** — Chromium-only, forever, and Linux users are excluded
  by default.
- **Native messaging is the only route to LAN sockets, mDNS or BLE without
  browser limits**, at the price of a signed, notarised, per-OS native binary
  installed by OS machinery, per-browser host manifests, an extension-ID
  allowlist, and a 1 MB inbound cap — which **80 MB prover keys blow through by
  two orders of magnitude.** Note that **both shipped precedents for this shape
  — Ledger Live Bridge and Trezor Bridge — were retired by their own vendors**,
  Ledger's as *"a big tech debt… hacky workaround"*.

⚠️ **Highest-risk volatile claim in this study:** extensions with correct
`host_permissions` are reportedly exempt from Chrome's Local Network Access
gate — but that is a Chrome-engineer statement on a mailing list, absent from
the public LNA documentation and from the WICG spec (which has **no extension
carve-out** and explicitly brings WebSockets in scope), and **it broke for
extensions in Chrome 142–143 before being fixed in 144.0.7512.0**. Anything
built on LAN transport rides this.

## Phone-side reality

| Transport | iOS | Android |
| --- | --- | --- |
| **BLE peripheral** | Foreground discovery only. Backgrounded, the local name is suppressed and service UUIDs move to Apple's **overflow area** — a proprietary `ff 4c 00 01` + 16-byte hashed bitmask, *"discovered only by an iOS device that is explicitly scanning for them."* **Chrome cannot match a service-UUID filter against a backgrounded iPhone.** RPA rotates ~15 min, so re-identification needs bonding | Works, but peripheral role is **chipset-dependent**. Cheapest permissions of any option: `BLUETOOTH_ADVERTISE` + `BLUETOOTH_CONNECT`, **no location** |
| **LAN listener** | **No background mode covers a network server.** Apple's own DTS answer to "how do I run a server in the background?" is *"You can't."* Using `voip`/`audio`/`location` to keep a socket alive is a documented 2.5.4 violation | Works with a typed foreground service; **Android 17 makes `ACCESS_LOCAL_NETWORK` mandatory even to *accept* a TCP connection**, and denial appears as timeouts, not errors |
| **USB** | **Definitively no** — see below | AOA works phone-side; the desktop side is the blocker |
| **QR** | Works | Works, but the current Android scanner must be replaced |

**BLE throughput is fine for our payload sizes** — ~2.7 kB/s worst, 10–30 kB/s
realistic through Chrome, ~50 kB/s best, so 100 KB lands in **3–10 s**, two
orders of magnitude better than QR. Design notes: Android 14+ requests MTU 517
on the first client request and **ignores all later requests** on that
connection (clamp to `min(supported, 517) − 5`; MTU is the single biggest
throughput lever, measured 4 → 22 kB/s from MTU alone); use **notifications**
phone→desktop and **`writeValueWithoutResponse()`** desktop→phone, both
unflagged since Chrome 85. And Android's own documentation warns that when
devices pair over BLE, *"the data that's communicated between the two devices
is accessible to **all** apps on the user's device"* — **link-layer BLE
security is not a trust boundary**, so application-layer AEAD terminated inside
the Rust core is mandatory, not optional. That single decision also makes the
LAN TLS problem disappear, and it is required anyway.

### USB is asymmetric to the point of being unusable

**iOS: there is no path.** `ExternalAccessory` needs an MFi authentication
coprocessor and a desktop cannot be an MFi accessory; `AccessorySetupKit` is
Bluetooth/Wi-Fi pairing UX, not a USB data path; `USBDriverKit` is iPadOS-only,
M1+, entitlement-gated, and makes the iPad the *host* — the wrong direction.
WebUSB cannot see an iPhone, not because of the blocklist (Apple's VID is
absent from it) but because of the documented rule that a device must not
already have a driver claiming the interface — Apple's stack and `usbmuxd` own
the phone. iOS 17's USB-C added no third-party USB API, and the EU DMA decision
forcing Apple to open nine connectivity features includes **zero wired items**.
The one real path is the `usbmuxd`/`iproxy` port relay (Duet Display,
PeerTalk) — which needs a **user-installed native desktop helper**, a trust
prompt, and the app foregrounded throughout.

**Android: possible.** The AOA handshake is vendor control transfers with
`recipient: 'device'`, and WebUSB's protected-class check only fires in
`claimInterface()`. Costs: two chooser prompts (the phone re-enumerates),
`requestDevice()` unavailable in the service worker, and **Windows likely
blocked** because the MTP driver already claims the phone. ⚠️ **Two research
threads disagreed on whether desktop WebUSB can claim an AOA-mode phone** —
one found working host+app examples and the mechanism, the other found no
working demo. **This needs a spike, not more searching**, and it gates the
whole USB option.

**USB tethering is the one useful variant** — it hands the desktop an IP on a
private subnet shared with the phone, turning the cable into an
isolation-proof "LAN" that sidesteps AP client isolation and VPN LAN-blocking.
Cost: the user toggles it manually in Settings every time; there is no app API,
and the TLS problem survives intact.

**Precedent check: no product anywhere has a desktop browser talking to a phone
app over USB.** Android Auto proves the phone-as-AOA-accessory side at scale,
but its host is a native car stack. Ledger and Trezor work because they present
claimable, non-protected interfaces; a phone's are OS-claimed.

## What QR actually costs

Per-frame capacity at QR v40 is **2,953 bytes** in byte mode — which no real
protocol uses, because readers support it inconsistently. Both shipping formats
re-encode into alphanumeric mode: **BC-UR** via Bytewords at 2 chars/byte
(~2,110 B/frame) and **BBQr** via base32 at 5 bits/char (**2,680 B/frame**,
plus optional zlib). BBQr carries ~25% more per frame before compression.

Multi-frame transfer should use **fountain coding** (BC-UR Multipart,
BCR-2024-001). The spec states the problem plainly: with fixed-rate cycling,
*"when a code is missed, the receiver must wait for the entire sequence to
cycle through before getting another chance."* Fountain parts are XOR mixtures
selected by Xoshiro256 with a harmonic degree distribution, so *"any
sufficiently large set of codes can be used to reconstruct the entire
message"* — order-independent, start-anywhere, no stall. A Rust implementation
exists (`bc-ur`), which matters because our core is Rust.

**Measured and shipped throughput:**

| Source | Rate |
| --- | --- |
| Keystone SDK default (400 B/frame, 100 ms) | 4.0 kB/s |
| BBQr / Coldcard Q (v27, 1062 B, 250 ms — *"fine-tuned for this data rate"*) | 4.2 kB/s |
| TXQR measured experiment, peak (11 fps × 850 B) | ~9 kB/s |
| Same author's planning figure | *"in the vast majority of cases you can expect more modest rates – 1-2KB/s"* |

The same experiment found **optimal ~6–7 fps** (above ~11 fps the phone's
refresh rate misaligns and frames drop), **optimal chunk 550–900 B**, with
**1,000 B/frame "almost guaranteed to miss frames and timeout"**, and
recommends **ECC level L** for on-screen frames — redundancy is already
provided by the fountain layer.

**Three constraints follow.**

1. **~10 KB is the comfort ceiling for a signing flow**; 100 KB is 25–100 s;
   300 KB is minutes. And CIP-30's `getUtxos` has *no* specified size bound —
   1,000 ada-only UTxOs is ~208 KB of hex, token-heavy wallets are
   multi-megabyte, and `max_val_size = 5000` means a single legal UTxO can
   exceed 10 KB of hex. **QR cannot be a general connector transport. It can be
   an excellent signing transport.**
2. **Compress before encoding.** The largest documented win in this space is
   AirGap's, whose serializer v3 reports **"an over 80% decrease in size"** for
   batch transactions — and the same release replaced their own ordered-chunk
   multipart QR with BC-UR *specifically because a missed frame forced a full
   re-cycle*. BBQr's zlib gets 23–58%. Payload format matters more than the
   encoding alphabet.
3. **The reverse channel is the weak one.** Forward (phone camera reading a
   laptop screen) is comfortable and *better for us than for any hardware
   wallet*, since a phone camera is the reader. Reverse (laptop webcam reading
   a phone screen) is **estimated** to cap near QR v10–v15, i.e. 150–350
   B/frame — a bare signature is 64–72 bytes so signing survives, a signed blob
   may not. **This estimate is uncited and is the first thing to measure.**

### The extension camera constraint shapes the UX

There is no `camera` extension permission — it is the ordinary WebRTC
permission on the `chrome-extension://<id>` origin. Per Chromium engineers,
`getUserMedia` **fails in offscreen documents, popups, and side panels**;
Chrome's own docs note popup capture *"causes focus issues and closes the
popup."* An offscreen document with reason `USER_MEDIA` can *hold* a stream
indefinitely but cannot *obtain* the grant. **MetaMask's shipping QR scanner
forces the extension out of the popup into a full tab before scanning** — so
the reverse channel costs the user their DApp tab context mid-flow. **One
30-minute experiment worth running:** whether an already-granted origin
permission lets `getUserMedia` succeed in the **side panel**, which survives
tab switches. If it does, that is the best UX available.

### And the QR decoder is the real remote attack surface

A camera turns "no network" into *unauthenticated attacker-controlled binary
input to a hand-rolled parser on a device holding keys*:

- **ZBar CVE-2023-40889 and CVE-2023-40890, both CVSS 9.8** — crafted QR codes
  *"may lead to information disclosure and/or arbitrary code execution."*
  Electrum, a standard air-gapped composer, bundles libzbar.
- **Coldcard, merged days before this study**: malformed BBQr part geometry let
  `final_size` count PSRAM bytes never written — *"PSRAM[0..2MB) is the PSBT
  staging area and is not wiped at boot"* — leaking uninitialised memory out of
  the PSBT buffer; a separate fix rejects oversized multisig imports because
  crafted payloads exhausted the heap.
- **Keystone fails ungracefully at size**: Rust OOM on multipart Cardano URs,
  hang-and-reboot on a complex Cosmos message, and an open report that a
  crafted 6-part UR **bricks the device**. It publishes no payload ceiling.
  Coldcard is the only vendor that publishes real limits (2 MB PSBT, 8,000 B
  NFC) and enforces them — the model to copy.

Also note two exfiltration paths the air gap does *not* close: **Dark Skippy**,
where malicious signer firmware embeds the seed in low-entropy nonces and *"a
single use of a malicious hardware wallet is enough to lose everything"*; and
optical side channels — a signer's screen is a modulatable emitter and the
composer's camera is a receiver (cf. VisiSploit).

**Repo consequence:** the existing `QrScannerPort` is one-shot, 32 KiB-bounded,
60 s deadline, and Android delegates to Play-services Google Code Scanner —
one-shot by design with no programmatic dismiss. **Animated multi-frame QR
requires replacing the Android scanner entirely.**

## Precedent

| Product | Transport | Server? | Native install? |
| --- | --- | --- | --- |
| **Veridian (Cardano Foundation identity wallet)** | CIP-45 WebRTC; **signing only, reads refused**; 1 h approval budget | Trackers/signalling | No |
| **MetaMask ↔ Keystone** | Animated QR both ways, **in a browser extension** | **No** | No |
| **Lace ↔ SeedSigner** (Jul 2026) | QR only — *"no USB… Bluetooth, or Wi-Fi"* | **No** | No |
| AirGap Vault + Wallet | QR (+ same-device deep link); Vault ships **with no network permission** | **No** | No |
| Sparrow / Passport / Coldcard Q | QR / microSD | No | Desktop app |
| Ledger Live web | WebUSB / WebHID to a dedicated device | No | No |
| Trezor Bridge · Ledger Live Bridge | Local daemon | No | **Yes — both retired by their vendors** |
| Solana MWA, local | Android intent → a loopback WebSocket | **No** | No |
| Solana MWA, remote | WSS reflector — spec calls it *"a potential adversary"* | **Yes** | No |
| WalletConnect v2 / Reown | WSS relay | **Yes, mandatory** | No |
| MetaMask Connect | Centrifugo relay (replaced socket.io) | **Yes** | No |
| Coinbase WalletLink | WSS relay | **Yes** | No |
| Phantom deep links | Universal links, x25519 + NaCl box | **No** | No — **same-device only** |
| **FIDO CTAP 2.2 hybrid** | QR + one BLE advert + WSS tunnel | **Yes** | No |

**Nothing ships a browser-extension-to-phone signing bridge.** Extensions
gained WebUSB and WebHID, but those reach *hardware wallets*, not phones. The
APIs that could reach a phone were Chrome-Apps-only and died in 2020.

**Three cautionary details worth carrying:**

- **CIP-45's decentralisation did not survive implementation.** The CIP
  specifies WebTorrent trackers precisely to *remove* the central signalling
  component — but `cardano-peer-connect` v1.2.19 replaced them with **PeerJS,
  defaulting to the public `0.peerjs.com`**, because *"the user experience
  wasn't great and sometimes the connection would fail to establish."* The CIP
  text was never updated. It names **zero implementing wallets**; Eternl
  documents it as *"currently only works with SundaeSwap"*, and the Cardano
  Foundation's own mobile-connect demo self-describes as having *"PeerJS
  connection reliability issues"* and *"transaction signing functionality
  incomplete."*
- **"Open source relay" rarely means self-hostable in production.** Coinbase's
  `walletlinkd` was Apache-2.0 Go and was **removed at SDK v3**. MetaMask's
  relay source is public but under a **non-commercial licence**, and repointing
  requires changing a setting inside the user's installed mobile app.
  WalletConnect's reference relay is **archived** with a README stating
  *"Self-hosting is currently not supported"*; its network is a ~15-node
  permissioned federation whose **gateway tier is single-operator by Reown's
  own documentation**. Solana MWA is the honourable exception — the wallet
  trusts the URI in the QR, so a third party genuinely can run its own
  reflector or Nostr relay. **That is the property to copy if a relay is ever
  added: trust the pairing artefact, never a pinned host.**
- **Relays see more than they claim.** WalletConnect's marketing says the relay
  *"has no insight into users' addresses, transaction hashes… or any other
  information"* — but the shipped SDK spreads undocumented "Transaction
  Validation Framework" fields directly into `irn_publish`, sending the **RPC
  method name, CAIP-2 chainId, the transaction's `to` address, and the
  resulting transaction hash in cleartext**, alongside a persistent
  `client_id`, the dapp hostname in the `ua` parameter, and the dapp origin
  inside the Verify attestation. Reown's own privacy policy concedes the relay
  processes *"Full wallet address."* Payload confidentiality is sound;
  **traffic-analysis resistance is essentially nil.** The strongest available
  argument for the no-relay principle — just not the usual one.

## What the two shipped hardware-wallet bridges teach

Both vendors built a localhost bridge for exactly our reason — a browser that
could not reach the device — and their divergence is the most useful design
material available.

**Origin control: Trezor's design is strictly better, and the difference is
concrete.** `trezord-go` binds `127.0.0.1` explicitly and **returns 403 before
the handler runs** on any origin outside a hard-coded regex allowlist
(`*.trezor.io` HTTPS-only, its onion, `localhost:[58]xxx`, `*.sldev.cz`), with
methods restricted to `POST`/`OPTIONS`. Ledger's bridge, by contrast, checked
the origin **only if the deeplink supplied one** (`if (origin)`), compared a
bare `host`, and — by the `ws` library's default — did not bind to loopback at
all. Ledger compensated with a per-session modal that said out loud *"Opening a
bridge exposes all of your accounts to third party applications."* Consent
instead of an allowlist.

**The reason the allowlist exists is a real, reported vulnerability.** In
February 2018 a DNS-rebinding report against `trezord` showed any website could
reach the daemon: after rebinding, the browser considers the page and the
daemon same-origin, so CORS never engages. The root cause was worse than the
concept — `gorilla/handlers`' CORS middleware **passed through** on a
disallowed origin instead of aborting, so the allowlist blocked nothing. The
fix was the hand-rolled handler that 403s. **Lesson: an origin check that does
not abort is not a check**, and advisory CORS never was one, since it only
stops the attacker *reading* the response while the side-effecting request
still lands. Note the honest limit the maintainers stated themselves: the
`Origin` header is trustworthy only because *browsers* set it, so this defends
against remote web origins and against nothing local.

**The Ledger Connect Kit compromise is the anatomy lesson.** Dapps did not
bundle `connect-kit`; they bundled `connect-kit-loader`, whose entire purpose
was to fetch the library from a CDN at runtime so Ledger could *"improve the
logic and UI without users having to wait for wallet libraries and dApps
updating package versions."* Consequence: **pinning the loader still fetched
the latest `connect-kit`**, so one npm publish propagated to every embedding
dapp within CDN-cache time. The malicious code inherited the dapp's origin —
which already held a persistent WebHID grant, since `navigator.hid.getDevices()`
returns previously-authorised devices **with no prompt** — plus full DOM
control to draw a convincing signing modal. Two hours of live drainer before
the vendor was told, by an ecosystem partner rather than by monitoring.
Post-incident, Ledger's own admission: *"NPMJS.com does not allow
multi-authorization or signature verification for automatic publishing."* And
the public record of the remediation — the repo, the incident issue, and the
strict-versioning PR — **now 404s.**

**Trezor's v9 architecture has the same shape with a different owner**, and one
structural difference worth knowing: it injects an iframe from
`connect.trezor.io/9/`, deployed by overwriting an S3 prefix, and **`integrity`
does not apply to iframes** — so the pin-plus-SRI mitigation that would have
contained the Ledger incident is *structurally unavailable* there. Self-hosting
is blocked on both sides by the same allowlist, which Trezor stated as policy:
*"Since you do not trust us, there is no reason to trust you and whitelisting
your url in our bridge."*

**Connect 10 is the interesting pivot**: the iframe is gone, the core ships
inside signed Suite desktop, and authorisation moves to a **consent prompt with
OS process attribution** (`findProcessFromIncomingPort`) instead of a domain
allowlist. That is a better trust story for code delivery and it unblocks third
parties — at the cost that the prompt is now the entire security boundary, the
process check is heuristic and skipped on Linux, and the method-call path still
carries a `// todo: this is incomplete validation`.

**Three transferable conclusions:**

1. **Never let display-layer metadata be caller-supplied.** Ledger's device
   genuinely binds screen to signature — Generic Clear Signing recomputes and
   *"check[s] computed fields hash"* before rendering, and EIP-712 filters are
   signed with per-type magic-byte domain separation over
   `chainID || contract || schema hash`. But the descriptors that make a screen
   legible are signed by **Ledger's own PKI**, so the vendor is a trusted
   authority for what the user reads even though it is not one for the key —
   and with no descriptor the device degrades to blind signing. Our equivalent
   defect is live: `SensitiveOperationConfirmation` takes `title`/`summary` from
   the caller.
2. **Trezor's third reason for keeping a daemon is the one nobody cites, and it
   applies to us**: *"WebUSB does not allow synchronization of USB access
   between domains."* Two tabs cannot arbitrate device access. The daemon's
   `/acquire` session with a compare-and-swap `PREVIOUS` is a **correctness**
   mechanism, not just a compatibility shim — and our single-active-draft rule
   is the same idea, which is worth stating explicitly in any bridge protocol.
3. **The localhost escape hatch is closing from both ends.** Firefox and Safari
   have *published formal opposition* to WebUSB, WebHID and Web Bluetooth —
   Mozilla `negative`, WebKit `oppose` — so those are Chromium-only forever;
   meanwhile Chrome and Firefox are gating localhost. Ledger deleted its legacy
   web transports from the monorepo in June 2026 and its current SDK ships **no
   WebUSB transport at all**; Trezor's standalone daemon is deprecated in favour
   of one bundled in Suite desktop, on a **different port** (21325 → 21328,
   with Connect 10 adding 21335). Any design pinned to a specific local port or
   a specific browser device API is building on sand.

### The LAN-TLS wall, and why the clever workaround is worse than a relay

The root cause is a rule, not a browser bug. CA/Browser Forum Baseline
Requirements §4.2.2: *"CAs SHALL NOT issue Certificates containing Internal
Names or Reserved IP Addresses."* **No publicly-trusted certificate can ever
name `192.168.x.y`, `10.x`, `127.0.0.1`, or a `.local` host.** Loopback is
exempt from the problem only because loopback origins are already potentially
trustworthy, which is exactly why every shipped wallet bridge uses loopback and
none uses the LAN.

The famous workaround — Plex's `*.HASH.plex.direct`, with a per-server wildcard
certificate and hostnames that encode the IP (`1-2-3-4.<hash>.plex.direct`), or
Tailscale's DNS-01-issued `*.ts.net` — does work. **But it is more
infrastructure than a relay, not less**: an authoritative DNS zone operated
forever, a per-device certificate issuance service, private keys shipped to
users' machines, and a log of which LAN addresses users' devices occupy. It
moves the trust from *"an operator who sees ciphertext"* to *"an operator who
controls naming and can be compelled"* — a worse trade under our own stated
principle. It also breaks in the field: Plex documents router **DNS-rebinding
protection** blocking its own product, which is the mitigation for the very
attack class described above.

**The crispest negative result in this space:** `lndconnect` is the best real
LAN-pairing precedent — a QR carrying host, port, the node's **TLS certificate**
and a macaroon — and it works only for **native apps**, because a browser cannot
be told to trust a self-signed certificate handed to it in a QR code. That one
sentence is why "pair over LAN and talk from the page" has no shipped example.

### Local Network Access has already landed, and it is still moving

| Milestone | Change |
| --- | --- |
| Chrome 142 (Oct 2025) | LNA ships desktop + Android; supersedes Private Network Access |
| Chrome 145 | Split into `local-network` and `loopback-network` permissions |
| **Chrome 147 (2026-04-07)** | **Extended to WebSocket and WebTransport** |
| Chrome 156 | The temporary enterprise opt-out is **removed** |
| **Firefox 151+** | Also shipping, **enabled by default** |
| Safari | No signal — and Safari never had any other local path |
| **WebRTC** | **Still un-gated**, `Proposed` with no milestone. Do not build on it. |

The motivation is documented abuse, not theory: **Meta and Yandex used
localhost as a covert web↔app bridge on Android at billions-of-users scale** —
Meta via SDP munging to STUN on UDP 12580–12585, Yandex via plain HTTP to ports
29009/29010/30102/30103 — and it **worked in Incognito, logged out, and after
clearing cookies.** Chrome 137 blocked the ports; LNA is the general fix. Note
which path actually scaled: **WebRTC, the one hole still open.** That is
precisely the gap a LAN or WebRTC design would be relying on, and it will
close.

### And a web page structurally cannot replicate caBLE

Worth stating because it closes the door on the most attractive-looking option:
the FIDO hybrid BLE advertisement service UUID `0000fff9` **is on the Web
Bluetooth GATT blocklist**, with the stated reason that *"a website could use
raw GATT commands to impersonate another website to the FIDO device."* Hybrid
transport is implemented **inside the browser/OS**; the only surface exposed to
a page is `navigator.credentials.get()`. So the one shipped, well-engineered
phone-signs-for-desktop-browser mechanism is not available as a building block
— and its designers, who could have used BLE for the data path, deliberately
used it for a 20-byte proximity beacon and put the payload on a cloud tunnel.

**One more transport neither the brief nor my first pass considered.** NIP-55's
web mode is a genuinely zero-infrastructure browser→signer channel built purely
from URL-scheme handoff: `nostrsigner:<payload>?callbackUrl=…`, answer returned
by redirect (or, failing that, the clipboard). No relay, no port, no CORS, no
LNA, no Bluetooth. Its costs are the ones the spec admits: **same-device only**,
a visible round trip per request, no background signing, and URL length limits.
**CIP-186 is precisely this pattern for Cardano** — merged August 2026, zero
implementations. It does not solve desktop→phone, but it is the right answer for
phone-browser→phone-wallet, and it belongs in the transport menu rather than
being rediscovered later.

**Finally, the extension's one genuine platform advantage:** native messaging
is **portless, CORS-free, and immune to LNA** — stdio to a local binary with an
`allowed_origins` allowlist that permits no wildcards. That is a real
capability no page has. It still costs a desktop installer, and its 1 MB
host→extension cap is two orders of magnitude below an 80 MB prover key.

## Latency is not the problem. Round-trip count and DApp-side deadlines are.

**Nothing in these APIs is synchronous.** Every method on CIP-30, CIP-95 and
the Midnight v4 connector returns a `Promise`. A phone round trip — even one
with a human in it — does not violate the API contract. **And the industry has
already proven the pattern**: Ledger and Trezor device protocols contain **no
query methods at all** — not `getUtxos`, not `getBalance`, not `getCollateral`
— while Lace's own `cip30.ts` serves `getBalance`, `getUtxos` and
`getCollateral` from local observables behind `waitForWalletStateSettle` with a
**120 s** sync ceiling, touching the device only inside `signTransaction`.
CIP-30 makes the split normative: *"All read-only methods… should not require
any user interaction… The remaining methods `api.signTx()` and `api.signData()`
must request the user's consent… for each and every API call."*

**So the design is: reads never touch the phone.** Take an account xpub once at
pairing, derive addresses locally, sync from an indexer, cross the wire only
for signing. **CIP-104 (account public key) exists precisely to bless this**,
and **CIP-103 (bulk transaction signing) collapses N human round-trips into
one** — those two extensions are what make a high-latency signer viable, and
both are already in the extensions register.

**The real hazards are elsewhere, and they are measurable:**

| Constraint | Value | Source |
| --- | --- | --- |
| **Midnight example DApps' wallet-detection deadline** | **1,000 ms** (bboard), 3,000 ms (leaderboard) | `timeout({ first: 1_000 })` |
| **…and their connect deadline** | **5,000 ms**, wrapping *two* wallet round-trips | bboard `connect()` + `getConnectionStatus()` |
| Midnight's *own* wallet dapp detection budget | 500 ms × 40 = **20 s** | `useWalletDetection.ts` |
| `cardano-connect-with-wallet` injection wait | 20 × 25 ms = **500 ms**, then "not installed" | `core/index.ts` |
| …and CIP-30 calls fired sequentially on connect | **5** (`getExtensions`, `getRewardAddresses`, `getUsedAddresses`, `getUnusedAddresses`, `getBalance`) | same |
| **Lace Mobile's per-CIP-30-call timeout** (WebView bridge — the closest shipping analogue) | **60,000 ms**, every method including `enable` and `signTx` | `cip30-injection-script.ts` |
| CIP-45 `PeerRpc` per-call timeout | **30,000 ms** | `PeerRpc.ts` |
| Veridian's human approval budget | **3,600,000 ms** | `identityWalletConnect.ts` |
| Lace extension / cardano-js-sdk | **no request timeout on the happy path** | `remoteApi.ts` |
| `@meshsdk/wallet` | **no timeouts, no polling, one synchronous probe of `window.cardano`** | grep: zero matches |
| lucid-evolution per `complete()→sign→submit` | **4–5 sequential CIP-30 calls**; no UTxO cache | `CompleteTxBuilder.ts` |
| mesh per transaction build | can call `getUtxos()` **twice** | `transaction/index.ts` |
| Midnight `/prove` per-attempt timeout | **300,000 ms**, ×3 retries with backoff | `DEFAULT_TIMEOUT` |
| Midnight `watchForTxData` / `submitTx` | **indefinite by design** — *"Do not implement timeouts in this method."* | `public-data-provider.ts` |

**The decisive practical finding: any DApp built from Midnight's own example
templates will reject a phone-hosted wallet inside 1–5 seconds, regardless of
which transport we pick.** Transport latency is subordinate to that. Two
consequences: if Oxid ships a DApp SDK its detection budget should be **≥20 s**
(matching Midnight's own wallet dapp) and its per-call budget **≥60 s**
(matching Lace Mobile) — and **published**, because the 30 s CIP-45 precedent
and the 60 s Lace Mobile precedent already disagree. And because **nobody
caches**, round-trip count matters more than per-call latency: at 2 s/call,
`cardano-connect-with-wallet` alone spends 10 s just connecting.

**One hard synchronous requirement remains.** DApps enumerate `window.cardano`
/ `window.midnight` and read `name`, `icon`, `apiVersion` (`rdns` for Midnight)
**synchronously at page load, with no polling and no retry** — verified in the
shipped `@meshsdk/wallet` bundle, which iterates keys and skips any entry whose
metadata is `undefined`. **If you have not injected by the time the DApp looks,
you do not exist.** Serve those fields from local storage at `document_start`,
never from the phone.

**Two Midnight-specific notes.** The connector is **one major version ahead of
the names in the brief** — `enable()`, `state()` and
`balanceAndProveTransaction` were **removed in v4.0.0**; mainnet runs
**4.0.1**. And **`hintUsage` is the most useful method in either API for this
architecture**: the spec says a wallet *"can use these calls as an opportunity
to ask user for permissions"*, making it a sanctioned hook for batching every
phone prompt into one up-front round trip. CIP-30 has no equivalent —
`enable()` is the only sanctioned prompt, so it has to count. Neither ecosystem
can **push** state changes to a DApp (CIP-30 has only the passive
`AccountChange: -4`; Midnight lists events as future direction), so an account
switch on the phone has no in-band route to the DApp. Stale-cache failures are
real ledger errors (`BadInputsUTxO`, `ValueNotConservedUTxO`,
`OutsideValidityIntervalUTxO`), and a human prompt can sit for a minute — so
build TTLs with slack and **re-validate after approval returns**.

## Threat model

The security case rests on one property: **the phone must be authoritative
about what it signs.** The extension is untrusted transport.

| Threat | Mitigation |
| --- | --- |
| Malicious DApp | The phone renders authoritative content **derived from the material it holds**. Bound every field. Reuse the strict-router pattern from `identity-ingress`: scheme allowlist, pre-registered requests only, unknown → fail closed. |
| Malicious page in another tab | Content scripts run with the *page's* origin. Per-origin session grants bound at `enable()`, origin stamped by the extension and **displayed on the phone**, never page-supplied. Never route transport through a content script. |
| Extension-store supply chain | The residual risk, and it **cannot be engineered away inside the extension** — updates install silently with no prompt unless permissions change, and the transport permissions are already granted. The mitigation lives on the phone: never trust a single extension-supplied field. Add reproducible builds, published artifact digests, a version the phone can check, and treat store API keys as tier-0 secrets. |
| **"Extension shows one thing, phone signs another"** | **The sharpest finding, and a live defect in the current design.** `SensitiveOperationConfirmation` is `{ title, summary, confirmed }` — verified at `crates/wallet/application/src/security.rs:354` — and both strings come **from the calling adapter**. A malicious bridge would supply both. For any bridge-originated request the phone must **derive** confirmation text from the authoritative payload and refuse caller-supplied display strings. This is an ADR-level change to the application boundary, not a UI tweak. |
| MITM on the local transport | Application-layer AEAD terminated **inside the Rust core**, keyed out-of-band. Bonded BLE data is readable by all apps on the device; a plaintext (non-TLS) WebSocket LAN channel carrying witness material would violate ADR-0027's non-loopback rule. Never rely on link-layer security. |
| Pairing / impersonation | Key exchange from a QR the **phone** displays; per-session keys by ECDH. A QR-bootstrapped channel is MITM-resistant because the pairing secret never crosses the network. Solana MWA's construction is worth copying directly: the association public key is the **HKDF salt**, so a relay that never sees the association private key cannot forge the handshake. If BLE is used at all, adopt FIDO's insight and make it the **proximity proof**, not the pipe — a QR alone does not prove co-location, since a phishing page can display someone else's. |
| Replay | Bind each request to a session nonce **and a digest of the preview the phone rendered**, single-use, under the existing single-active-draft rule. The current `authorization_challenge` is an opaque draft handle, not a commitment to displayed content. |
| Capability-manifest over-trust | ADR-0099 already warns the manifest *"becomes a load-bearing security boundary for a second consumer, which issue #69 shows it is not yet fully ready for."* A DApp bridge is a **third** consumer and the first exposed to hostile web content. Keep `oxid-mcp`'s belt-and-braces pattern: manifest flags **plus** an independent authority-verb denylist. The manifest is `composition_time` static, so it cannot express per-origin grants — that is genuinely new state. |
| QR decoder RCE | See the CVE record above. Bound sizes, fuzz the parser, keep it on-device. |
| Dapp identity | **Nobody has solved this.** Solana MWA specifies an elaborate origin-attestation flow — and it is error-code stubs and a diagram: the reference wallet's `ClientTrustUseCase` carries `// TODO: kick off web-based client verification here`, fakes a 1.5 s delay, and returns `VerificationSucceeded`; remote associations are `NotVerifiable` by design. CIP-30 `metadata` is attacker-controlled and Verify-style services are advisory. Do not assume this is a solved problem you can adopt. |

## The blocking decision is not transport

Oxid's protocol is **intent-based** (`recipient`, `asset`, `amount`); DApp
connectors are **byte-based**. There is no method that accepts a DApp-built
transaction, `serializedTransactionExposed: false` is asserted everywhere, and
`wallet.key.sign` is `development_only`, `confirmationRequired`, 64 KiB-bounded.

**Serving `signTx` or `balanceSealedTransaction` means deliberately flipping a
secret-hygiene invariant.** That is the biggest single decision here, it is
**independent of transport**, and it is cheap to answer. **If the answer is no,
there is no DApp connector and no reason to build the extension at all.**

Veridian's precedent suggests a third option worth putting on the table:
**serve signing only and refuse the read surface outright**, which is exactly
what `IdentityWalletConnect` does. For an identity-first wallet that may be the
right answer rather than a compromise.

## Reuse vs new protocol

**Reusable essentially unchanged:** the NDJSON envelope with
`deny_unknown_fields` and stable error codes; `system.capabilities` as the
front door with `oxid-mcp`'s exact filter policy; the prepare → authorize →
submit staging with its single-active-draft rule and persist-before-broadcast
journal; `WalletTransferPreviewView` as the phone's render source; the truthful
`cached` source label, which is already what a read cache needs to report; the
strict ingress router; and the `prepare → get → accept/refuse` ceremony shape
that `credential.*` and `identity.*` already use — that is the right shape for
remote approval.

**Needs new protocol, each an ADR:** a byte-oriented signing surface (above);
per-origin session state, since the manifest is static; phone-derived
confirmation text; a pairing and channel-security protocol; a transport framing
layer with chunking, backpressure, resume and MTU-degradation handling; and a
multi-frame QR port to replace the one-shot one.

**One repo constraint to remember:** *"keeping one plugin package is required
by Dioxus 0.7.10, whose iOS bundler compiles multiple Swift packages but embeds
only its primary framework"* — all native transport code lands in one plugin.

## Recommendation

**Transport ranking:**

1. **QR, fountain-coded** — the only zero-install, zero-server, both-platform
   option, with a shipping browser precedent, a shipping *Midnight-ecosystem*
   precedent, a Rust library, and **no browser API that Chrome can take
   away**. Limit it to signing ceremonies and small payloads.
2. **WebUSB + Android AOA** — the only transport with extension-service-worker
   support, real persistent permission, and one gesture ever. Android-only,
   Windows-doubtful, gated on the spike above.
3. **BLE** — best bandwidth-per-effort on Android and the cheapest permissions
   there, and 100 KB in seconds. But iOS background discovery is impossible,
   `getDevices()` is flag-gated so users re-pick the phone every session, Linux
   is excluded by default, and both other browser engines oppose the standard.
   **Prototype before believing.** Note the foreground-only constraint is
   *acceptable* for a signing device — the user is holding the phone anyway —
   which turns a platform limitation into a stated design property rather than
   something to fight.
4. **LAN WebSocket, QR-paired** — highest bandwidth, but the TLS story has no
   serverless answer, iOS cannot keep a listener alive backgrounded, AP client
   isolation and VPNs break it in the real world, and it rides the undocumented
   LNA exemption.
5. **Native messaging** — removes every browser restriction, costs a desktop
   installer, and both shipped precedents were retired.

**Should the extension mirror the mobile UI?** **No.** It should be transport
plus an approval-status surface: connection state, "waiting for your phone", a
read-only echo of what the phone is rendering, and a clear statement that the
phone is authoritative. A richer mirror creates a second display that a
compromised update can lie with. Host it in the **side panel** — all APIs,
survives tab switches, can produce a user gesture — never the popup, which dies
on focus loss and takes the connection with it.

**Phased plan, each phase shippable and provable:**

- **Phase 1 — the QR approval channel, no extension at all.** A multi-frame
  BC-UR-style QR port in and out, plus a bridge-request ceremony reusing the
  `prepare → get → accept/refuse` shape and the strict-router ingress pattern.
  Invert `SensitiveOperationConfirmation` so the phone derives its own display
  text. Prove it against `oxid-headless` with no browser involved. **This ships
  value immediately, is provable with existing gates, and is the out-of-band
  approval channel ADR-0099 already defers to issue #70 — one primitive serving
  both AI agents and DApps.**
- **Phase 2 — settle the byte-signing ADR** before any extension code exists,
  with "signing only, reads refused" as a live option.
- **Phase 3 — a thin extension.** Inject the synchronous metadata at
  `document_start` from local storage; serve reads from a locally derived view
  (xpub taken once at pairing, per CIP-104); route only signing to the phone,
  initially over the Phase-1 channel in the side panel; use `hintUsage` and
  CIP-103 bulk signing to batch prompts. Publish the timeout budgets. Ship
  reproducible builds and digests from day one.
- **Phase 4 — a faster transport** behind the same approval protocol, keeping
  **QR as the permanent fallback**.

**Do not put proving on the phone.** It is *"infeasible in WASM on mobile
browsers"*, prover keys are 10–80 MB against a 1 MB native-messaging cap, and
shielded spends are proof-authorized rather than signature-authorized. Proving
belongs on a server the *user* controls — and MPS-0004's TEE-attested delegated
proving is the specification to track, with a working implementation already
public.

**One reframe worth putting to the owner:** the extension's genuine product
value is **distribution** — appearing in wallet pickers on DApps that do not
include your library. It is not a security win over a well-built page beyond
the custody dimension, and **the custody win is available from the QR channel
without an extension at all.** Which of those two things is being bought is
worth deciding explicitly.

## The standards opening

Two adjacent gaps, both live as of this week:

- **Cardano.** CPS-0010 "Wallet Connectors" is **Open** with
  `Proposed Solutions: []`, and states our problem for us: *"mobile wallets are
  thus required to reimplement web browsers in their applications, which is
  wasted effort."* The transport-agnostic successor — **CIP-144 "Extensible
  Wallet Connector Framework"**, which wanted *"an API without committing to a
  specific transport layer"* — was declared abandoned by its author **on
  2026-08-17**: *"I am not currently working in the Cardano ecosystem anymore."*
  Editors are deciding whether to merge or close within the month. A documented
  cached-reads / remote-sign split is a credible contribution, and **CIP-186
  "Cardano Wallet Deep-Link Signing"** (merged Aug 2026, Proposed, `Solution
  To: CPS-0010`) has **zero shipped implementations** — it frames CIP-45 and
  itself as *"complementary entries in a wallet's transport menu"*, which is
  precisely the menu Eternl already ships **seven** transports into, all under
  one CIP-30 API.
- **Midnight.** No mobile or remote connector proposal exists at all — the
  connector installs into `window.midnight` with `configurable: false`, and its
  "future direction" section never mentions a non-browser transport. `midnight`
  CAIP-2 registration is still Draft with the WalletConnect layer explicitly
  out of scope, and no ChainAgnostic namespace PR has been filed. Genuine
  greenfield.

## Verify before building

Every item is cheap, and each is load-bearing:

1. **Reverse-channel QR density** — laptop webcam reading a phone screen. The
   v10–v15 estimate is uncited.
2. **`getUserMedia` in the side panel** with an already-granted origin.
3. **Web Bluetooth from an MV3 extension page** — undocumented; does the
   chooser plus GATT work, and does the connection survive?
4. **Whether desktop WebUSB can claim an AOA-mode phone** — sources disagreed;
   gates option 2.
5. **`device.open()` on Windows** against a phone in MTP mode.
6. **Whether the LNA extension exemption holds** on current Chrome, Edge and
   Brave, and whether `http://192.168.*/*` is even a legal match pattern.
7. **Whether a phone-side listener survives App Review** — no precedent found
   either way.
8. **Whether an unbonded Chrome↔iOS-peripheral connection survives RPA
   rotation.**

## Store constraints that apply regardless of transport

- **Apple 3.1.5(i): wallet apps must come from a Developer Program account
  enrolled as an organization.** Non-negotiable, weeks of lead time — start it
  independently of this decision.
- Apple 4.2.3(i) means the wallet must stay standalone-useful with bridging as
  one feature; declare the bridge in the listing (2.5.1). Declaring
  `bluetooth-peripheral` for genuine BLE is intended use and low risk; using
  `voip`/`audio`/`location` to keep a listener alive is a documented violation
  — and backgrounded BLE would not work anyway.
- **Google Play explicitly exempts non-custodial wallets** from its
  cryptocurrency licensing regime — state on-device-only custody unambiguously,
  and note that cloud key backup or server-side MPC shares could pull us back
  in scope.
- Use foreground-service type **`connectedDevice`**, not `dataSync` (Android 15
  caps `dataSync` at 6 h per 24 h with a fatal exception; `connectedDevice` has
  no timeout). Play requires a **demo video** per declared type. Neither
  Bluetooth permissions nor `NEARBY_WIFI_DEVICES` are declaration-gated; set
  `usesPermissionFlags="neverForLocation"`.
