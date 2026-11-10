// In-app viewport benchmark (HUD → « Lancer le benchmark ») on both stress presets, against
// a RELEASE build (WebView2 with --remote-debugging-port=9222): open, orbit, drag, move,
// undo / redo, statics recompute. Prints the paths of the reports the app writes.
//
//   node apps/desktop/e2e/bench-viewport.mjs <output directory>
import path from 'node:path';
import { connect } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const { browser, page } = await connect();
await page.waitForSelector('.app');
const paths = [];
for (const name of [/Résidentiel|Residential/, /Industriel|Industrial/]) {
  await page.getByRole('button', { name }).click();
  await page.waitForSelector('canvas', { timeout: 60_000 });
  await page.waitForTimeout(1500);
  // The benchmark drives the module tools: layout step, full 3D view.
  await page.getByRole('button', { name: /Calepinage|Layout/ }).click();
  if (!(await page.locator('.hud').count())) await page.keyboard.press('F2');
  await page.getByRole('button', { name: /Lancer le benchmark|Run benchmark/ }).click();
  const done = page.locator('.hud').getByText(/Rapport écrit|Report written/);
  await done.waitFor({ timeout: 120_000 });
  paths.push(await done.innerText());
  await page.screenshot({ path: `${out}/bench-${paths.length}.png` });
}
console.log(paths.join('\n'));
await browser.close();
