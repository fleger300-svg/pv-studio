// M10 (PV*SOL import) in the running app (French UI): a PV*SOL project is imported into a new
// .pvproj, opened, and the import report shows what was left out and the comparison with
// PV*SOL's results. The .pvprj is a client file: pass its path (it is read, never copied).
//
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"; pnpm tauri dev
//   node apps/desktop/e2e/m10-import-pvsol.mjs <output directory> <project.pvprj>
import path from 'node:path';
import { check, connect, summary, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const source = process.argv[3];
if (!source) throw new Error('usage: m10-import-pvsol.mjs <output directory> <project.pvprj>');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);

await page.reload();
await page.getByRole('button', { name: 'Importer un projet PV*SOL…' }).waitFor();
// The store hook appears with the viewport: open a preset first.
await page.getByRole('button', { name: /Résidentiel/ }).click();
await waitFor(page, () => hook(() => window.__pv?.project.getState().summary), 'preset');

const target = path.join(out, `m10-${Date.now()}.pvproj`);
await hook(([s, t]) => window.__pv.project.getState().importPvsol(s, t), [source, target]);
const report = page.getByRole('dialog', { name: 'Import PV*SOL' });
await report.waitFor({ timeout: 120000 });
const s = await summary(page);
check(s.path === target, `project saved as ${path.basename(target)}`);
const modules = s.arrays.reduce((n, a) => n + a.moduleCount, 0);
check(
  s.buildings.length >= 1 && s.arrays.length >= 1,
  `${s.buildings.length} building(s), ${s.arrays.length} arrays`,
);
check(modules > 0, `${modules} modules imported`);
const error = await hook(() => window.__pv.project.getState().error);
check(error === null, 'no error');

const rows = await report.locator('tbody tr').count();
check(rows >= 10, `comparison table with ${rows} rows`);
const energy = await report.getByRole('row', { name: /Énergie AC annuelle/ }).innerText();
check(/\d/.test(energy), `annual energy row: ${energy.replace(/\s+/g, ' ')}`);
await page.screenshot({ path: path.join(out, 'm10-report.png') });

await report.getByRole('button', { name: 'Fermer' }).click();
await waitFor(page, async () => (await report.count()) === 0, 'report closed');
await page.screenshot({ path: path.join(out, 'm10-project.png') });
check(true, 'report closed, imported project shown');
await browser.close();
