// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

/// Immutable build-selected presentation metadata supplied by a thin app crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrandProfile {
    product_name: &'static str,
    wordmark: &'static str,
    tagline: &'static str,
    bundle_identifier: &'static str,
    publisher: &'static str,
    show_vault_card: bool,
    style_sheet: &'static str,
    logo_svg: &'static str,
}

impl BrandProfile {
    /// Constructs a profile only from build-generated, schema-validated values.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        product_name: &'static str,
        wordmark: &'static str,
        tagline: &'static str,
        bundle_identifier: &'static str,
        publisher: &'static str,
        show_vault_card: bool,
        style_sheet: &'static str,
        logo_svg: &'static str,
    ) -> Self {
        Self {
            product_name,
            wordmark,
            tagline,
            bundle_identifier,
            publisher,
            show_vault_card,
            style_sheet,
            logo_svg,
        }
    }

    pub const fn product_name(self) -> &'static str {
        self.product_name
    }

    pub const fn wordmark(self) -> &'static str {
        self.wordmark
    }

    pub const fn tagline(self) -> &'static str {
        self.tagline
    }

    pub const fn bundle_identifier(self) -> &'static str {
        self.bundle_identifier
    }

    pub const fn publisher(self) -> &'static str {
        self.publisher
    }

    pub const fn show_vault_card(self) -> bool {
        self.show_vault_card
    }

    pub const fn style_sheet(self) -> &'static str {
        self.style_sheet
    }

    pub const fn logo_svg(self) -> &'static str {
        self.logo_svg
    }

    pub fn security_copy(self) -> SecurityCopySnapshot {
        security_copy_snapshot(self.product_name)
    }
}

/// Code-owned safety and consent sentences. Brands may substitute only the
/// validated product name in the explicitly marked slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityCopySnapshot {
    pub complete_recovery_warning: String,
    pub complete_recovery_confirmation: String,
    pub submission_ambiguity_warning: String,
    pub backup_receipt_failure: String,
    pub presentation_consent: &'static str,
    pub vault_broadcast_warning: &'static str,
}

pub fn security_copy_snapshot(product_name: &str) -> SecurityCopySnapshot {
    SecurityCopySnapshot {
        complete_recovery_warning: format!(
            "{product_name} never merges this archive into existing local wallet state. Chain-derived caches and transaction history rebuild from their authoritative sources."
        ),
        complete_recovery_confirmation: format!(
            "I confirm complete recovery into this empty {product_name} installation."
        ),
        submission_ambiguity_warning: format!(
            "This may have reached the network. {product_name} will check before anything is sent again."
        ),
        backup_receipt_failure: format!(
            "Backup document was saved, but {product_name} could not record its completion status."
        ),
        presentation_consent: "I consent to use the selected credential and disclose exactly these claims to this verifier.",
        vault_broadcast_warning: "Cancellation is safe only before the broadcast boundary. The wallet never blind-retries an ambiguous outcome.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_BRAND: BrandProfile = BrandProfile::new(
        "Example Wallet",
        "example",
        "Identity wallet",
        "io.example.wallet",
        "Example Publisher",
        true,
        ":root {}",
        "<svg viewBox=\"0 0 1 1\"></svg>",
    );

    #[test]
    fn profile_exposes_only_build_selected_presentation_values() {
        assert_eq!(TEST_BRAND.product_name(), "Example Wallet");
        assert_eq!(TEST_BRAND.wordmark(), "example");
        assert_eq!(TEST_BRAND.tagline(), "Identity wallet");
        assert_eq!(TEST_BRAND.bundle_identifier(), "io.example.wallet");
        assert_eq!(TEST_BRAND.publisher(), "Example Publisher");
        assert!(TEST_BRAND.show_vault_card());
        assert_eq!(TEST_BRAND.style_sheet(), ":root {}");
        assert!(TEST_BRAND.logo_svg().starts_with("<svg"));
    }

    #[test]
    fn security_copy_changes_only_the_product_name_slot() {
        let example = TEST_BRAND.security_copy();
        let another = security_copy_snapshot("Another Wallet");

        assert_eq!(example.presentation_consent, another.presentation_consent);
        assert_eq!(
            example.vault_broadcast_warning,
            another.vault_broadcast_warning
        );
        assert_eq!(
            example
                .complete_recovery_warning
                .replace("Example Wallet", "Another Wallet"),
            another.complete_recovery_warning
        );
        assert_eq!(
            example
                .submission_ambiguity_warning
                .replace("Example Wallet", "Another Wallet"),
            another.submission_ambiguity_warning
        );
        assert_eq!(
            example
                .backup_receipt_failure
                .replace("Example Wallet", "Another Wallet"),
            another.backup_receipt_failure
        );
    }
}
