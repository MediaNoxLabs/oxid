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

pub(super) const fn review_surface_class() -> &'static str {
    "credential-review-surface"
}

#[component]
pub(super) fn IdentityReviewSheet(
    test_id: String,
    review_state: String,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: review_surface_class(),
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
    use dioxus::dioxus_core::{AttributeValue, Mutation};

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

    fn terminal_review_harness() -> Element {
        rsx! {
            IdentityReviewSheet {
                test_id: "identity-review".to_owned(),
                review_state: "refused".to_owned(),
                p { "Request refused" }
            }
        }
    }

    fn rendered_attributes(root: fn() -> Element) -> Vec<(String, String)> {
        let mut dom = VirtualDom::new(root);
        dom.rebuild_to_vec()
            .edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::SetAttribute {
                    name,
                    value: AttributeValue::Text(value),
                    ..
                } => Some((name.to_string(), value.clone())),
                _ => None,
            })
            .collect()
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
    fn shared_review_surface_uses_outcome_neutral_structure() {
        assert_eq!(review_surface_class(), "credential-review-surface");
        assert_eq!(REVIEW_SHEET_PRIMITIVE, "Sheet");

        let attributes = rendered_attributes(terminal_review_harness);
        assert!(attributes.iter().any(|(name, value)| {
            name == "data-ui-primitive" && value == REVIEW_SHEET_PRIMITIVE
        }));
        assert!(
            attributes
                .iter()
                .any(|(name, value)| name == "data-review-state" && value == "refused")
        );
        assert!(
            attributes
                .iter()
                .any(|(name, value)| name == "class" && value == "credential-review-surface")
        );
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

        let attributes = rendered_attributes(empty_state_harness);
        assert!(attributes.iter().any(|(name, value)| {
            name == "data-ui-primitive" && value == EMPTY_STATE_PRIMITIVE
        }));
    }
}
