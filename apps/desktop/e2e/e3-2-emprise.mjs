// E3.2 (footprint editing) in the running app, with the real mouse and keyboard on the plan:
// rectangle tool, polygon tool (an L: hipped by default), vertex drag with the live Rust
// preview, a vertex added from the « + » of an edge, whole-building move (faces and their
// ids kept), a typed dimension, vertex deletion, then undo / redo of every gesture.
//
//   node apps/desktop/e2e/e3-2-emprise.mjs <output directory>
import fs from 'node:fs';
import path from 'node:path';
import { check, connect, summary, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);

await page.reload();
await page.getByRole('button', { name: /Nouveau projet|New project/ }).waitFor();
// A fresh file per run (the previous one may still be open in the app).
const file = path.join(out, `e3-2-${Date.now()}.pvproj`);
for (const f of [file, `${file}-wal`, `${file}-shm`]) fs.rmSync(f, { force: true });
// The dev hook appears with the viewport: open a preset first, then the new project.
await page.getByRole('button', { name: /Résidentiel|Residential/ }).click();
await waitFor(
  page,
  () => hook(() => window.__pv?.project.getState().summary?.buildings.length),
  'preset',
);
await hook((p) => window.__pv.project.getState().newProject(p), file);
await waitFor(
  page,
  () => hook(() => window.__pv.project.getState().summary?.buildings.length === 0),
  'new',
);
await page.locator('.plan-view canvas').first().waitFor();
// Deterministic layout: the splitter is remembered between runs.
await hook(() => window.__pv.plan.getState().setRatio(0.5));
await page.waitForTimeout(500);

const box = await page.locator('.plan-view').boundingBox();
const screen = async ([e, n]) => {
  const t = await hook(() => window.__pv.plan.getState().transform);
  return [box.x + t.x + e * t.scale, box.y + t.y - n * t.scale];
};
const drag = async (from, to, steps = 12) => {
  const [x0, y0] = await screen(from);
  const [x1, y1] = await screen(to);
  await page.mouse.move(x0, y0);
  await page.mouse.down();
  await page.mouse.move(x1, y1, { steps });
  await page.mouse.up();
};
const click = async (p) => {
  const [x, y] = await screen(p);
  await page.mouse.click(x, y);
};
const buildings = async () => (await summary(page)).buildings;
const facesOf = async (id) =>
  (await summary(page)).roofFaces.filter((f) => f.face.buildingId === id).map((f) => f.face.id);
const world = (b) => b.footprint.map(([e, n]) => [b.origin[0] + e, b.origin[1] + n]);
const history = () => hook(() => window.__pv.project.getState().counters);

// 1. Rectangle (R): 12 × 8 m, gable roof along its long side, selected.
await page.keyboard.press('r');
await drag([-6, -4], [6, 4]);
await waitFor(page, async () => (await buildings()).length === 1, 'rectangle');
let [house] = await buildings();
check(
  house.roofKind === 'gable' && house.sizeM[0] === 12 && house.sizeM[1] === 8,
  'rectangle → 12 × 8 m gable',
);
check((await facesOf(house.id)).length === 2, 'two roof faces');
check(
  await hook((id) => window.__pv.selection.getState().buildingId === id, house.id),
  'new building selected',
);
await page.waitForTimeout(600);
await page.screenshot({ path: `${out}/e3-2-rect.png` });

// 2. Polygon (P): an L, closed on its first vertex → hipped by default (a gable wall cannot
// stand at its re-entrant corner).
await page.keyboard.press('p');
for (const p of [
  [-19, -18],
  [-7, -18],
  [-7, -12],
  [-13, -12],
  [-13, -6],
  [-19, -6],
  [-19, -18],
]) {
  await click(p);
}
await waitFor(page, async () => (await buildings()).length === 2, 'L');
const l = (await buildings()).find((b) => b.id !== house.id);
check(l.roofKind === 'hipped' && l.footprint.length === 6, 'L → hipped roof');
check((await facesOf(l.id)).length === 6, 'L: six faces (hips and a valley)');

