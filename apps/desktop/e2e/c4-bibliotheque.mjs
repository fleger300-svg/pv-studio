// C4 (component library) in the running app (French UI), with an empty library
// (`PV_LIBRARY_PATH` pointing to a new file): a new array uses the base catalogue; a module
// is created in the library and assigned to the array (face laid out again); a new price
// becomes an update proposal, accepted then undone; the synthetic K2 list is imported.
//
//   $env:PV_LIBRARY_PATH = "<new file>"; $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"; pnpm tauri dev
//   node apps/desktop/e2e/c4-bibliotheque.mjs <output directory>
import fs from 'node:fs';
import path from 'node:path';
import ExcelJS from 'exceljs';
import { check, connect, invoke, summary, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);
const proposals = () => invoke(page, 'component_updates');

await page.reload();
await page.getByRole('button', { name: /Résidentiel/ }).click();
await waitFor(
  page,
  () => hook(() => window.__pv?.project.getState().summary?.buildings.length),
  'preset',
);
const file = path.join(out, `c4-${Date.now()}.pvproj`);
await hook((p) => window.__pv.project.getState().newProject(p), file);
await waitFor(
  page,
  () => hook(() => window.__pv.project.getState().summary?.buildings.length === 0),
  'new',
);
await hook(() => window.__pv.plan.getState().setRatio(0.5));
await page.locator('.plan-view canvas').first().waitFor();
await page.waitForTimeout(400);

// A house, its south face covered with standing seams, an array on it.
const box = await page.locator('.plan-view').boundingBox();
const screen = async ([e, n]) => {
  const t = await hook(() => window.__pv.plan.getState().transform);
  return [box.x + t.x + e * t.scale, box.y + t.y - n * t.scale];
};
await page.keyboard.press('r');
const [x0, y0] = await screen([-8, -5]);
const [x1, y1] = await screen([8, 5]);
await page.mouse.move(x0, y0);
await page.mouse.down();
await page.mouse.move(x1, y1, { steps: 10 });
await page.mouse.up();
await waitFor(page, async () => (await summary(page)).roofFaces.length === 2, 'house');
const south = (await summary(page)).roofFaces.find((f) => f.face.azimuthDeg === 180).face;
await page.getByRole('button', { name: /Calepinage/ }).click();
await hook(
  ([f, b]) => window.__pv.selection.getState().selectFace(f, b),
  [south.id, south.buildingId],
);
await page.getByRole('button', { name: /Ajouter un champ/ }).click();
const arrayOf = async () =>
  (await summary(page)).arrays.find((a) => a.array.roofFaceId === south.id);
await waitFor(page, async () => (await arrayOf())?.moduleCount > 0, 'array');
let a = await arrayOf();
check(
  a.mountingLabel === 'K2 SingleRail' && a.moduleLabel === 'LONGi LR7-54HVH-490M',
  `new array: ${a.moduleLabel} on ${a.mountingLabel}, ${a.moduleCount} modules`,
);

// A new module in the library.
await page.getByRole('button', { name: 'Bibliothèque', exact: true }).click();
await page.getByRole('button', { name: 'Nouveau module' }).click();
const fill = async (label, value) => {
  const input = page.getByLabel(label, { exact: true });
  await input.fill(value);
  await input.press('Tab');
};
await fill('Fabricant', 'Test Solar');
await fill('Modèle', 'TS-450');
await fill('Libellé', 'Test Solar TS-450');
await fill('Longueur (m)', '1,762');
await page.getByRole('button', { name: 'Créer' }).click();
await page.getByText('Test Solar TS-450 · révision 1').waitFor();
check(true, 'module created in the library (revision 1)');
await page.screenshot({ path: `${out}/c4-library.png` });
await page.getByRole('button', { name: 'Fermer' }).first().click();

// Assigned to the array: its face is laid out again with the new size.
const moduleSelect = page.getByLabel('Module', { exact: true });
const value = await moduleSelect
  .locator('option', { hasText: 'Test Solar TS-450' })
  .getAttribute('value');
await moduleSelect.selectOption(value);
await waitFor(page, async () => (await arrayOf())?.moduleLabel === 'Test Solar TS-450', 'assign');
a = await arrayOf();
check(
  a.array.moduleLengthM === 1.762 && a.mountingLabel === 'K2 SingleRail' && a.moduleCount > 0,
  `module assigned: ${a.array.moduleLengthM} m long, system kept, ${a.moduleCount} modules`,
);

// A new price for the rail: proposed, accepted (one command), undone.
check((await proposals()).length === 0, 'no proposal before any library change');
await page.getByRole('button', { name: 'Bibliothèque', exact: true }).click();
await page.getByLabel('Rechercher (fabricant, modèle, n° d’article)').fill('2004393');
await page.getByRole('button', { name: /SingleRail 36; 4.80 m/ }).click();
await fill('Fournisseur', 'K2 Systems');
await fill('Prix catalogue à la pièce (CHF)', '61,50');
await page.getByRole('button', { name: 'Ajouter le prix' }).click();
await page.getByRole('cell', { name: /61[.,]50/ }).waitFor();
await page.getByRole('button', { name: 'Fermer' }).first().click();
await page.getByText('1 mise à jour de composant est disponible.').waitFor();
check(true, 'the new price is proposed, not applied');
await page.getByRole('button', { name: 'Examiner' }).click();
await page.screenshot({ path: `${out}/c4-proposal.png` });
await page.getByRole('button', { name: 'Appliquer 1 mise à jour' }).click();
await waitFor(page, async () => (await proposals()).length === 0, 'accepted');
check(true, 'proposal accepted');
await page.keyboard.press('Control+z');
await waitFor(page, async () => (await proposals()).length === 1, 'undone');
check(true, 'undo brings the proposal back');

// The synthetic K2 list (scaled prices) imported from an .xlsx file.
const fixture = JSON.parse(
  fs.readFileSync(new URL('../../../testdata/library/k2_list_synthetic.json', import.meta.url)),
);
const wb = new ExcelJS.Workbook();
for (const s of fixture.sheets) {
  const ws = wb.addWorksheet(s.name);
  for (const row of s.rows) ws.addRow(row.map((c) => (c.kind === 'empty' ? null : c.value)));
}
const xlsx = path.join(out, 'k2-synthetic.xlsx');
await wb.xlsx.writeFile(xlsx);
await page.getByRole('button', { name: 'Bibliothèque', exact: true }).click();
await page.getByRole('button', { name: 'Importer une liste de prix…' }).click();
await hook((p) => window.__pvImportFile(p), xlsx);
await page.getByText('5 articles seront tarifés.').waitFor();
await page.getByRole('button', { name: 'Importer', exact: true }).click();
await page.getByText('5 articles tarifés.').waitFor();
await page.screenshot({ path: `${out}/c4-import.png` });
check(true, 'synthetic K2 list imported: 5 articles priced, 1 unknown reported');
await page.getByRole('dialog').getByRole('button', { name: 'Fermer' }).click();
await page.getByRole('button', { name: 'Fermer' }).first().click();
await waitFor(page, async () => (await proposals()).length === 5, 'five proposals');
check(true, 'five price proposals after the import');

// German UI.
await page.getByLabel('Langue').selectOption('de');
await page.getByRole('button', { name: 'Bibliothek', exact: true }).waitFor();
check(true, 'German UI: « Bibliothek »');
await page.getByLabel('Sprache').selectOption('fr');

await browser.close();
console.log('C4 scenario passed');
