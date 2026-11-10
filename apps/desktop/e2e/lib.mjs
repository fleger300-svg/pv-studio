// Shared helpers of the end-to-end drivers. They drive the real PV Studio (Tauri dev or a
// release build) through the WebView2 debugging port; they are run by hand, not in CI:
//
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"; pnpm tauri dev
//   node apps/desktop/e2e/<scenario>.mjs <output directory>
//
// Native dialogs cannot be driven: scenarios call the commands with an explicit path.
import { chromium } from 'playwright-core';

export async function connect() {
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
  const page = browser.contexts()[0].pages()[0];
  await page.setViewportSize({ width: 1440, height: 900 }).catch(() => {});
  return { browser, page };
}

/** Polls `check` (async, returns a value or a falsy one) until it returns something. */
export async function waitFor(page, check, what, timeoutMs = 60000) {
  const started = Date.now();
  for (;;) {
    const v = await check();
    if (v) return v;
    if (Date.now() - started > timeoutMs) throw new Error(`timed out waiting for ${what}`);
    await page.waitForTimeout(100);
  }
}

/** Calls a Tauri command from the page (as the front-end does). */
export function invoke(page, cmd, args = {}) {
  return page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a), [cmd, args]);
}

export function summary(page) {
  return invoke(page, 'get_project_summary');
}

/** Fails the scenario with a message. */
export function check(ok, message) {
  if (!ok) throw new Error(`check failed: ${message}`);
  console.log(`ok - ${message}`);
}