// 3. Edit tool (V): select the rectangle, drag a corner; the roof is previewed by Rust
// during the drag (screenshot mid-drag), one command on release, faces kept.
await page.keyboard.press('v');
await click([0, 0]);
await waitFor(
  page,
  () => hook((id) => window.__pv.selection.getState().buildingId === id, house.id),
  'select',
);
const faces0 = await facesOf(house.id);
const [cx, cy] = await screen([6, 4]);
const [tx, ty] = await screen([8, 6]);
await page.mouse.move(cx, cy);
await page.mouse.down();
await page.mouse.move((cx + tx) / 2, (cy + ty) / 2, { steps: 6 });
await page.mouse.move(tx, ty, { steps: 6 });
await page.waitForTimeout(300);
await page.screenshot({ path: `${out}/e3-2-drag-preview.png` });
const ipcDuring = await hook(() => window.__pv.ipcCallCount());
await page.mouse.up();
await waitFor(
  page,
  async () =>
    world((await buildings()).find((b) => b.id === house.id)).some(([e, n]) => e === 8 && n === 6),
  'vertex drag',
);
check(JSON.stringify(await facesOf(house.id)) === JSON.stringify(faces0), 'faces kept (same ids)');
check(ipcDuring > 0, 'previews requested during the drag');

// 4. The « + » of the bottom edge: a new vertex dragged down.
house = (await buildings()).find((b) => b.id === house.id);
const before = house.footprint.length;
await drag([0, -4], [0, -7]);
await waitFor(
  page,
  async () => (await buildings()).find((b) => b.id === house.id).footprint.length === before + 1,
  'insert',
);
check(true, 'vertex added from the middle of an edge');

// 5. Move the whole building 10 m north: same faces.
house = (await buildings()).find((b) => b.id === house.id);
const faces1 = await facesOf(house.id);
await drag([0, 0], [0, 10]);
await waitFor(
  page,
  async () => (await buildings()).find((b) => b.id === house.id).origin[1] === house.origin[1] + 10,
  'move',
);
check(
  JSON.stringify(await facesOf(house.id)) === JSON.stringify(faces1),
  'moved building keeps its faces',
);

// 6. Typed dimension: fit the view, double-click the label of the longest edge, type 15.
await page.getByRole('button', { name: /Ajuster à la vue|Fit to view/ }).click();
await page.waitForTimeout(300);
house = (await buildings()).find((b) => b.id === house.id);
const w0 = world(house);
const lengths = w0.map((p, i) =>
  Math.hypot(w0[(i + 1) % w0.length][0] - p[0], w0[(i + 1) % w0.length][1] - p[1]),
);
const k = lengths.indexOf(Math.max(...lengths));
const a = w0[k];
const b = w0[(k + 1) % w0.length];
const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
const t = await hook(() => window.__pv.plan.getState().transform);
const off = 14 / t.scale;
const mid = [
  (a[0] + b[0]) / 2 + ((b[1] - a[1]) / len) * off,
  (a[1] + b[1]) / 2 - ((b[0] - a[0]) / len) * off,
];
const [lx, ly] = await screen(mid);
await page.screenshot({ path: `${out}/e3-2-before-dimension.png` });
console.log(`dimension label expected at ${lx.toFixed(0)}, ${ly.toFixed(0)}`);
await page.mouse.dblclick(lx, ly);
const input = page.locator('.plan-view__dimension');
await input.waitFor();
await input.fill('15');
await input.press('Enter');
await waitFor(
  page,
  async () => {
    const h = (await buildings()).find((x) => x.id === house.id);
    const p = world(h);
    const q = p[(k + 1) % p.length];
    return Math.abs(Math.hypot(q[0] - p[k][0], q[1] - p[k][1]) - 15) < 1e-9;
  },
  'typed length',
);
check(true, `edge typed: ${len.toFixed(2)} → 15.00 m`);

// 7. Select a vertex, Delete removes it (not the building).
house = (await buildings()).find((b) => b.id === house.id);
const n0 = house.footprint.length;
await click(world(house)[2]);
await page.keyboard.press('Delete');
await waitFor(
  page,
  async () => (await buildings()).find((x) => x.id === house.id)?.footprint.length === n0 - 1,
  'delete vertex',
);
check((await buildings()).length === 2, 'Delete removed the vertex, not the building');
await page.waitForTimeout(600);
await page.screenshot({ path: `${out}/e3-2-edited.png` });

// 8. Undo every gesture (7 commands since the empty project), then redo them.
let undos = 0;
while ((await history()).canUndo) {
  await page.keyboard.press('Control+z');
  undos += 1;
  await page.waitForTimeout(250);
  if (undos > 20) break;
}
check(
  undos === 7 && (await buildings()).length === 0,
  `${undos} gestures undone, empty site again`,
);
for (let i = 0; i < undos; i++) {
  await page.keyboard.press('Control+y');
  await page.waitForTimeout(250);
}
await waitFor(page, async () => (await buildings()).length === 2, 'redo');
check(!(await history()).canRedo, 'all redone');
await page.waitForTimeout(600);
await page.screenshot({ path: `${out}/e3-2-redone.png` });

await browser.close();
console.log('E3.2 scenario passed');
