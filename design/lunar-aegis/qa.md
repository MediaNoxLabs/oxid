# Design export QA and unresolved decisions

The files in this profile are a complete **source export** of the active
linked screens. They are not a claim that every UX Pilot preview is ready
for direct implementation.

## Findings from the exported screens

| Priority | Evidence | Required resolution |
| --- | --- | --- |
| High | [Presentation Consent](screens/png/FLVs6VUqSwlb9sGswduG.png) has overlapping “illustrative request / connection unverified” metadata and a prominent Share Data action on an unverified example. | Reflow at 375 px and disable authorization until requester, claim set, source, and consent binding are valid. |
| High | [Receive](screens/png/WuqRtdvZAmT09UJtiVO0.png) places a proposed QR illustration below “Address unavailable.” | Keep any proposed QR clearly separate from the unavailable state; never render a scannable/copied address without a controlled address. |
| Medium | [Home](screens/png/kI1o6I63AKA1Z0AIAwHI.png) uses small header controls and tiny metadata despite the 44 px target and large-text goal. | Check touch hitboxes and dynamic type on device; adjust before snapshot acceptance. |
| Medium | [Scan entry](screens/png/Yv3m128eEWimsftexS7J.png) says “Identity Scanner” although the shared dispatcher is intended for payments, credentials, and login requests. | Use task-neutral naming and classify payloads before review. |
| Medium | [Wallet](screens/png/dnug5nrBQ8Edsf30jegU.png) heads the screen “Assets” while the shared tab label is “Wallet.” | Choose one user-facing noun in shell and page title. |
| Medium | [Send review](screens/png/1BVzLKBz9gbMrTtlQuhH.png) shows illustrative amount, fee, recipient, and shielded mode. | Bind every value to a real review object; preserve pending/unknown outcomes. |

## Design-system and asset limitations

- UX Pilot's published V1 has 20 components, but its dark color-token table
  shows blank values for several card/primary/accent entries. The normalized
  approved palette is in [profile.json](profile.json); remaining brand-pack
  fields are deliberately unresolved.
- The UX Pilot MCP currently lists the React design-system collection but
  returns no component bodies, styles, or documentation. The local
  [component contracts](components.md), exported screens, assets, and graph
  are the account-independent handoff.
- Some published gallery fixtures explicitly mount fake data: a `0x`
  deposit address with active Copy, Confirm Transfer, and unbound age-proof
  language. Corrected component defaults do not make those fixtures safe to
  copy. A partial documentation cleanup exists as an unpublished UX Pilot
  draft; the published gallery remains a known issue.
- [logo-concept.jpg](assets/logo-concept.jpg) is a visual reference with
  “0xID” lettering. [logo-master.svg](assets/logo-master.svg) differs in
  geometry and detail. Review the final wordmark and 16–120 px app icon
  before replacing shipped assets.
- The screen HTML imports remote Tailwind, Font Awesome, Google Fonts, and
  a UX Pilot-hosted logo. The local PNGs and bundled assets are the offline
  reference. Generated HTML interaction code does not define wallet
  capability or authorization behavior.
