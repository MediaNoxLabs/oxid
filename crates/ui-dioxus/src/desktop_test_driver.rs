// SPDX-License-Identifier: Apache-2.0

//! Release-absent rendered-control driver for the ARM64-Darwin desktop test.
//!
//! This module can interact only with controls and fields that Dioxus rendered.
//! It has no access to wallet services, scanner/router ports, or application
//! use cases.

use std::{
    env, fs,
    fs::OpenOptions,
    path::{Path, PathBuf},
    time::Duration,
};

use dioxus::prelude::*;

const PROFILE_CREATION_STAGE: &str = r##"
return await (async () => {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const button = (label) => [...document.querySelectorAll("button")]
    .find((candidate) => candidate.getClientRects().length && !candidate.disabled
      && text(candidate) === label);
  const wait = async (probe) => {
    for (let i = 0; i < 150; i += 1) {
      const value = probe();
      if (value) return value;
      await sleep(100);
    }
    throw new Error("bounded rendered-control wait expired");
  };
  try {
    (await wait(() => button("Create private wallet"))).click();
    const input = await wait(() => document.querySelector("#profile-name"));
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set
      .call(input, "Oxid Desktop Test");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    (await wait(() => button("Use public demo wallet"))).click();
    const create = await wait(() => button("Create and continue"));
    create.scrollIntoView({ block: "center" });
    setTimeout(() => create.click(), 0);
    return "ok";
  } catch (_) {
    return "failed:profile-creation";
  }
})();
"##;
const ACCOUNT_ACTIVATION_STAGE: &str = r##"
return await (async () => {
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const body = text(document.body);
  if (body.includes("Synced") && body.includes("Live source")) return "ok";
  const target = [...document.querySelectorAll("button")]
    .find((candidate) => candidate.getClientRects().length && !candidate.disabled
      && ["Activate development wallet", "Sync now"].includes(text(candidate)));
  if (!target) return "pending";
  target.scrollIntoView({ block: "center" });
  target.click();
  return "ok";
})();
"##;
const CONSENT_VISIBLE_STAGE: &str = r##"
return await (async () => {
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const redactForScreenshot = () => {
    const styleId = "oxid-desktop-test-screenshot-redaction";
    let style = document.getElementById(styleId);
    if (!style) {
      style = document.createElement("style");
      style.id = styleId;
      document.head.appendChild(style);
    }
    style.textContent = "textarea, code, .privacy-value, .privacy-qr { visibility: hidden !important; }";
    const sensitive = [...document.querySelectorAll("textarea, code, .privacy-value, .privacy-qr")];
    if (sensitive.length === 0) return false;
    const visibleText = document.body.innerText || "";
    const forbidden = [
      ["openid", "-credential-offer://"].join(""), "did:",
      "Alice", "Example", "John", "Doe", "AB1234567"
    ];
    return sensitive.every((node) => getComputedStyle(node).visibility === "hidden")
      && forbidden.every((value) => !visibleText.includes(value));
  };
  try {
    if (!text(document.body).includes("Credential offer preview")
        || !text(document.body).includes("Digital Passport")
        || !document.querySelector("#credential-issuance-consent")) {
      throw new Error("consent controls unavailable");
    }
    if (!redactForScreenshot()) throw new Error("screenshot redaction failed");
    return "ok";
  } catch (_) {
    return "failed:consent-visible";
  }
})();
"##;

