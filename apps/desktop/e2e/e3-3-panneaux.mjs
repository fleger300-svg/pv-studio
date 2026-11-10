// E3.3 (building and roof panels) in the running app (French UI): every field is one
// undoable command; invalid entries are refused in the field; the roof type, pitches,
// half-hip, overhang and per-edge settings regenerate the roof; the roof step renames a
// face, sets its covering and adds an obstacle drawn on the plan.
//
//   node apps/desktop/e2e/e3-3-panneaux.mjs <output directory>
import path from 'node:path';
import { check, connect, summary, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);

await page.reload();
await page.getByRole('button', { name: /Résidentiel/ }).click();
await waitFor(
  page,
  () => hook(() => window.__pv?.project.getState().summary?.buildings.length),
  'preset',
);
const file = path.join(out, `e3-3-${Date.now()}.pvproj`);
await hook((p) => window.__pv.project.getState().newProject(p), file);
await waitFor(
  page,
  () => hook(() => window.__pv.project.getState().summary?.buildings.length === 0),
  'new',
);
await hook(() => window.__pv.plan.getState().setRatio(0.5));
await page.locator('.plan-view canvas').first().waitFor();
await page.waitForTimeout(400);

const box = await page.locator('.plan-view').boundingBox();
const screen = async ([e, n]) => {
  const t = await hook(() => window.__pv.plan.getState().transform);
  return [box.x + t.x + e * t.scale, box.y + t.y - n * t.scale];
};
const building = async () => (await summary(page)).buildings[0];
const faces = async () => (await summary(page)).roofFaces.map((f) => f.face);
const undoCount = async () => {
  let n = 0;
  while ((await hook(() => window.__pv.project.getState().counters)).canUndo && n < 40) {
    await page.keyboard.press('Control+z');
    await page.waitForTimeout(200);
    n += 1;
  }
  return n;
};
const fill = async (label, value) => {
  const input = page.getByLabel(label, { exact: true });
  await input.fill(value);
  await input.press('Enter');
};
let commands = 0;

// A 14 × 9 m rectangle.
await page.keyboard.press('r');
const [x0, y0] = await screen([-7, -4.5]);
const [x1, y1] = await screen([7, 4.5]);
await page.mouse.move(x0, y0);
await page.mouse.down();
await page.mouse.move(x1, y1, { steps: 10 });
await page.mouse.up();
await waitFor(page, async () => (await summary(page)).buildings.length === 1, 'rectangle');
commands += 1;
await page.keyboard.press('v');

// Name, eave height (decimal comma).
await fill('Nom', 'Maison');
await waitFor(page, async () => (await building()).name === 'Maison', 'name');
commands += 1;
await fill('Hauteur d’égout', '7,5');
await waitFor(page, async () => (await building()).eaveHeightM === 7.5, 'eave');
commands += 1;
check(true, 'name and eave height (« 7,5 ») committed');

// Hipped, then pitches.
await page.getByLabel('Type de toit').selectOption('hipped');
await waitFor(page, async () => (await building()).roofKind === 'hipped', 'hipped');
commands += 1;
check((await faces()).length === 4, 'hipped: four faces');
await fill('Pente', '35');
await waitFor(
  page,
  async () => (await faces()).some((f) => Math.abs(f.tiltDeg - 35) < 1e-9),
  'pitch',
);
commands += 1;
await fill('Pente des croupes', '50');
await waitFor(
  page,
  async () => (await faces()).some((f) => Math.abs(f.tiltDeg - 50) < 1e-9),
  'end pitch',
);
commands += 1;
check(true, 'pitch 35°, hipped ends 50°');

// Out of range: refused in the field, no command.
const before = await hook(() => window.__pv.project.getState().counters);
await fill('Pente', '95');
await page.getByText('Entre 1 et 75.').waitFor();
await page.keyboard.press('Escape');
const after = await hook(() => window.__pv.project.getState().counters);
check(
  JSON.stringify(before) === JSON.stringify(after),
  'pitch 95° refused in the field (no command)',
);

// Half-hip: ends start 1.5 m above the eave.
await page.getByLabel('Type de toit').selectOption('halfHipped');
// The 0.5 m default overhang lowers the outer edge of the ends by 0.5 · tan 50°.
const endEdge = 9 - 0.5 * Math.tan((50 * Math.PI) / 180);
await waitFor(
  page,
  async () => (await faces()).some((f) => Math.abs(f.origin[2] - endEdge) < 1e-6),
  'half-hip',
);
commands += 1;
check(true, 'half-hip: ends rise from 9 m (8.40 m at the overhang)');
await fill('Débord', '0,3');
await waitFor(page, async () => (await building()).roof?.overhangM === 0.3, 'overhang');
commands += 1;
await page.waitForTimeout(600);
await page.screenshot({ path: `${out}/e3-3-building.png` });

// Per-edge settings: edge 2 becomes a gable wall → custom roof.
await page.getByRole('button', { name: 'Réglages par arête' }).click();
await page.locator('.edge-table__row').nth(1).getByLabel('Pignon').check();
await waitFor(page, async () => (await building()).roof?.edges[1].pitchDeg === 90, 'gable edge');
commands += 1;
await waitFor(
  page,
  async () => (await page.getByLabel('Type de toit').inputValue()) === 'custom',
  'custom form',
);
check(true, 'per-edge change → custom roof');

// Roof step: rename a face, covering, obstacle.
await page.getByRole('button', { name: /Toiture/ }).click();
await page.locator('.face-list__item').first().click();
await fill('Nom du pan', 'Sud');
await waitFor(page, async () => (await faces()).some((f) => f.name === 'Sud'), 'rename');
commands += 1;
await page.getByLabel('Couverture').selectOption('standingSeam');
await waitFor(
  page,
  async () => (await faces()).some((f) => f.supportGrid?.kind === 'standingSeam'),
  'covering',
);
commands += 1;
await fill('Entraxe', '0,6');
await waitFor(
  page,
  async () => (await faces()).some((f) => f.supportGrid?.spacingM === 0.6),
  'spacing',
);
commands += 1;
await page.getByRole('button', { name: 'Ajouter un obstacle' }).click();
await waitFor(
  page,
  async () => (await faces()).some((f) => (f.outline?.obstacles.length ?? 0) === 1),
  'obstacle',
);
commands += 1;
await fill('Hauteur', '1,5');
await waitFor(
  page,
  async () => (await faces()).some((f) => f.outline?.obstacles[0]?.heightM === 1.5),
  'obstacle height',
);
commands += 1;
check(true, 'face renamed, standing seam 0,6 m, chimney 1,5 m high');
await page.waitForTimeout(600);
await page.screenshot({ path: `${out}/e3-3-roof.png` });

// Every field was one command: undo them all.
const undone = await undoCount();
check(
  undone === commands && (await summary(page)).buildings.length === 0,
  `${undone} commands undone (${commands} expected)`,
);

await browser.close();
console.log('E3.3 scenario passed');
