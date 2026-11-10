// E3.4 (layout) in the running app (French UI): from an empty site to a laid-out roof —
// draw a house, add an array on its south face (laid out at once), change settings (each
// re-lays the face out as one command), clear and lay out again, undo everything; then a
// setting changed on the residential stress project, with the statics recomputed.
//
// The roof face is selected through the dev hook (a click in the 3D view would need the
// camera's projection; the 3D click itself is covered by the E3.1 scenario).
//
//   node apps/desktop/e2e/e3-4-calepinage.mjs <output directory>
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
const file = path.join(out, `e3-4-${Date.now()}.pvproj`);
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
const arraysOn = async (faceId) =>
  (await summary(page)).arrays.filter((a) => a.array.roofFaceId === faceId);
const counters = () => hook(() => window.__pv.project.getState().counters);
const fill = async (label, value) => {
  const input = page.getByLabel(label, { exact: true });
  await input.fill(value);
  await input.press('Enter');
};
const timed = async (what, action, until) => {
  const t0 = Date.now();
  await action();
  await waitFor(page, until, what);
  return Date.now() - t0;
};

// A 16 × 10 m house (gable, 30°): its south face.
await page.keyboard.press('r');
const [x0, y0] = await screen([-8, -5]);
const [x1, y1] = await screen([8, 5]);
await page.mouse.move(x0, y0);
await page.mouse.down();
await page.mouse.move(x1, y1, { steps: 10 });
await page.mouse.up();
await waitFor(page, async () => (await summary(page)).roofFaces.length === 2, 'house');
const south = (await summary(page)).roofFaces.find((f) => f.face.azimuthDeg === 180).face;

// Layout step, south face selected.
await page.getByRole('button', { name: /Calepinage/ }).click();
await hook(
  ([f, b]) => window.__pv.selection.getState().selectFace(f, b),
  [south.id, south.buildingId],
);
await page.getByText('Champs · South').waitFor();

// Add an array: laid out at once (one command).
let commands = 1;
const addMs = await timed(
  'array',
  () => page.getByRole('button', { name: /Ajouter un champ/ }).click(),
  async () => (await arraysOn(south.id))[0]?.moduleCount > 0,
);
commands += 1;
const first = (await arraysOn(south.id))[0];
check(
  first.moduleCount > 0,
  `new array laid out: ${first.moduleCount} modules (${addMs} ms click → model)`,
);
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/e3-4-array.png` });

// Wider setbacks: fewer modules.
await fill('Retrait des bords', '0,8');
await waitFor(
  page,
  async () => (await arraysOn(south.id))[0].array.layout.edgeSetbackM === 0.8,
  'setback',
);
commands += 1;
const narrower = (await arraysOn(south.id))[0].moduleCount;
check(narrower < first.moduleCount, `setback 0,8 m: ${first.moduleCount} → ${narrower} modules`);

// Landscape: laid out again.
await page.getByLabel('Orientation des modules').selectOption('landscape');
await waitFor(
  page,
  async () => (await arraysOn(south.id))[0].array.orientation === 'landscape',
  'landscape',
);
commands += 1;
const landscape = (await arraysOn(south.id))[0].moduleCount;
check(landscape > 0, `landscape: ${landscape} modules`);

// Clear, then lay out again.
await page.getByRole('button', { name: 'Vider le champ' }).click();
await waitFor(page, async () => (await arraysOn(south.id))[0].moduleCount === 0, 'clear');
commands += 1;
await page.getByRole('button', { name: 'Calepiner automatiquement' }).click();
await waitFor(
  page,
  async () => (await arraysOn(south.id))[0].moduleCount === landscape,
  'relayout',
);
commands += 1;
check(true, 'cleared, then laid out again identically');
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/e3-4-landscape.png` });

// Statics of the new array: on a fresh site they first ask for the face's covering (the
// fixings depend on it)…
const statics = page.locator('.pv-panel', { hasText: 'Statique' });
await statics.getByText('La couverture du pan').waitFor({ timeout: 30000 });
check(true, 'statics: covering of the face asked for');
// … set at the roof step (standing seam), then for the building's site data.
await page.getByRole('button', { name: /Toiture/ }).click();
await page.getByLabel('Couverture').selectOption('standingSeam');
await waitFor(
  page,
  async () => (await summary(page)).roofFaces.some((f) => f.face.supportGrid),
  'covering',
);
commands += 1;
await page.getByRole('button', { name: /Calepinage/ }).click();
await statics.getByText('Les données de site du bâtiment').waitFor({ timeout: 30000 });
check(true, 'statics: then the site data (wind, snow, terrain) asked for');

// Every action was one command: undo them all, back to the empty site.
let undone = 0;
while ((await counters()).canUndo && undone < 30) {
  await page.keyboard.press('Control+z');
  await page.waitForTimeout(250);
  undone += 1;
}
check(
  undone === commands && (await summary(page)).buildings.length === 0,
  `${undone} commands undone (${commands} expected)`,
);

// Residential stress project: a setting changed on a populated face (the face's array is
// laid out again, the statics follow).
await page.getByRole('button', { name: /Résidentiel/ }).click();
await waitFor(page, async () => (await summary(page)).arrays.length === 6, 'preset');
const s = await summary(page);
const face = s.roofFaces[0].face;
await hook(
  ([f, b]) => window.__pv.selection.getState().selectFace(f, b),
  [face.id, face.buildingId],
);
const before = (await arraysOn(face.id))[0].moduleCount;
const ms = await timed(
  'residential relayout',
  () => fill('Retrait des bords', '0,5'),
  async () => (await arraysOn(face.id))[0]?.array.layout.edgeSetbackM === 0.5,
);
const after = (await arraysOn(face.id))[0].moduleCount;
check(
  after > 0 && after < before,
  `residential face: ${before} → ${after} modules in ${ms} ms (Enter → reloaded model)`,
);
await page.waitForTimeout(1500);
await page.screenshot({ path: `${out}/e3-4-residential.png` });

await browser.close();
console.log('E3.4 scenario passed');
