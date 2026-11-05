# Experimental browser automation hiding

The fork includes an experimental browser compatibility change. Its effects are limited and do not guarantee that a website will accept an automated session. Respect the website's access controls; do not use this mode to solve challenges or bypass rate limits.

A driver-launched isolated Chromium browser uses the launch flag `--disable-blink-features=AutomationControlled` and the other launch options described by the code. This affects the launched process and is separate from existing-profile attachment.

## Existing profiles

Ordinary existing-profile attachment and navigation do **not** install the experimental script. For a single navigation attempt, a caller may set `automation_hiding: true` on `browser_navigate` for an exact, already bound tab and HTTPS destination. A separate trusted authorization provider must approve the exact browser, tab, and origin and disclose that page-visible properties will change. A normal existing-profile attachment grant is not approval for this extra behavior. Without a provider or the additional approval, the call refuses. Bounded sessions and any session carrying a capability manifest refuse this opt-in.

The fixed script runs only in the top-level frame at the approved HTTPS origin. It can alter `navigator.webdriver`, `window.chrome`, plugin and MIME-type values, notification permission results, and WebGL properties. Some values are synthesized; they are not claims about the actual hardware or browser. Cross-origin redirects and subframes do not run the script. A same-origin reload or navigation while the registration is active can also run it.

The driver removes the registration and detaches the dedicated CDP session after an observed load or a bounded timeout, and closes the shared browser connection if detachment fails. The script's repeated `navigator.webdriver` update stops after 15 seconds. Removing a registration cannot undo changes already applied to a loaded document; those changes can remain until that document is replaced. A canceled or failed navigation can still have taken effect, and cleanup errors are reported rather than represented as success.

The generic existing-profile CDP allowlist continues to refuse document-start script registration. The opt-in uses a separate internal fixed-script path; it does not allow callers to submit arbitrary CDP methods or JavaScript. No platform-wide acceptance, anti-detection guarantee, or upstream approval is implied.
