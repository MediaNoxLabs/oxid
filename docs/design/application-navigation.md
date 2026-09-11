# Responsive application navigation

Oxid owns application navigation, not operating-system navigation. Detail routes use one leading, icon-only application Back action and retain profile and overflow controls on the trailing edge. Every supported input delegates to the same route-stack pop action: Android system or gesture Back, Apple navigation behavior, and desktop mouse or keyboard Back where the host exposes it. Oxid never renders Android Back/Home/Overview controls in application content.

The header is a three-column grid (`leading`, truncatable title, `trailing`) so controls reserve their own space and cannot overlap the route title. The title uses deterministic ellipsis while its accessible name remains the full route title. The Back control has one accessible name, `Go back`; its 3rem minimum target meets Android's 48dp minimum and exceeds Apple’s 44pt minimum. Logical grid placement and RTL icon mirroring preserve the same contract for left-to-right and right-to-left layouts. Shell padding continues to respect safe-area/window insets.

Owner-invoked desktop rendering evidence covers 390, 430, and 768 CSS-pixel widths, the Proof benchmark title, text scaling, and both layout directions. This contract deliberately excludes benchmark content density and diagnostics hierarchy.
