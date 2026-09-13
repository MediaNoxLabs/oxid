// SPDX-License-Identifier: Apache-2.0

//! Release-absent rendered-control driver for the owner-invoked developer pager smoke.
//! It can only click Dioxus-rendered controls and move the rendered pager; it never
//! starts a proof or calls application services.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use dioxus::prelude::*;

const CONTROL_DIRECTORY: &str = "developer-pager-test";
const ATTEMPTS: usize = 100;
const PROBE_TIMEOUT: Duration = Duration::from_millis(200);

const PROFILE_CREATION_STAGE: &str = r##"
return await (async () => {
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const button = (label) => [...document.querySelectorAll("button")]
    .find((node) => node.getClientRects().length && !node.disabled && text(node) === label);
  const createPrivate = button("Create private wallet");
  if (createPrivate) {
    createPrivate.click();
    return "pending";
  }
  const input = document.querySelector("#profile-name");
  if (!input) return "pending";
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set
    .call(input, "Developer Pager Test");
  input.dispatchEvent(new Event("input", { bubbles: true }));
  const fixture = button("Use public demo wallet");
  if (!fixture) return "pending";
  fixture.click();
  const create = button("Create and continue");
  if (!create) return "pending";
  create.click();
  return "ok";
})();
"##;

const PROFILE_PROTECTION_STAGE: &str = r##"
return await (async () => {
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const target = [...document.querySelectorAll("button")]
    .find((node) => node.getClientRects().length && !node.disabled
      && text(node) === "Enable device protection");
  if (!target) return "pending";
  target.scrollIntoView({ block: "center" });
  target.click();
  return "ok";
})();
"##;

fn control_root() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).map(|home| {
        home.join("Library/Application Support/io.medianox.oxid")
            .join(CONTROL_DIRECTORY)
    })
}

fn marker(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}
fn write_marker(root: &Path, name: &str) {
    let _ = fs::write(marker(root, name), b"ok\n");
}

fn write_failure(root: &Path, failure: &str) {
    let code = if failure.starts_with("failed:") && failure.len() <= 64 {
        failure
    } else {
        "failed:invalid-code"
    };
    let _ = fs::write(marker(root, "driver-failed"), format!("{code}\n"));
}

