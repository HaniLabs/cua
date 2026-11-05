// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Cua AI, Inc.

//! Experimental browser automation-hiding script for an explicitly approved navigation.
//!
//!
//! This script changes page-visible browser properties. It is registered only
//! for one explicitly approved navigation attempt, an exact HTTPS origin and
//! top-level frame. Registration is temporary; applied properties remain in a
//! loaded document until it is replaced. A dedicated trusted host must approve
//! the additional effect separately from ordinary profile attachment.

/// What a Chromium-derived automation stack gets wrong, and what a person's Chrome reports.
///
/// Kept as one string so it installs atomically as a document-start script. It runs before any
/// page script, in every frame; the interval at the end outlives the document commit, which is
/// where Chromium re-defines the automation property.
const HIDE_AUTOMATION_SCRIPT: &str = r#"
(() => {
  const define = (target, name, value) => {
    try {
      Object.defineProperty(target, name, { get: () => value, configurable: true, enumerable: true });
    } catch (error) {
      try { target[name] = value; } catch (_) {}
    }
  };

  // 1. The flag itself. Re-applied on an interval because Chromium re-defines it when a document
  //    commits its automation state, after document-start scripts have run — a single delete is
  //    undone a moment later.
  const hideWebdriver = () => {
    if (navigator.webdriver === false) return;
    try {
      // Chromium keeps the getter on the prototype, so deleting the instance property alone
      // leaves it `true`.
      Object.defineProperty(Navigator.prototype, 'webdriver', { get: () => false, configurable: true });
      delete navigator.webdriver;
    } catch (_) {}
  };
  hideWebdriver();
  const interval = setInterval(hideWebdriver, 30);
  setTimeout(() => clearInterval(interval), 15000);

  // 2. `window.chrome` is how a page checks for a Chromium build that is not a bare automation
  //    shell. Real Chrome is missing pieces of it with extensions disabled.
  const chrome = (window.chrome = window.chrome || {});
  if (!chrome.runtime) {
    chrome.runtime = {
      onConnect: { addListener() {}, removeListener() {}, hasListener: () => false },
      connect: () => ({ disconnect() {}, postMessage() {}, onMessage: { addListener() {} } }),
      sendMessage: () => {},
      getManifest: () => ({}),
      id: undefined
    };
  }
  if (!chrome.csi) chrome.csi = () => ({ onloadT: Date.now(), startE: Date.now(), pageT: 0, tran: 15 });
  if (!chrome.loadTimes) {
    chrome.loadTimes = () => ({
      commitLoadTime: 0, connectionInfo: 'h2', finishDocumentLoadTime: 0, finishLoadTime: 0,
      firstPaintTime: 0, firstPaintAfterLoadTime: 0, navigationType: 'Other',
      npnNegotiatedProtocol: 'h2', requestTime: 0, startLoadTime: 0,
      wasAlternateProtocolAvailable: false, wasFetchedViaSpdy: true, wasNpnNegotiated: true
    });
  }

  // 3. A Chrome install ships the PDF viewer's plugins; an automation shell reports none.
  if (navigator.plugins && navigator.plugins.length === 0) {
    const plugin = (name, filename, description) => {
      const entry = { name, filename, description, length: 1 };
      entry[0] = { type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format' };
      return entry;
    };
    const plugins = [
      plugin('PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
      plugin('Chrome PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
      plugin('Chromium PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
      plugin('Microsoft Edge PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
      plugin('WebKit built-in PDF', 'internal-pdf-viewer', 'Portable Document Format')
    ];
    const bag = (items) => {
      const value = { length: items.length, item: (index) => items[index] || null, namedItem: (name) => items.find((entry) => entry.name === name) || null };
      items.forEach((entry, index) => { value[index] = entry; });
      return value;
    };
    define(Navigator.prototype, 'plugins', bag(plugins));
    define(Navigator.prototype, 'mimeTypes', bag([plugins[0][0]]));
  }

  // 4. Under CDP, `permissions.query` answers "denied" for notifications even when nothing was
  //    asked; a person's Chrome answers "prompt". Only the notifications name is touched.
  if (navigator.permissions && navigator.permissions.query) {
    const query = navigator.permissions.query.bind(navigator.permissions);
    navigator.permissions.query = (parameters) =>
      parameters && parameters.name === 'notifications'
        ? query(parameters).then((status) => {
            if (status.state === 'denied' && Notification.permission === 'default') {
              Object.defineProperty(status, 'state', { get: () => 'prompt', configurable: true });
            }
            return status;
          })
        : query(parameters);
  }

  // 5. Headless/SwiftShader and virtualised GPUs name themselves. Keep a real GPU's wording.
  const gl = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function (type, ...rest) {
    const context = gl.call(this, type, ...rest);
    if (!context || (type !== 'webgl' && type !== 'webgl2') || !context.getParameter) return context;
    const vendor = String(context.getParameter(0x9240) || '');
    const renderer = String(context.getParameter(0x9241) || '');
    if (/swiftshader|llvmpipe|virtualbox|mesa/i.test(vendor + renderer)) {
      const parameter = context.getParameter.bind(context);
      context.getParameter = (constant) => {
        if (constant === 0x9240) return 'Google Inc. (Apple)';
        if (constant === 0x9241) return 'ANGLE (Apple, ANGLE Metal Renderer: Apple M3, Unspecified Version)';
        return parameter(constant);
      };
    }
    return context;
  };
})();
"#;

/// Approval for one exact origin. The fields are private so only a successful
/// request-bound trusted provider can construct this capability.
pub(super) struct ApprovedHiding {
    origin: String,
    grant: crate::consent::ProtectedGrant,
}

impl ApprovedHiding {
    pub(super) fn origin(&self) -> &str {
        &self.origin
    }

    pub(super) fn is_live(&self) -> bool {
        self.grant.is_live()
    }
}

impl Drop for ApprovedHiding {
    fn drop(&mut self) {
        self.grant.revoke();
    }
}

pub(super) async fn approve(
    engine: &super::engine::BrowserEngine,
    tab: &super::engine::ValidatedTab,
    public_session: &str,
    origin: String,
) -> Result<ApprovedHiding, String> {
    use super::types::EndpointAccessClass;
    use crate::authorization::{PermissionMode, RiskClass};

    if tab.record.endpoint_access_class != EndpointAccessClass::ExistingProfileApproved {
        return Err("automation hiding is supported only for an approved existing-profile tab".into());
    }
    let context = crate::tool::current_dispatch_authorization_context();
    if context.as_deref().is_some_and(|ctx| ctx.capability_manifest().is_some()) {
        return Err("automation hiding is unavailable for manifest-scoped sessions".into());
    }
    let mode = context.as_deref().map(|ctx| ctx.mode()).map(Ok)
        .unwrap_or_else(crate::authorization::configured_permission_mode)
        .map_err(|error| format!("permission mode is unavailable: {error}"))?;
    if mode == PermissionMode::Bounded {
        return Err("automation hiding is unavailable for bounded sessions".into());
    }
    let transport = tab.record.transport_session.as_deref().unwrap_or(public_session);
    let request = engine.approval_broker.request(
        mode,
        "browser_navigate.existing_profile_automation_hiding",
        RiskClass::R2,
        public_session.to_owned(),
        transport.to_owned(),
        serde_json::json!({
            "pid": tab.record.pid,
            "window_id": tab.record.window_id,
            "target": tab.tab.cdp_target_id,
            "origin": origin,
            "generation": tab.record.generation,
            "tab_generation": tab.tab.generation,
            "process_fingerprint": tab.record.fingerprint,
        }),
        format!(
            "Allow Cua to change page-visible browser properties on the top-level {origin} page in browser window {} for this navigation attempt? Changes already made remain until the page is replaced.",
            tab.record.window_id
        ),
    );
    let grant = engine.approval_broker.approve(&request).await
        .map_err(|error| format!("separate trusted approval was not granted: {error}"))?;
    Ok(ApprovedHiding { origin, grant })
}

/// Only the exact top-level HTTPS origin may execute the fixed script. JSON
/// encoding prevents the approved origin from becoming JavaScript source.
pub(super) fn script_for_origin(origin: &str) -> String {
    let scope = serde_json::to_string(origin).expect("serializing a string cannot fail");
    format!("if (self === self.top && location.origin === {scope}) {{\n{HIDE_AUTOMATION_SCRIPT}\n}}")
}
