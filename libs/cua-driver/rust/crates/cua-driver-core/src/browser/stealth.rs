//! Making a driven Chrome look like somebody's Chrome.
//!
//! A page gets two questions answered by the browser itself before it decides whether to serve a
//! person or a robot: *is this under automation* (`navigator.webdriver`, the infobar) and *what
//! kind of browser is this* (plugins, `window.chrome`, WebGL, permissions). Sign-in walls act on
//! the answer — X says "This browser or app may not be secure" and never looks at the password.
//!
//! Half of the answer is fixed at launch and cannot be fixed afterwards, so
//! [`super::prepare`] passes `--disable-blink-features=AutomationControlled`: Chromium then never
//! sets `navigator.webdriver` nor shows the "controlled by automated software" infobar, for every
//! document and every tab the page opens.
//!
//! The other half is repaired here, at attach time, for browsers that were already running and
//! cannot be relaunched with a flag. Every patch is narrow and only fires when the automation tell
//! is actually present, so a genuine headful Chrome keeps its own values.

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
  setInterval(hideWebdriver, 30);

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

/// Whether the attach-time half is wanted. Callers that want to observe the tell itself set
/// `CUA_DRIVER_NO_STEALTH`.
pub fn script_enabled() -> bool {
    enabled_for(std::env::var("CUA_DRIVER_NO_STEALTH").ok().as_deref())
}

/// The switch, split out so it can be tested without touching the process environment.
fn enabled_for(value: Option<&str>) -> bool {
    !matches!(
        value
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Collapse a CDP error into one log line.
fn failure_detail(detail: &str) -> String {
    detail
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect()
}

/// Install the document-start script on one tab session.
///
/// `Page.enable` goes first: without it the registration is acknowledged and never runs.
pub async fn install(
    connection: &super::cdp_ws::CdpConnection,
    session: &str,
) -> Result<(), String> {
    if !script_enabled() {
        return Ok(());
    }
    connection
        .call(Some(session), "Page.enable", serde_json::json!({}))
        .await
        .map_err(|error| failure_detail(&format!("Page.enable: {error}")))?;
    let params = serde_json::json!({ "source": HIDE_AUTOMATION_SCRIPT });
    connection
        .call(
            Some(session),
            "Page.addScriptToEvaluateOnNewDocument",
            params,
        )
        .await
        .map_err(|error| {
            failure_detail(&format!("Page.addScriptToEvaluateOnNewDocument: {error}"))
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_is_opt_out() {
        assert!(enabled_for(None));
        assert!(enabled_for(Some("")));
        assert!(enabled_for(Some("0")));
        for off in ["1", "true", "TRUE", " yes ", "on", "On"] {
            assert!(!enabled_for(Some(off)), "{off}");
        }
    }

    #[test]
    fn the_script_hides_the_tell_the_wall_reads() {
        assert!(HIDE_AUTOMATION_SCRIPT.contains("Navigator.prototype, 'webdriver'"));
        // Chromium re-defines the property when a document commits its automation state, so a
        // one-shot delete is not enough.
        assert!(HIDE_AUTOMATION_SCRIPT.contains("setInterval(hideWebdriver"));
        assert!(HIDE_AUTOMATION_SCRIPT.contains("window.chrome"));
    }
}