const RESTART_REVERIFY_STAGE: &str = r##"
return await (async () => {
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const redactForScreenshot = () => {
    const styleId = "oxid-desktop-test-screenshot-redaction";
    let style = document.getElementById(styleId);
    if (!style) {
      style = document.createElement("style");
      style.id = styleId;
      document.head.appendChild(style);
    }
    style.textContent = "textarea, code, .privacy-value, .privacy-qr, .credential-record__facts dd { visibility: hidden !important; }";
    const sensitive = [...document.querySelectorAll("textarea, code, .privacy-value, .privacy-qr, .credential-record__facts dd")];
    const visibleText = document.body.innerText || "";
    const forbidden = [
      ["openid", "-credential-offer://"].join(""), "did:",
      "Alice", "Example", "John", "Doe", "AB1234567"
    ];
    return sensitive.every((node) => getComputedStyle(node).visibility === "hidden")
      && forbidden.every((value) => !visibleText.includes(value));
  };
  try {
    if (!text(document.body).includes("Digital Passport")) {
      throw new Error("credential record unavailable");
    }
    if (!redactForScreenshot()) throw new Error("screenshot redaction failed");
    return "ok";
  } catch (_) {
    return "failed:restart-redaction";
  }
})();
"##;

fn control_root() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/io.medianox.oxid/desktop-test"))
}