async fn wait_for_marker(root: &Path, name: &str) -> bool {
    for _ in 0..ATTEMPTS {
        if marker(root, name).is_file() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn stage(script: &str, failure: &str) -> Result<(), String> {
    let mut pending = None;
    for _ in 0..ATTEMPTS {
        let evaluator = dioxus_document::eval(script);
        match tokio::time::timeout(PROBE_TIMEOUT, evaluator.join::<String>()).await {
            Ok(Ok(result)) if result == "ok" => return Ok(()),
            Ok(Ok(result)) if result.starts_with("failed:") => return Err(result),
            Ok(Ok(result)) if result == "pending" || result.starts_with("pending:") => {
                pending = Some(result);
            }
            Ok(Ok(_)) => return Err(format!("failed:{failure}-eval")),
            Ok(Err(_)) => pending = Some("pending:evaluation".to_owned()),
            Err(_) => pending = Some("pending:evaluation-timeout".to_owned()),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if let Some(detail) =
        pending.and_then(|result| result.strip_prefix("pending:").map(str::to_owned))
    {
        return Err(format!("failed:{failure}-{detail}"));
    }
    Err(format!("failed:{failure}"))
}

fn rendered_stage(action: &str, expected: &str, viewport: &str) -> String {
    let (expected_width, expected_height) = match viewport {
        "360x640" => (360, 640),
        "390x844" => (390, 844),
        _ => return "return \"failed:viewport\";".to_owned(),
    };
    let page_title = match expected {
        "Capabilities" => "Capability manifest",
        "Benchmark" => "Proof benchmark",
        "Event log" => "Event log",
        _ => expected,
    };
    let page_index = match expected {
        "Capabilities" => 0,
        "Benchmark" => 1,
        "Event log" => 2,
        _ => 0,
    };
    format!(
        r##"
return await (async () => {{
  const text = (node) => (node.textContent || "").replace(/\s+/g, " ").trim();
  const button = (label) => [...document.querySelectorAll("button")]
    .find((node) => node.getClientRects().length && !node.disabled && text(node) === label);
  const redact = () => {{
    const id = "oxid-developer-pager-screenshot-redaction";
    let style = document.getElementById(id);
    if (!style) {{ style = document.createElement("style"); style.id = id; document.head.appendChild(style); }}
    style.textContent = "code, textarea, .privacy-value, .privacy-qr {{ visibility: hidden !important; }}";
    const sensitive = [...document.querySelectorAll("code, textarea, .privacy-value, .privacy-qr")];
    const visible = document.body.innerText || "";
    return sensitive.every((node) => getComputedStyle(node).visibility === "hidden")
      && !["openid-credential-offer", "access_token", "pre-authorized", "did:", "ab1234567"]
        .some((value) => visible.toLowerCase().includes(value));
  }};
  if (window.innerWidth !== {expected_width} || window.innerHeight !== {expected_height}) return "failed:viewport";
  const readiness = () => {{
    if ({expected:?} === "menu") {{
      const menu = document.querySelector("#global-application-menu");
      if (!menu) return "pending:no-menu";
      return button("Developer tools") ? "ok" : "pending:no-developer-tools";
    }}
    if ({expected:?} === "hub") {{
      return [...document.querySelectorAll("h1")].some((node) => text(node) === "Developer tools")
        ? "ok" : "pending:no-hub";
    }}
    const pager = document.querySelector(".developer-section-pager");
    if (!pager || !pager.clientWidth) return "pending:no-pager";
    const active = document.querySelector(".developer-section-nav__item.active");
    const page = document.querySelector(".developer-section-pager__page[aria-hidden=\"false\"]");
    if (!active) return "pending:no-active-chip";
    if (text(active) !== {expected:?}) return "pending:active-chip-mismatch";
    if (!page) return "pending:no-current-page";
    if (!text(page).includes({page_title:?})) return "pending:page-mismatch";
    return Math.abs(pager.scrollLeft - (pager.clientWidth * {page_index})) <= 2
      ? "ok" : "pending:scroll-mismatch";
  }};
  let state = readiness();
  if (state === "ok") return {expected:?} === "menu" || {expected:?} === "hub" || redact() ? "ok" : "failed:redaction";
  const action = {action:?};
  if (action.startsWith("scroll:")) {{
    const pager = document.querySelector(".developer-section-pager");
    if (!pager || !pager.clientWidth) return "pending";
    pager.scrollTo({{ left: pager.clientWidth * Number(action.slice(7)), behavior: "instant" }});
    pager.dispatchEvent(new Event("scroll", {{ bubbles: true }}));
  }} else {{
    const target = action === "Open global application menu"
      ? document.querySelector("button.global-menu-trigger")
      : action === "Back"
        ? document.querySelector("button.back-action")
      : button(action);
    if (!target) return state;
    target.scrollIntoView({{ block: "center" }}); target.click();
  }}
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  state = readiness();
  if (state !== "ok") return state;
  return {expected:?} === "menu" || {expected:?} === "hub" || redact() ? "ok" : "failed:redaction";
}})();
"##
    )
}

async fn run_driver() {
    let Some(root) = control_root() else {
        return;
    };
    if fs::create_dir_all(&root).is_err() || marker(&root, "driver-admitted").exists() {
        return;
    }
    let Ok(viewport) = env::var("OXID_DEVELOPER_PAGER_VIEWPORT") else {
        write_failure(&root, "failed:viewport");
        return;
    };
    write_marker(&root, "driver-admitted");
    if !wait_for_marker(&root, "window-ready").await {
        write_failure(&root, "failed:window-ready");
        return;
    }
    if let Err(failure) = stage(PROFILE_CREATION_STAGE, "profile-creation").await {
        write_failure(&root, &failure);
        return;
    }
    if let Err(failure) = stage(PROFILE_PROTECTION_STAGE, "profile-protection").await {
        write_failure(&root, &failure);
        return;
    }
    write_marker(&root, "profile-created");
    for (action, expected, ready) in [
        ("Open global application menu", "menu", "global-menu"),
        ("Developer tools", "hub", "hub"),
        ("Open manifest", "Capabilities", "capabilities"),
    ] {
        if let Err(failure) = stage(&rendered_stage(action, expected, &viewport), ready).await {
            write_failure(&root, &failure);
            return;
        }
        write_marker(&root, ready);
        if ready == "capabilities" && !wait_for_marker(&root, "capture-capabilities").await {
            write_failure(&root, "failed:capture-capabilities");
            return;
        }
    }
    if let Err(failure) = stage(
        &rendered_stage("Benchmark", "Benchmark", &viewport),
        "benchmark",
    )
    .await
    {
        write_failure(&root, &failure);
        return;
    }
    write_marker(&root, "benchmark");
    if !wait_for_marker(&root, "capture-benchmark").await {
        write_failure(&root, "failed:capture-benchmark");
        return;
    }
    if let Err(failure) = stage(
        &rendered_stage("scroll:2", "Event log", &viewport),
        "event-log",
    )
    .await
    {
        write_failure(&root, &failure);
        return;
    }
    write_marker(&root, "event-log");
    if !wait_for_marker(&root, "capture-event-log").await {
        write_failure(&root, "failed:capture-event-log");
        return;
    }
    if let Err(failure) = stage(&rendered_stage("Back", "hub", &viewport), "back").await {
        write_failure(&root, &failure);
        return;
    }
    write_marker(&root, "complete");
}

pub(super) fn use_desktop_developer_pager_driver() {
    use_effect(move || {
        spawn(async move { run_driver().await });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn driver_is_rendered_control_only_and_redacts_captures() {
        let script = rendered_stage("scroll:2", "Event log", "390x844");
        assert!(script.contains("developer-section-pager"));
        assert!(script.contains("developer-section-nav__item.active"));
        assert!(script.contains("aria-hidden"));
        assert!(script.contains("oxid-developer-pager-screenshot-redaction"));
        assert!(script.contains("visibility: hidden !important"));
        assert!(script.contains("window.innerWidth"));
        assert!(script.contains("getComputedStyle"));
        assert!(script.contains("Open global application menu"));
        assert!(script.contains("button.global-menu-trigger"));
        assert!(script.contains("button.back-action"));
        assert!(script.contains("global-application-menu"));
        assert!(PROFILE_CREATION_STAGE.contains("Create private wallet"));
        assert!(PROFILE_CREATION_STAGE.contains("Use public demo wallet"));
        assert!(PROFILE_PROTECTION_STAGE.contains("Enable device protection"));
        assert!(!script.contains("UseCase"));
        assert!(!script.contains(&[".", "execute("].concat()));
        assert_eq!(CONTROL_DIRECTORY, "developer-pager-test");
    }
}
