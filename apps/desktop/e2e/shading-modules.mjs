// Shading per module in the running app (French UI): a PV*SOL project is imported, the 3-D
// view is switched to the shading colour mode, the legend shows the project's means and the
// module under the pointer shows its figures.
//
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"; pnpm tauri dev
//   node apps/desktop/e2e/shading-modules.mjs <output directory> <project.pvprj>
import path from 'node:path';
import { check, connect, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const source = process.argv[3];
if (!source) throw new Error('usage: shading-modules.mjs <output directory> <project.pvprj>');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);

await page.reload();
await page.getByRole('button', { name: 'Importer un projet PV*SOL…' }).waitFor();
await page.getByRole('button', { name: /Résidentiel/ }).click();
await waitFor(page, () => hook(() => window.__pv?.project.getState().summary), 'preset');

const target = path.join(out, `shading-${Date.now()}.pvproj`);
await hook(([s, t]) => window.__pv.project.getState().importPvsol(s, t), [source, target]);
const report = page.getByRole('dialog', { name: 'Import PV*SOL' });
await report.waitFor({ timeout: 120000 });
await report.getByRole('button', { name: 'Fermer' }).click();
await waitFor(page, async () => (await report.count()) === 0, 'report closed');

// The layout step shows the display options.
await page.getByRole('button', { name: /Calepinage/ }).first().click();
await page.getByLabel('Mode de couleur').selectOption('shading');
const legend = page.getByRole('group', { name: 'Ombrage annuel par module' });
await legend.waitFor();
const mean = await waitFor(
  page,
  async () => {
    const text = await legend.innerText();
    return /Moyenne/.test(text) ? text : null;
  },
  'shading computed',
  120000,
);
check(true, `legend: ${mean.replace(/\s+/g, ' ')}`);
await page.screenshot({ path: path.join(out, 'shading-legend.png') });

// Hover the modules: find a point over one (the module grid of the viewport).
const canvas = page.locator('.app__3d canvas');
const box = await canvas.boundingBox();
let tip = null;
for (let y = 0.3; y <= 0.7 && !tip; y += 0.05) {
  for (let x = 0.3; x <= 0.7 && !tip; x += 0.05) {
    await page.mouse.move(box.x + box.width * x, box.y + box.height * y);
    await page.waitForTimeout(60);
    if ((await page.locator('.shading-tip').count()) > 0) {
      tip = await page.locator('.shading-tip').innerText();
    }
  }
}
check(tip !== null && /Horizon/.test(tip), `tooltip: ${String(tip).replace(/\s+/g, ' ')}`);
await page.screenshot({ path: path.join(out, 'shading-hover.png') });
await browser.close();