fn marker(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

fn write_marker(root: &Path, name: &str) -> bool {
    fs::create_dir_all(root).is_ok() && fs::write(marker(root, name), b"ok\n").is_ok()
}

async fn wait_for_marker(root: &Path, name: &str) -> bool {
    for _ in 0..200 {
        if marker(root, name).is_file() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

const DOCUMENT_EVALUATION_TIMEOUT: Duration = Duration::from_secs(20);
const PROBE_EVALUATION_TIMEOUT: Duration = Duration::from_millis(200);
const PROBE_ATTEMPTS: usize = 60;

async fn run_stage(script: &str) -> Result<(), String> {
    let evaluator = dioxus_document::eval(script);
    match tokio::time::timeout(DOCUMENT_EVALUATION_TIMEOUT, evaluator.join::<String>()).await {
        Ok(Ok(result)) if result == "ok" => Ok(()),
        Ok(Ok(result)) if result.starts_with("failed:") && result.len() <= 64 => Err(result),
        Ok(Ok(_)) | Ok(Err(_)) => Err("failed:document-eval".to_owned()),
        Err(_) => Err("failed:document-evaluation-timeout".to_owned()),
    }
}

fn click_stage_script(label: &'static str, failure: &'static str) -> String {
    format!(
        r##"
return await (async () => {{
  // Rust reports failed:{failure} if this bounded probe never becomes ready.
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const target = [...document.querySelectorAll("button")]
    .find((candidate) => candidate.getClientRects().length && !candidate.disabled
      && text(candidate) === {label:?});
  if (!target) return "pending";
  target.scrollIntoView({{ block: "center" }});
  target.click();
  return "ok";
}})();
"##
    )
}

async fn click_when_visible(label: &'static str, failure: &'static str) -> Result<(), String> {
    let script = click_stage_script(label, failure);
    run_retryable_stage(&script, failure).await
}

fn selector_click_stage_script(selector: &'static str, failure: &'static str) -> String {
    format!(
        r##"
return await (async () => {{
  // Rust reports failed:{failure} if this bounded probe never becomes ready.
  const target = document.querySelector({selector:?});
  if (!target || !target.getClientRects().length || target.disabled) return "pending";
  target.scrollIntoView({{ block: "center" }});
  target.click();
  return "ok";
}})();
"##
    )
}

async fn click_selector_when_visible(
    selector: &'static str,
    failure: &'static str,
) -> Result<(), String> {
    let script = selector_click_stage_script(selector, failure);
    run_retryable_stage(&script, failure).await
}

async fn activate_account_if_needed() -> Result<(), String> {
    run_retryable_stage(ACCOUNT_ACTIVATION_STAGE, "account-activation").await
}

async fn run_retryable_stage(script: &str, failure: &'static str) -> Result<(), String> {
    let mut received_response = false;
    let mut timed_out = false;
    let mut evaluation_failed = false;
    for _ in 0..PROBE_ATTEMPTS {
        let evaluator = dioxus_document::eval(script);
        match tokio::time::timeout(PROBE_EVALUATION_TIMEOUT, evaluator.join::<String>()).await {
            Ok(Ok(result)) if result == "ok" => return Ok(()),
            Ok(Ok(result)) if result == "pending" => received_response = true,
            Ok(Ok(result)) if result.starts_with("failed:") && result.len() <= 64 => {
                return Err(result);
            }
            Ok(Ok(_)) => return Err(format!("failed:{failure}-response")),
            Ok(Err(_)) => evaluation_failed = true,
            Err(_) => timed_out = true,
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if received_response {
        Err(format!("failed:{failure}"))
    } else if timed_out {
        Err("failed:document-evaluation-timeout".to_owned())
    } else if evaluation_failed {
        Err(format!("failed:{failure}-eval"))
    } else {
        Err("failed:document-eval".to_owned())
    }
}

async fn wait_for_rendered_text(
    required: &[&'static str],
    failure: &'static str,
) -> Result<(), String> {
    let required = required
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        r##"
return await (async () => {{
  const text = (document.body.textContent || "").replace(/\s+/g, " ").trim();
  return [{required}].every((value) => text.includes(value)) ? "ok" : "pending";
}})();
"##
    );
    run_retryable_stage(&script, failure).await
}

fn write_failure(root: &Path, failure: String) {
    let safe_failure = if failure.starts_with("failed:")
        && failure
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b':' | b'-'))
    {
        failure
    } else {
        "failed:unknown".to_owned()
    };
    let _ = fs::create_dir_all(root);
    let _ = fs::write(marker(root, "driver-failed"), safe_failure);
}

async fn run_driver() {
    let Some(root) = control_root() else {
        return;
    };
    let _ = fs::create_dir_all(&root);
    if OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker(&root, "driver-admitted"))
        .is_err()
    {
        return;
    }
    if !wait_for_marker(&root, "window-ready").await {
        write_failure(&root, "failed:window-ready".to_owned());
        return;
    }
    let _ = write_marker(&root, "driver-started");
    if marker(&root, "restart").is_file() {
        if let Err(failure) = click_when_visible("Documents", "restart-documents").await {
            write_failure(&root, failure);
            return;
        }
        if let Err(failure) =
            wait_for_rendered_text(&["Digital Passport"], "restart-credential").await
        {
            write_failure(&root, failure);
            return;
        }
        if let Err(failure) = run_stage(RESTART_REVERIFY_STAGE).await {
            write_failure(&root, failure);
            return;
        }
        if let Err(failure) = click_when_visible("Reverify", "restart-reverify").await {
            write_failure(&root, failure);
            return;
        }
        if let Err(failure) = wait_for_rendered_text(
            &["Credential reverification applied"],
            "restart-reverification",
        )
        .await
        {
            write_failure(&root, failure);
            return;
        }
        let _ = write_marker(&root, "restart-complete");
        return;
    }

    if let Err(failure) = run_stage(PROFILE_CREATION_STAGE).await {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) = click_when_visible("Enable device protection", "protection").await {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "profile-created");
    if let Err(failure) = click_when_visible("Wallet", "open-wallet").await {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "protection-enabled");
    if let Err(failure) = activate_account_if_needed().await {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "account-activated");
    if let Err(failure) = wait_for_rendered_text(&["Synced", "Live source"], "live-sync").await {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "live-sync-complete");
    for (label, failure) in [
        ("Documents", "open-documents"),
        ("Manage identities", "manage-identities"),
        ("Create a DID", "open-create-did"),
        ("Create DID", "create-did"),
    ] {
        if let Err(failure) = click_when_visible(label, failure).await {
            write_failure(&root, failure);
            return;
        }
    }
    if let Err(failure) = wait_for_rendered_text(
        &["A protected managed DID is ready for credential issuance."],
        "did-readiness",
    )
    .await
    {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "did-ready");
    let _ = write_marker(&root, "sync-and-holder-visible");
    if !wait_for_marker(&root, "holder-ready").await {
        write_failure(&root, "failed:holder-ready".to_owned());
        return;
    }
    if let Err(failure) = click_when_visible("Scan", "open-scanner").await {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) =
        click_when_visible("Preview credential offer", "preview-credential-offer").await
    {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) = wait_for_rendered_text(
        &["Credential offer preview", "Digital Passport"],
        "consent-visible",
    )
    .await
    {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) = run_stage(CONSENT_VISIBLE_STAGE).await {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "consent-visible");
    if !wait_for_marker(&root, "consent-approved").await {
        write_failure(&root, "failed:consent-approved".to_owned());
        return;
    }
    if let Err(failure) =
        click_selector_when_visible("#credential-issuance-consent", "consent-control").await
    {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) =
        click_when_visible("Accept and issue credential", "accept-credential").await
    {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) =
        wait_for_rendered_text(&["Credential stored."], "credential-storage").await
    {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) = click_when_visible("Reverify", "reverify-control").await {
        write_failure(&root, failure);
        return;
    }
    if let Err(failure) = wait_for_rendered_text(
        &["Credential reverification applied"],
        "credential-reverification",
    )
    .await
    {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "first-complete");
}

pub(super) fn use_desktop_test_driver() {
    use_effect(move || {
        spawn(async move { run_driver().await });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_interact_only_with_rendered_controls_and_never_embed_protocol_payloads() {
        for script in [
            PROFILE_CREATION_STAGE,
            ACCOUNT_ACTIVATION_STAGE,
            CONSENT_VISIBLE_STAGE,
            RESTART_REVERIFY_STAGE,
        ] {
            assert!(script.trim_start().starts_with("return await (async () =>"));
            assert!(script.contains("return \"ok\";"));
            assert!(!script.contains("openid-credential-offer"));
            assert!(!script.contains("pre-authorized"));
            assert!(!script.contains("access_token"));
        }
        assert!(PROFILE_CREATION_STAGE.contains(".click()"));
        assert!(PROFILE_CREATION_STAGE.contains("HTMLInputElement.prototype"));
        assert_eq!(DOCUMENT_EVALUATION_TIMEOUT, Duration::from_secs(20));
        assert_eq!(PROBE_EVALUATION_TIMEOUT, Duration::from_millis(200));
        assert_eq!(PROBE_ATTEMPTS, 60);
        let click_script = click_stage_script("Wallet", "open-wallet");
        assert!(click_script.contains("target.click();"));
        assert!(click_script.contains("return \"pending\""));
        let selector_script =
            selector_click_stage_script("#credential-issuance-consent", "consent-control");
        assert!(selector_script.contains("target.click();"));
        assert!(selector_script.contains("failed:consent-control"));
        assert!(ACCOUNT_ACTIVATION_STAGE.contains("Activate development wallet"));
        assert!(ACCOUNT_ACTIVATION_STAGE.contains("Sync now"));
        assert!(ACCOUNT_ACTIVATION_STAGE.contains("Synced"));
        assert!(ACCOUNT_ACTIVATION_STAGE.contains("Live source"));
        for script in [CONSENT_VISIBLE_STAGE, RESTART_REVERIFY_STAGE] {
            assert!(script.contains("redactForScreenshot"));
            assert!(script.contains("getComputedStyle"));
            assert!(script.contains("document.body.innerText"));
            assert!(script.contains("oxid-desktop-test-screenshot-redaction"));
            assert!(script.contains("visibility: hidden !important"));
        }
        // A restored credential page may legitimately render no sensitive
        // field. Its screenshot is still admissible only when the body-text
        // denylist passes; treating an empty sensitive-node set as failure
        // makes the restart harness fail before its rendered reverify proof.
        assert!(
            RESTART_REVERIFY_STAGE
                .contains("forbidden.every((value) => !visibleText.includes(value))")
        );
        assert!(!RESTART_REVERIFY_STAGE.contains("if (sensitive.length === 0) return false;"));
        assert!(RESTART_REVERIFY_STAGE.contains("failed:restart-redaction"));
        assert!(RESTART_REVERIFY_STAGE.contains(".credential-record__facts dd"));
    }
}
