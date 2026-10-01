// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;

pub(super) const EMPTY_STATE_PRIMITIVE: &str = "EmptyState";
pub(super) const REVIEW_SHEET_PRIMITIVE: &str = "Sheet";

#[component]
pub(super) fn IdentityEmptyState(
    title: String,
    description: String,
    scope: String,
    class: String,
) -> Element {
    rsx! {
        article { class: "empty-state surface-card {class}",
            "data-ui-primitive": EMPTY_STATE_PRIMITIVE,
            span { class: "empty-state__mark", aria_hidden: "true", "◇" }
            h2 { "{title}" }
            p { "{description}" }
            span { class: "status-pill", "{scope}" }
        }
    }
}

pub(super) const fn review_surface_class(terminal: bool) -> &'static str {
    if terminal {
        "credential-issued-receipt"
    } else {
        "credential-offer-preview"
    }
}

#[component]
pub(super) fn IdentityReviewSheet(
    test_id: String,
    review_state: String,
    terminal: bool,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: review_surface_class(terminal),
            "data-testid": "{test_id}",
            "data-review-state": "{review_state}",
            "data-ui-primitive": REVIEW_SHEET_PRIMITIVE,
            {children}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::dioxus_core::Mutation;

    fn empty_state_harness() -> Element {
        rsx! {
            IdentityEmptyState {
                title: "No identities yet".to_owned(),
                description: "Create or resolve an identity.".to_owned(),
                scope: "Profile scoped".to_owned(),
                class: "did-empty-state".to_owned(),
            }
        }
    }

    fn rendered_text(root: fn() -> Element) -> Vec<String> {
        let mut dom = VirtualDom::new(root);
        dom.rebuild_to_vec()
            .edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::CreateTextNode { value, .. } => Some(value.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn shared_review_surface_preserves_open_and_terminal_structure() {
        assert_eq!(review_surface_class(false), "credential-offer-preview");
        assert_eq!(review_surface_class(true), "credential-issued-receipt");
        assert_eq!(REVIEW_SHEET_PRIMITIVE, "Sheet");
    }

    #[test]
    fn shared_empty_state_owns_the_semantic_primitive() {
        assert_eq!(EMPTY_STATE_PRIMITIVE, "EmptyState");
        let rendered = rendered_text(empty_state_harness);
        assert_eq!(rendered.len(), 3);
        assert!(rendered.iter().any(|text| text == "No identities yet"));
        assert!(
            rendered
                .iter()
                .any(|text| text == "Create or resolve an identity.")
        );
        assert!(rendered.iter().any(|text| text == "Profile scoped"));
    }
}
