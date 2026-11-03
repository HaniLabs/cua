# Not announcing the automation

A page decides whether it is talking to a person or to a script from a handful of expressions it
can read in a few milliseconds. `navigator.webdriver` is the one with a name and the one sign-in
walls act on: with it set, X answers "This browser or app may not be secure" and never looks at
the password. The rest is corroboration — `window.chrome`, an empty plugin list, a GPU that calls
itself SwiftShader, `permissions.query` answering "denied" for a permission nobody asked for.

The driver answers all of it the way a person's Chrome does.

## Two halves, two layers

**A browser the driver launches is launched clean.** `isolated_browser_command` passes
`--disable-blink-features=AutomationControlled`, so Chromium never sets `navigator.webdriver` and
never shows the "controlled by automated software" infobar — for every document, every tab, and
every tab the page opens itself. It also passes a few flags that keep a fresh profile from
behaving like a fresh profile (`--no-service-autorun`, `--hide-crash-restore-bubble`,
`--disable-search-engine-choice-screen`).

**A browser that is already running cannot be relaunched with a flag**, so `attach` registers a
document-start script on the tab's own CDP session (`browser::stealth`). It deletes the flag,
defines a `false` getter so a re-read stays false, restores `window.chrome.runtime`/`csi`/
`loadTimes`, synthesises the PDF-viewer plugins a real Chrome ships, repairs the notifications
permission answer, and renames a software/virtualised GPU the way a real one names itself.

Three things here are load-bearing and easy to get wrong:

* **`Page.enable` first.** Without it the registration is acknowledged and never fires.
* **The getter lives on `Navigator.prototype`.** Deleting the instance property alone leaves
  `navigator.webdriver` reading `true`.
* **Deleting once is not enough.** Chromium re-defines the property when a document commits its
  automation state — after document-start scripts have run — so the script re-applies on an
  interval. A single delete is undone a moment later.

Every patch is guarded: it only fires when the tell is actually present, so a genuine headful
Chrome keeps its own values. Installation is best effort — an engine without a `Page` domain (an
embedded webview) is not an error, and a tab that refused the script is still a drivable tab.
`CUA_DRIVER_NO_STEALTH=1` turns the attach-time half off for callers that want to observe the raw
signal.

## The reviewed existing-profile CDP surface

`Page.enable` was already allowed. `Page.addScriptToEvaluateOnNewDocument` joins it: both are
read-shaped and register behaviour in one tab without changing state a later command depends on.
`approved_existing_profile_tools_stay_within_the_reviewed_cdp_surface` pins the rest of the
surface, `an_attached_tab_is_given_the_automation_hiding_script` pins that the script is registered
on the tab's own session before its first navigation, and
`a_launched_browser_does_not_announce_the_automation` pins the launch flag.

## Measured

A probe page reading the tells, driven over CDP:

| | `navigator.webdriver` | `window.chrome.runtime` |
| :--- | :--- | :--- |
| Chrome launched by the driver | `false` (flag) | — |
| Chrome launched with `--enable-automation` | `true` | `undefined` |
| … after the driver attaches to it | `false` | `object` (script) |

## What this does not do

* It does not spoof the user agent, client hints, timezone, screen metrics or canvas noise. The
  browser is a real Chrome reporting real values; a tell is only repaired where automation, not
  the machine, produced it.
* It does not hide the tab from a fingerprint that looks at protocol artefacts instead of the
  DOM.
* It does not pass `--enable-automation` (never did) and does not touch the profile's own state.
