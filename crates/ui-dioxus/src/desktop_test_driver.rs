// SPDX-License-Identifier: Apache-2.0

//! Release-absent rendered-control driver for the ARM64-Darwin desktop test.
//!
//! This module can only click controls that Dioxus rendered. It has no access
//! to wallet services, scanner/router ports, or application use cases.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use dioxus::prelude::*;

const FIRST_STAGE: &str = r##"
(async () => {
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
  try {
    await click("Create new wallet");
    const input = await wait(() => document.querySelector("#profile-name"));
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setter.call(input, "Oxid Desktop Test");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await click("Create and continue");
    await click("Enable device protection");
    await click("Wallet");
    await click("Activate development wallet");
    await wait(() => hasText("Synced") && hasText("Live source"));
    await click("Documents");
    await click("Manage identities");
    await click("Create standalone DID");
    await wait(() => hasText("A protected managed DID is ready for credential issuance."));
    dioxus.send("ok");
  } catch (_) {
    dioxus.send("failed");
  }
})();
"##;

const SCAN_AND_PREVIEW_STAGE: &str = r##"
(async () => {
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
    for (const sensitive of document.querySelectorAll("textarea, code, .privacy-value, .privacy-qr")) {
      sensitive.style.visibility = "hidden";
    }
    dioxus.send("ok");
  } catch (_) {
    dioxus.send("failed");
  }
})();
"##;

const CONSENT_AND_REVERIFY_STAGE: &str = r##"
(async () => {
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
    dioxus.send("ok");
  } catch (_) {
    dioxus.send("failed");
  }
})();
"##;

const RESTART_REVERIFY_STAGE: &str = r##"
(async () => {
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
  try {
    const documents = await wait(() => button("Documents"));
    documents.click();
    await wait(() => text(document.body).includes("Digital Passport") && button("Reverify"));
    for (const sensitive of document.querySelectorAll("textarea, code, .privacy-value, .privacy-qr")) {
      sensitive.style.visibility = "hidden";
    }
    const reverify = await wait(() => button("Reverify"));
    reverify.scrollIntoView({ block: "center" });
    reverify.click();
    await wait(() => text(document.body).includes("Credential reverification applied"));
    dioxus.send("ok");
  } catch (_) {
    dioxus.send("failed");
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

async fn run_stage(script: &'static str) -> bool {
    let mut evaluator = dioxus_document::eval(script);
    evaluator
        .recv::<String>()
        .await
        .is_ok_and(|result| result == "ok")
}

async fn run_driver() {
    let Some(root) = control_root() else {
        return;
    };
    if marker(&root, "restart").is_file() {
        let completed = run_stage(RESTART_REVERIFY_STAGE).await;
        let _ = write_marker(
            &root,
            if completed {
                "restart-complete"
            } else {
                "driver-failed"
            },
        );
        return;
    }

    if !run_stage(FIRST_STAGE).await {
        let _ = write_marker(&root, "driver-failed");
        return;
    }
    let _ = write_marker(&root, "sync-and-holder-visible");
    if !wait_for_marker(&root, "holder-ready").await || !run_stage(SCAN_AND_PREVIEW_STAGE).await {
        let _ = write_marker(&root, "driver-failed");
        return;
    }
    let _ = write_marker(&root, "consent-visible");
    if !wait_for_marker(&root, "consent-approved").await
        || !run_stage(CONSENT_AND_REVERIFY_STAGE).await
    {
        let _ = write_marker(&root, "driver-failed");
        return;
    }
    let _ = write_marker(&root, "first-complete");
}

#[component]
pub(super) fn DesktopTestDriver() -> Element {
    use_future(run_driver);
    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_click_only_rendered_controls_and_never_embed_protocol_payloads() {
        for script in [
            FIRST_STAGE,
            SCAN_AND_PREVIEW_STAGE,
            CONSENT_AND_REVERIFY_STAGE,
            RESTART_REVERIFY_STAGE,
        ] {
            assert!(script.contains(".click()"));
            assert!(!script.contains("openid-credential-offer"));
            assert!(!script.contains("pre-authorized"));
            assert!(!script.contains("access_token"));
        }
    }
}
