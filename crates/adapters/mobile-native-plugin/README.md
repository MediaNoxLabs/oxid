# Oxid mobile native plugin

This repository-owned Manganis plugin is the single native bridge shared by
Oxid's driven mobile adapters. It owns only OS integration: QR camera capture,
custom-scheme delivery, typed public receive-address copy/share actions, device
custody/document pickers, and a boolean screen-snapshot privacy operation.
Protocol classification, presentation masking, consent, and wallet behavior
remain in Rust ports and application services.

Keeping one plugin package is required by Dioxus 0.7.10, whose iOS bundler
compiles multiple Swift packages but embeds only its primary framework.
