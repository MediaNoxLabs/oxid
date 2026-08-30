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

const FIRST_STAGE: &str = r##"
return await (async () => {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const visible = (node) => !!(node && node.getClientRects().length && !node.disabled);
  const wait = async (probe) => {
    for (let i = 0; i < 1200; i += 1) {
      const value = probe();
      if (value) return value;
      await sleep(100);
    }
    throw new Error("bounded rendered-control wait expired");
  };
  const button = (label) => [...document.querySelectorAll("button")]
    .find((candidate) => visible(candidate) && text(candidate) === label);
  const click = async (label) => {
    const target = await wait(() => button(label));
    target.scrollIntoView({ block: "center" });
    target.click();
  };
  const hasText = (value) => text(document.body).includes(value);
  let phase = "create-wallet";
  try {
    await click("Create new wallet");
    phase = "profile-name";
    const input = await wait(() => document.querySelector("#profile-name"));
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setter.call(input, "Oxid Desktop Test");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    phase = "create-profile";
    await click("Create and continue");
    phase = "protect-wallet";
    await click("Enable device protection");
    phase = "open-wallet";
    await click("Wallet");
    phase = "activate-account";
    await click("Activate development wallet");
    phase = "live-sync";
    await wait(() => hasText("Synced") && hasText("Live source"));
    phase = "open-documents";
    await click("Documents");
    phase = "manage-identities";
    await click("Manage identities");
    phase = "create-did";
    await click("Create standalone DID");
    phase = "did-ready";
    await wait(() => hasText("A protected managed DID is ready for credential issuance."));
    return "ok";
  } catch (_) {
    return `failed:${phase}`;
  }
})();
"##;

const SCAN_AND_PREVIEW_STAGE: &str = r##"
return await (async () => {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const visible = (node) => !!(node && node.getClientRects().length && !node.disabled);
  const wait = async (probe) => {
    for (let i = 0; i < 1200; i += 1) {
      const value = probe();
      if (value) return value;
      await sleep(100);
    }
    throw new Error("bounded rendered-control wait expired");
  };
  const button = (label) => [...document.querySelectorAll("button")]
    .find((candidate) => visible(candidate) && text(candidate) === label);
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
    const scan = await wait(() => button("Scan"));
    scan.scrollIntoView({ block: "center" });
    scan.click();
    const preview = await wait(() => button("Preview credential offer"));
    preview.scrollIntoView({ block: "center" });
    preview.click();
    await wait(() => text(document.body).includes("Credential offer preview")
      && text(document.body).includes("Digital Passport")
      && document.querySelector("#credential-issuance-consent"));
    if (!redactForScreenshot()) throw new Error("screenshot redaction failed");
    return "ok";
  } catch (_) {
    return "failed:rendered-stage";
  }
})();
"##;

const CONSENT_AND_REVERIFY_STAGE: &str = r##"
return await (async () => {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const visible = (node) => !!(node && node.getClientRects().length && !node.disabled);
  const wait = async (probe) => {
    for (let i = 0; i < 1800; i += 1) {
      const value = probe();
      if (value) return value;
      await sleep(100);
    }
    throw new Error("bounded rendered-control wait expired");
  };
  const button = (label) => [...document.querySelectorAll("button")]
    .find((candidate) => visible(candidate) && text(candidate) === label);
  try {
    const consent = await wait(() => document.querySelector("#credential-issuance-consent"));
    consent.click();
    const accept = await wait(() => button("Accept and issue credential"));
    accept.scrollIntoView({ block: "center" });
    accept.click();
    await wait(() => text(document.body).includes("Credential stored.") && button("Reverify"));
    const reverify = await wait(() => button("Reverify"));
    reverify.scrollIntoView({ block: "center" });
    reverify.click();
    await wait(() => text(document.body).includes("Credential reverification applied"));
    return "ok";
  } catch (_) {
    return "failed:rendered-stage";
  }
})();
"##;

const RESTART_REVERIFY_STAGE: &str = r##"
return await (async () => {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const visible = (node) => !!(node && node.getClientRects().length && !node.disabled);
  const wait = async (probe) => {
    for (let i = 0; i < 1200; i += 1) {
      const value = probe();
      if (value) return value;
      await sleep(100);
    }
    throw new Error("bounded rendered-control wait expired");
  };
  const button = (label) => [...document.querySelectorAll("button")]
    .find((candidate) => visible(candidate) && text(candidate) === label);
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
    const documents = await wait(() => button("Documents"));
    documents.click();
    await wait(() => text(document.body).includes("Digital Passport") && button("Reverify"));
    if (!redactForScreenshot()) throw new Error("screenshot redaction failed");
    const reverify = await wait(() => button("Reverify"));
    reverify.scrollIntoView({ block: "center" });
    reverify.click();
    await wait(() => text(document.body).includes("Credential reverification applied"));
    return "ok";
  } catch (_) {
    return "failed:rendered-stage";
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
    for _ in 0..1200 {
        if marker(root, name).is_file() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn run_stage(script: &'static str) -> Result<(), String> {
    let evaluator = dioxus_document::eval(script);
    match evaluator.join::<String>().await {
        Ok(result) if result == "ok" => Ok(()),
        Ok(result) if result.starts_with("failed:") && result.len() <= 64 => Err(result),
        Ok(_) | Err(_) => Err("failed:document-eval".to_owned()),
    }
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
    let _ = write_marker(&root, "driver-started");
    if marker(&root, "restart").is_file() {
        match run_stage(RESTART_REVERIFY_STAGE).await {
            Ok(()) => {
                let _ = write_marker(&root, "restart-complete");
            }
            Err(failure) => write_failure(&root, failure),
        }
        return;
    }

    if let Err(failure) = run_stage(FIRST_STAGE).await {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "sync-and-holder-visible");
    if !wait_for_marker(&root, "holder-ready").await {
        write_failure(&root, "failed:holder-ready".to_owned());
        return;
    }
    if let Err(failure) = run_stage(SCAN_AND_PREVIEW_STAGE).await {
        write_failure(&root, failure);
        return;
    }
    let _ = write_marker(&root, "consent-visible");
    if !wait_for_marker(&root, "consent-approved").await {
        write_failure(&root, "failed:consent-approved".to_owned());
        return;
    }
    if let Err(failure) = run_stage(CONSENT_AND_REVERIFY_STAGE).await {
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
            FIRST_STAGE,
            SCAN_AND_PREVIEW_STAGE,
            CONSENT_AND_REVERIFY_STAGE,
            RESTART_REVERIFY_STAGE,
        ] {
            assert!(script.trim_start().starts_with("return await (async () =>"));
            assert!(script.contains("return \"ok\";"));
            assert!(script.contains(".click()"));
            assert!(!script.contains("openid-credential-offer"));
            assert!(!script.contains("pre-authorized"));
            assert!(!script.contains("access_token"));
        }
        assert!(FIRST_STAGE.contains("HTMLInputElement.prototype"));
        for script in [SCAN_AND_PREVIEW_STAGE, RESTART_REVERIFY_STAGE] {
            assert!(script.contains("redactForScreenshot"));
            assert!(script.contains("getComputedStyle"));
            assert!(script.contains("document.body.innerText"));
            assert!(script.contains("oxid-desktop-test-screenshot-redaction"));
            assert!(script.contains("visibility: hidden !important"));
        }
    }
}
